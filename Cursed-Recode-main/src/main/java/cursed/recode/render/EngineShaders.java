package cursed.recode.render;

import com.mojang.logging.LogUtils;
import cursed.recode.dev.Dev;
import java.nio.ByteBuffer;
import java.util.List;
import java.util.Map;
import org.jspecify.annotations.Nullable;
import org.lwjgl.system.MemoryUtil;
import org.lwjgl.util.shaderc.Shaderc;
import org.slf4j.Logger;

/**
 * The engine's terrain shaders in place of the game's (native/shaders/engine): their bodies are in the DLL; here the
 * version, the defines and the uniform blocks go in front — the game's, with the bindings the game gave them (their
 * place in the pipeline's list), then the engine's own after them — and shaderc makes SPIR-V of it. The pipeline is
 * then made with these shaders and the engine's uniforms added (CursedPipeline), which the DLL binds itself
 * (vk/pass.rs). CURSED_DEV_NOENGINE keeps the game's shaders.
 */
public final class EngineShaders {
    private static final Logger LOGGER = LogUtils.getLogger();
    public static final boolean OFF = Dev.on("CURSED_DEV_NOENGINE");
    /** The engine's uniforms, after the game's: name and kind (0 sampler, 1 uniform buffer). */
    static final List<Map.Entry<String, Integer>> EXTRA = List.of(
            Map.entry("CursedVoxels", 0), Map.entry("CursedBricks", 0), Map.entry("CursedPalette", 0), Map.entry("CursedEngine", 1),
            Map.entry("CursedMask", 0), Map.entry("CursedLight", 0), Map.entry("CursedBounce", 0), Map.entry("CursedWater", 0),
            Map.entry("CursedScene", 0), Map.entry("CursedSceneDepth", 0));
    /** The variants' numbers (vk/pipeline.rs): the depth pass of solid, of cutout (made with solid), the lit pass. */
    static final int PREPASS = 0;
    static final int PREPASS_CUTOUT = 1;
    static final int SHADE = 2;

    private static final Map<String, String> GAME = Map.of(
            "Fog", "uniform Fog {\n    vec4 FogColor;\n    float FogEnvironmentalStart;\n    float FogEnvironmentalEnd;\n    float FogRenderDistanceStart;\n    float FogRenderDistanceEnd;\n    float FogSkyEnd;\n    float FogCloudsEnd;\n};\n",
            "Globals", "uniform Globals {\n    ivec3 CameraBlockPos;\n    float GlintAlpha;\n    vec3 CameraOffset;\n    float GameTime;\n    vec2 ScreenSize;\n    int MenuBlurRadius;\n    int UseRgss;\n};\n",
            "Projection", "uniform Projection {\n    mat4 ProjMat;\n};\n",
            "TerrainUniform", "uniform TerrainUniform {\n    mat4 ModelViewMat;\n    ivec2 TextureSize;\n};\n",
            "ChunkSection", "uniform ChunkSection {\n    ivec3 ChunkPosition;\n    float ChunkVisibility;\n};\n",
            "Sampler0", "uniform sampler2D Sampler0;\n",
            "Sampler2", "uniform sampler2D Sampler2;\n");

    private EngineShaders() {
    }

    /** The shaders' SPIR-V (direct buffers the caller frees), vertex then fragment. */
    record Variant(int kind, ByteBuffer fragment) {
    }

    record Compiled(ByteBuffer vertex, ByteBuffer fragment, List<Variant> variants) {
        void free() {
            MemoryUtil.memFree(this.vertex);
            MemoryUtil.memFree(this.fragment);
            this.variants.forEach(v -> MemoryUtil.memFree(v.fragment()));
        }
    }

    /** Whether the pipeline is one the engine draws: the game's solid, cutout and translucent terrain. */
    static boolean wants(String pipeline) {
        if (OFF) {
            return false;
        }
        String p = pipeline.startsWith("minecraft:") ? pipeline.substring("minecraft:".length()) : pipeline;
        return switch (p) {
            case "pipeline/solid_terrain", "pipeline/solid_terrain_multidraw", "pipeline/cutout_terrain", "pipeline/cutout_terrain_multidraw",
                 "pipeline/translucent_terrain", "pipeline/translucent_terrain_multidraw" -> true;
            default -> false;
        };
    }

    /** The engine's shaders for this terrain pipeline, or null (the game's stay) when something is not as expected. */
    static @Nullable Compiled compile(long r, String pipeline, List<String> uniforms, boolean compact) {
        String p = pipeline.substring(pipeline.indexOf(':') + 1);
        StringBuilder version = new StringBuilder("#version 450\n");
        if (p.endsWith("_multidraw")) {
            version.append("#define MULTIDRAW_TERRAIN\n");
        }
        if (compact) {
            version.append("#define CURSED_COMPACT\n");
        }
        String cutout = p.startsWith("pipeline/cutout") ? "#define ALPHA_CUTOUT 0.5\n" : p.startsWith("pipeline/translucent") ? "#define ALPHA_CUTOUT 0.1\n#define CURSED_TRANSLUCENT\n" : "";
        StringBuilder head = new StringBuilder();
        int binding = 0;
        for (String name : uniforms) {
            String decl = GAME.get(name);
            if (decl == null) {
                LOGGER.warn("[engine] {} has a uniform the engine does not know ({}): the game's shaders stay", pipeline, name);
                return null;
            }
            head.append(decl.startsWith("uniform sampler") ? "layout(set = 0, binding = " + binding + ") " : "layout(std140, set = 0, binding = " + binding + ") ").append(decl);
            binding++;
        }
        head.append("layout(set = 0, binding = ").append(binding).append(") uniform usampler2DArray CursedVoxels;\n");
        head.append("layout(set = 0, binding = ").append(binding + 1).append(") uniform usampler2DArray CursedBricks;\n");
        head.append("layout(set = 0, binding = ").append(binding + 2).append(") uniform sampler2D CursedPalette;\n");
        head.append("layout(std140, set = 0, binding = ").append(binding + 3).append(") uniform CursedEngine {\n"
                + "    ivec4 CursedWindow;\n    ivec4 CursedSize;\n    vec4 CursedSun;\n    vec4 CursedSunColor;\n"
                + "    vec4 CursedMoon;\n    vec4 CursedSky;\n    vec4 CursedTime;\n    vec4 CursedSpare;\n    ivec4 CursedColumns[64];\n"
                + "    ivec4 CursedCameraBlock;\n    vec4 CursedCameraFrac;\n    vec4 CursedEntities[64];\n    vec4 CursedExtra;\n};\n#define CURSED_ENTITY_SHADOWS\n#define CURSED_CLOUD_SHADOWS\n");
        head.append("layout(set = 0, binding = ").append(binding + 4).append(") uniform sampler2D CursedMask;\n");
        head.append("layout(set = 0, binding = ").append(binding + 5).append(") uniform sampler3D CursedLight;\n");
        head.append("layout(set = 0, binding = ").append(binding + 6).append(") uniform sampler3D CursedBounce;\n");
        head.append("layout(set = 0, binding = ").append(binding + 7).append(") uniform sampler2D CursedWater;\n");
        head.append("layout(set = 0, binding = ").append(binding + 8).append(") uniform sampler2D CursedScene;\n");
        head.append("layout(set = 0, binding = ").append(binding + 9).append(") uniform sampler2D CursedSceneDepth;\n");
        String vertexBody = Native.text(r, 300);
        String fragmentBody = Native.text(r, 301);
        ByteBuffer v = spirv(pipeline + " (engine vertex)", version + cutout + head + vertexBody, Shaderc.shaderc_glsl_vertex_shader);
        if (v == null) {
            return null;
        }
        ByteBuffer f = spirv(pipeline + " (engine fragment)", version + cutout + head + fragmentBody, Shaderc.shaderc_glsl_fragment_shader);
        if (f == null) {
            MemoryUtil.memFree(v);
            return null;
        }
        // Opaque terrain: the depth pass (made with solid, for both layers) and the lit pass.
        List<Variant> variants = new java.util.ArrayList<>();
        boolean solid = p.startsWith("pipeline/solid");
        if (solid || p.startsWith("pipeline/cutout")) {
            String[][] wanted = solid
                    ? new String[][]{{"0", "#define CURSED_PREPASS\n"}, {"1", "#define CURSED_PREPASS\n#define ALPHA_CUTOUT 0.5\n"}, {"2", "#define CURSED_SHADE\n"}}
                    : new String[][]{{"2", cutout + "#define CURSED_SHADE\n"}};
            for (String[] w : wanted) {
                ByteBuffer spv = spirv(pipeline + " (engine variant " + w[0] + ")", version + w[1] + head + fragmentBody, Shaderc.shaderc_glsl_fragment_shader);
                if (spv == null) {
                    variants.forEach(x -> MemoryUtil.memFree(x.fragment()));
                    variants.clear();
                    break;
                }
                variants.add(new Variant(Integer.parseInt(w[0]), spv));
            }
        }
        return new Compiled(v, f, variants);
    }

    private static @Nullable ByteBuffer spirv(String name, String source, int kind) {
        long compiler = Shaderc.shaderc_compiler_initialize();
        long options = Shaderc.shaderc_compile_options_initialize();
        try {
            Shaderc.shaderc_compile_options_set_target_env(options, Shaderc.shaderc_target_env_vulkan, Shaderc.shaderc_env_version_vulkan_1_2);
            Shaderc.shaderc_compile_options_set_optimization_level(options, Shaderc.shaderc_optimization_level_performance);
            long result = Shaderc.shaderc_compile_into_spv(compiler, source, kind, name, "main", options);
            try {
                if (Shaderc.shaderc_result_get_compilation_status(result) != Shaderc.shaderc_compilation_status_success) {
                    LOGGER.error("[engine] {} failed, the game's shaders stay:\n{}", name, Shaderc.shaderc_result_get_error_message(result));
                    return null;
                }
                ByteBuffer spv = Shaderc.shaderc_result_get_bytes(result);
                ByteBuffer copy = MemoryUtil.memAlloc(spv.remaining());
                copy.put(spv).flip();
                return copy;
            } finally {
                Shaderc.shaderc_result_release(result);
            }
        } finally {
            Shaderc.shaderc_compile_options_release(options);
            Shaderc.shaderc_compiler_release(compiler);
        }
    }
}
