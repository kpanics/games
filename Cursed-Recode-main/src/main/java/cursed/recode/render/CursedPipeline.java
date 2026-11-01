package cursed.recode.render;

import com.mojang.renderpearl.api.pipeline.BindGroupLayout;
import com.mojang.renderpearl.api.pipeline.BlendFunction;
import com.mojang.renderpearl.api.pipeline.ColorTargetState;
import com.mojang.renderpearl.api.pipeline.DepthStencilState;
import com.mojang.renderpearl.api.pipeline.UniformType;
import com.mojang.renderpearl.backend.api.BackendRenderPipeline;
import com.mojang.renderpearl.backend.api.SpvModule;
import java.nio.ByteBuffer;
import java.util.ArrayList;
import java.util.List;
import org.lwjgl.system.MemoryUtil;

/** A compiled pipeline (vk/pipeline.rs Pipeline). */
final class CursedPipeline implements BackendRenderPipeline {
    private final CursedDevice device;
    final long handle;
    private boolean closed;

    private CursedPipeline(CursedDevice device, long handle) {
        this.device = device;
        this.handle = handle;
    }

    static long handle(BackendRenderPipeline pipeline) {
        return ((CursedPipeline) pipeline).handle;
    }

    /** Compiles on the calling thread (the game's worker): the DLL reads the description written here. */
    static CursedPipeline compile(CursedDevice device, BackendRenderPipeline.CreateInfo info) {
        Pack p = new Pack(1024);
        // SPIR-V the game kept on the Java heap is copied to native memory for the call.
        List<ByteBuffer> copies = new ArrayList<>();
        EngineShaders.Compiled engine = null;
        try {
            // Terrain the engine draws: its own shaders, the game's uniforms in the same places, its own after them.
            if (EngineShaders.wants(info.name())) {
                boolean compactVertex = false;
                for (BackendRenderPipeline.CreateInfo.Shader shader : info.shaders()) {
                    if (TerrainCompact.SHADER.equals(shader.name()) && shader.module().type() == com.mojang.renderpearl.api.pipeline.ShaderType.VERTEX) {
                        compactVertex = TerrainCompact.isPatched(shader.module());
                    }
                }
                List<String> names = new ArrayList<>();
                for (BindGroupLayout.UniformDescription u : info.uniforms()) {
                    names.add(u.name());
                }
                engine = EngineShaders.compile(device.r, info.name(), names, compactVertex);
            }
            // Creatures and things with the engine's light: both shaders' lit twins, bound to the same places.
            java.util.List<SpvModule> twins = null;
            if (engine == null) {
                java.util.List<SpvModule> found = new ArrayList<>();
                int twinned = 0;
                for (BackendRenderPipeline.CreateInfo.Shader shader : info.shaders()) {
                    SpvModule twin = EntityEngine.twin(shader.module());
                    // A shader without a twin keeps the game's, unless it is a creature's (both or neither).
                    if (twin == null && !EntityEngine.CLOUDS.equals(shader.name())) {
                        found = null;
                        break;
                    }
                    found.add(twin != null ? twin : shader.module());
                    twinned += twin != null ? 1 : 0;
                }
                if (found != null && twinned > 0) {
                    List<String> names = new ArrayList<>();
                    for (BindGroupLayout.UniformDescription u : info.uniforms()) {
                        names.add(u.name());
                    }
                    boolean ok = true;
                    for (SpvModule twin : found) {
                        if (EntityEngine.isTwin(twin)) {
                            ok &= EntityEngine.bind(twin, names);
                        }
                    }
                    twins = ok ? found : null;
                    if (twins != null) {
                        cursed.recode.CursedRecode.LOGGER.info("[engine] {} lit by the engine", info.name());
                    }
                }
            }
            p.str(info.name());
            p.i32(info.shaders().size());
            int shaderIndex = 0;
            for (BackendRenderPipeline.CreateInfo.Shader shader : info.shaders()) {
                if (twins != null) {
                    ByteBuffer spv = twins.get(shaderIndex++).spv();
                    if (!spv.isDirect()) {
                        ByteBuffer copy = MemoryUtil.memAlloc(spv.remaining());
                        copy.put(spv.duplicate()).flip();
                        copies.add(copy);
                        spv = copy;
                    }
                    p.i32(VkMap.stage(shader.module().type())).str("main").i64(MemoryUtil.memAddress(spv)).i32(spv.remaining());
                    continue;
                }
                if (engine != null) {
                    boolean vertex = shader.module().type() == com.mojang.renderpearl.api.pipeline.ShaderType.VERTEX;
                    ByteBuffer spv = vertex ? engine.vertex() : engine.fragment();
                    p.i32(VkMap.stage(shader.module().type())).str("main").i64(MemoryUtil.memAddress(spv)).i32(spv.remaining());
                    continue;
                }
                ByteBuffer spv = shader.module().spv();
                if (!spv.isDirect()) {
                    ByteBuffer copy = MemoryUtil.memAlloc(spv.remaining());
                    copy.put(spv.duplicate()).flip();
                    copies.add(copy);
                    spv = copy;
                }
                p.i32(VkMap.stage(shader.module().type())).str(shader.entryPoint()).i64(MemoryUtil.memAddress(spv)).i32(spv.remaining());
            }
            // Terrain with the patched vertex shader reads the packed vertices (TerrainCompact): slot 0 is 16 bytes.
            boolean terrain = false;
            boolean compact = false;
            for (BackendRenderPipeline.CreateInfo.Shader shader : info.shaders()) {
                // The fragment shader has the same name; only the vertex shader reads the vertices.
                if (TerrainCompact.SHADER.equals(shader.name()) && shader.module().type() == com.mojang.renderpearl.api.pipeline.ShaderType.VERTEX) {
                    terrain = true;
                    compact = TerrainCompact.isPatched(shader.module());
                }
            }
            p.i32(info.vertexBuffers().size());
            for (BackendRenderPipeline.CreateInfo.VertexBuffer vb : info.vertexBuffers()) {
                p.i32(vb.bufferSlot()).i32(compact && vb.bufferSlot() == 0 ? TerrainCompact.STRIDE : vb.stride()).i32(vb.stepRate());
            }
            p.i32(info.attribBindings().size());
            for (BackendRenderPipeline.CreateInfo.AttribBinding a : info.attribBindings()) {
                int offset = a.offset();
                int format = VkMap.format(a.format());
                if (compact && a.bufferSlot() == 0) {
                    // DefaultVertexFormat.BLOCK: Position 0, Color 12, UV0 16, UV2 24. The position reads four
                    // components (the fourth is the light's two bytes, unused there): a three-component 16-bit
                    // format need not be supported for vertices.
                    switch (offset) {
                        case 0 -> format = 91; // VK_FORMAT_R16G16B16A16_UNORM
                        case 12 -> offset = 8;
                        case 16 -> {
                            offset = 12;
                            format = 77; // VK_FORMAT_R16G16_UNORM
                        }
                        case 24 -> {
                            offset = 6;
                            format = 21; // VK_FORMAT_R8G8_SINT (the shader takes the low eight bits)
                        }
                        default -> throw new IllegalStateException("Unexpected terrain attribute at " + offset);
                    }
                }
                p.i32(a.bufferSlot()).i32(a.location()).i32(offset).i32(format);
            }
            p.i32(info.uniforms().size() + (engine != null || twins != null ? EngineShaders.EXTRA.size() : 0));
            for (BindGroupLayout.UniformDescription u : info.uniforms()) {
                int kind = switch (u.type()) {
                    case COMBINED_IMAGE_SAMPLER -> 0;
                    case UNIFORM_BUFFER -> 1;
                    case TEXEL_BUFFER -> 2;
                };
                p.str(u.name()).i32(kind).i32(u.type() == UniformType.TEXEL_BUFFER && u.gpuFormat() != null ? VkMap.format(u.gpuFormat()) : 0);
            }
            if (engine != null || twins != null) {
                for (java.util.Map.Entry<String, Integer> e : EngineShaders.EXTRA) {
                    p.str(e.getKey()).i32(e.getValue()).i32(0);
                }
            }
            p.i32(info.pushConstantsSize());
            DepthStencilState depth = info.depthStencilState();
            if (depth != null) {
                boolean write = depth.writeDepth() && !CheapOit.keepsDepth(info.name());
                p.i32(1).i32(VkMap.compareOp(depth.depthTest())).i32(write ? 1 : 0).f32(depth.depthBiasScaleFactor()).f32(depth.depthBiasConstant());
            } else {
                p.i32(0);
            }
            p.i32(VkMap.polygonMode(info.polygonMode())).i32(info.cull() ? 1 : 0);
            p.i32(info.colorTargetStates().size());
            for (ColorTargetState target : info.colorTargetStates()) {
                if (target == null) {
                    p.i32(0).i32(0).i32(0).i32(0).i32(0).i32(0).i32(0).i32(0).i32(0).i32(0);
                    continue;
                }
                int mask = (target.writeRed() ? 1 : 0) | (target.writeGreen() ? 2 : 0) | (target.writeBlue() ? 4 : 0) | (target.writeAlpha() ? 8 : 0);
                p.i32(1).i32(VkMap.format(target.format())).i32(mask);
                if (target.blendFunction().isPresent()) {
                    BlendFunction f = target.blendFunction().get();
                    p.i32(1)
                            .i32(VkMap.blendFactor(f.color().sourceFactor()))
                            .i32(VkMap.blendFactor(f.color().destFactor()))
                            .i32(VkMap.blendOp(f.color().op()))
                            .i32(VkMap.blendFactor(f.alpha().sourceFactor()))
                            .i32(VkMap.blendFactor(f.alpha().destFactor()))
                            .i32(VkMap.blendOp(f.alpha().op()));
                } else {
                    p.i32(0).i32(0).i32(0).i32(0).i32(0).i32(0).i32(0);
                }
            }
            p.i32(VkMap.topology(info.primitiveTopology()));
            if (engine != null || twins != null) {
                // The engine's variants, then the game's own shaders (3 vertex, 4 fragment) for frames the engine is off.
                List<EngineShaders.Variant> variants = engine != null ? engine.variants() : List.of();
                p.i32(variants.size() + info.shaders().size());
                for (EngineShaders.Variant v : variants) {
                    p.i32(v.kind()).i64(MemoryUtil.memAddress(v.fragment())).i32(v.fragment().remaining());
                }
                for (BackendRenderPipeline.CreateInfo.Shader shader : info.shaders()) {
                    ByteBuffer spv = shader.module().spv();
                    if (!spv.isDirect()) {
                        ByteBuffer copy = MemoryUtil.memAlloc(spv.remaining());
                        copy.put(spv.duplicate()).flip();
                        copies.add(copy);
                        spv = copy;
                    }
                    boolean vertex = shader.module().type() == com.mojang.renderpearl.api.pipeline.ShaderType.VERTEX;
                    p.i32(vertex ? 3 : 4).i64(MemoryUtil.memAddress(spv)).i32(spv.remaining());
                }
            }
            if (terrain) {
                if (!compact && !TerrainCompact.OFF) {
                    cursed.recode.CursedRecode.LOGGER.warn("[terrain compact] {}: the terrain shader is not the expected one, terrain stays unpacked", info.name());
                }
                Native.terrainCompact(device.r, compact);
            }
            return new CursedPipeline(device, Native.pipelineCreate(device.r, p.address(), p.length()));
        } finally {
            p.free();
            copies.forEach(MemoryUtil::memFree);
            if (engine != null) {
                engine.free();
            }
        }
    }

    @Override
    public boolean isClosed() {
        return this.closed;
    }

    @Override
    public void close() {
        if (!this.closed) {
            this.closed = true;
            Native.pipelineClose(this.device.r, this.handle);
        }
    }
}
