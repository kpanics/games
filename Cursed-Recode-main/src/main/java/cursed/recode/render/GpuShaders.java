package cursed.recode.render;

import com.mojang.logging.LogUtils;
import cursed.recode.dev.Dev;
import java.io.ByteArrayOutputStream;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import org.lwjgl.system.MemoryUtil;
import org.lwjgl.util.shaderc.Shaderc;
import org.slf4j.Logger;

/**
 * The renderer's own compute shaders (native/src/vk/shaders.rs: entity vertices, terrain occlusion): their GLSL is in
 * the DLL, compiled here with the game's shaderc and handed back as SPIR-V, once, on the render thread. Until then —
 * or when it fails — models stay on the CPU and every terrain section is drawn.
 * CURSED_DEV_NOOCCLUSION leaves terrain occlusion culling off.
 */
public final class GpuShaders {
    private static final Logger LOGGER = LogUtils.getLogger();
    private static final boolean NO_OCCLUSION = Dev.on("CURSED_DEV_NOOCCLUSION");
    /** 0 not tried yet, 1 made, -1 failed. */
    private static int state;

    private GpuShaders() {
    }

    public static boolean ready() {
        if (state == 0) {
            init();
        }
        return state > 0;
    }

    private static void init() {
        CursedDevice d = CursedDevice.current;
        if (d == null) {
            return;
        }
        state = -1;
        ByteArrayOutputStream all = new ByteArrayOutputStream();
        long compiler = Shaderc.shaderc_compiler_initialize();
        long options = Shaderc.shaderc_compile_options_initialize();
        try {
            Shaderc.shaderc_compile_options_set_target_env(options, Shaderc.shaderc_target_env_vulkan, Shaderc.shaderc_env_version_vulkan_1_2);
            Shaderc.shaderc_compile_options_set_optimization_level(options, Shaderc.shaderc_optimization_level_zero);
            for (int i = 0; ; i++) {
                String source = Native.text(d.r, 100 + i);
                if (source.isEmpty()) {
                    break;
                }
                // compute shaders by default; a name ending .vert or .frag is a vertex or fragment shader
                String base = Native.text(d.r, 200 + i);
                int kind = base.endsWith(".vert") ? Shaderc.shaderc_glsl_vertex_shader
                        : base.endsWith(".frag") ? Shaderc.shaderc_glsl_fragment_shader : Shaderc.shaderc_glsl_compute_shader;
                String name = "cursed:" + base + (kind == Shaderc.shaderc_glsl_compute_shader ? ".comp" : "");
                long result = Shaderc.shaderc_compile_into_spv(compiler, source, kind, name, "main", options);
                try {
                    if (Shaderc.shaderc_result_get_compilation_status(result) != Shaderc.shaderc_compilation_status_success) {
                        throw new IllegalStateException(name + ": " + Shaderc.shaderc_result_get_error_message(result));
                    }
                    ByteBuffer spv = Shaderc.shaderc_result_get_bytes(result);
                    int n = spv.remaining();
                    ByteBuffer head = ByteBuffer.allocate(8).order(ByteOrder.LITTLE_ENDIAN).putInt(i).putInt(n);
                    all.writeBytes(head.array());
                    byte[] code = new byte[(n + 3) & ~3];
                    spv.get(code, 0, n);
                    all.writeBytes(code);
                } finally {
                    Shaderc.shaderc_result_release(result);
                }
            }
            byte[] bytes = all.toByteArray();
            ByteBuffer data = MemoryUtil.memAlloc(bytes.length);
            try {
                data.put(bytes).flip();
                Native.shadersInit(d.r, MemoryUtil.memAddress(data), bytes.length, (GpuModels.CHECK ? 1 : 0) | (NO_OCCLUSION ? 2 : 0));
            } finally {
                MemoryUtil.memFree(data);
            }
            state = 1;
            LOGGER.info("Compute shaders made: entity vertices on the GPU{}", NO_OCCLUSION ? "" : ", terrain occlusion culling");
        } catch (Throwable t) {
            LOGGER.error("The renderer's compute shaders failed: models stay on the CPU, no occlusion culling", t);
        } finally {
            Shaderc.shaderc_compile_options_release(options);
            Shaderc.shaderc_compiler_release(compiler);
        }
    }
}
