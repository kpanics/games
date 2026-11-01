import java.io.IOException;
import java.nio.ByteBuffer;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import org.lwjgl.util.shaderc.Shaderc;

/**
 * Compiles the launch's compute shaders (native/shaders/launch) and the menu's (native/shaders/gui) into SPIR-V the
 * DLL carries (launch/mod.rs, gui/render.rs): the window before the game draws them long before the game's own shader
 * compiler is up. Same shaderc as the game's. Every shader is "#version 450", its folder's head (launch.glsl,
 * gui.glsl), then its file; scene.glsl twice, without and with the world.
 *
 *   tools\spirv.cmd      (or: java -cp "libs\mc\*" tools\Spirv.java native\shaders\launch, then native\shaders\gui)
 */
public final class Spirv {
    public static void main(String[] args) throws IOException {
        Path dir = Path.of(args.length > 0 ? args[0] : "native/shaders/launch");
        Path out = dir.resolve("spv");
        Files.createDirectories(out);
        // the head is the folder's own .glsl (launch.glsl, gui.glsl)
        String head = Files.readString(dir.resolve(dir.toAbsolutePath().normalize().getFileName() + ".glsl"));
        List<String[]> jobs = new ArrayList<>();   // name, defines, file
        if (Files.exists(dir.resolve("scene.glsl"))) {
            jobs.add(new String[] {"scene_sky", "#define WORLD 0\n", "scene.glsl"});
            jobs.add(new String[] {"scene_world", "#define WORLD 1\n", "scene.glsl"});
        }
        try (var files = Files.list(dir)) {
            files.filter(f -> f.toString().endsWith(".comp")).sorted().forEach(f -> {
                String n = f.getFileName().toString();
                jobs.add(new String[] {n.substring(0, n.length() - 5), "", n});
            });
        }
        long compiler = Shaderc.shaderc_compiler_initialize();
        long options = Shaderc.shaderc_compile_options_initialize();
        Shaderc.shaderc_compile_options_set_target_env(options, Shaderc.shaderc_target_env_vulkan, Shaderc.shaderc_env_version_vulkan_1_2);
        Shaderc.shaderc_compile_options_set_optimization_level(options, Shaderc.shaderc_optimization_level_performance);
        int failed = 0;
        for (String[] job : jobs) {
            String body = Files.readString(dir.resolve(job[2]));
            // a file's own #define lines stay first, before the shared head
            String source = "#version 450\n" + job[1] + head + "\n#line 1\n" + body;
            if (body.startsWith("#define")) {
                int eol = body.indexOf('\n');
                source = "#version 450\n" + job[1] + body.substring(0, eol + 1) + head + "\n#line 2\n" + body.substring(eol + 1);
            }
            long result = Shaderc.shaderc_compile_into_spv(compiler, source, Shaderc.shaderc_glsl_compute_shader, job[2], "main", options);
            try {
                if (Shaderc.shaderc_result_get_compilation_status(result) != Shaderc.shaderc_compilation_status_success) {
                    System.err.println("[spirv] " + job[0] + ": " + Shaderc.shaderc_result_get_error_message(result));
                    failed++;
                    continue;
                }
                ByteBuffer spv = Shaderc.shaderc_result_get_bytes(result);
                byte[] code = new byte[spv.remaining()];
                spv.get(code);
                Files.write(out.resolve(job[0] + ".spv"), code);
                System.out.println("[spirv] " + job[0] + " " + code.length + " bytes");
            } finally {
                Shaderc.shaderc_result_release(result);
            }
        }
        Shaderc.shaderc_compile_options_release(options);
        Shaderc.shaderc_compiler_release(compiler);
        if (failed > 0) {
            System.exit(1);
        }
    }
}
