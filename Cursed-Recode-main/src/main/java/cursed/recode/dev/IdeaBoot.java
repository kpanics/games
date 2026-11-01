package cursed.recode.dev;

import java.io.IOException;
import java.lang.management.ManagementFactory;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.stream.Stream;

/**
 * IntelliJ's «Minecraft Client» starts here and does around the game what run.cmd does: the launch's shaders
 * compiled when one changed (tools\spirv.cmd), the DLL and the programs built (cargo, its `dev-run` profile: release's
 * optimizations without the whole-crate LTO, so a change builds in seconds, not minutes), then the game started as
 * Cursed Recode is started for players — one program, CursedRecode.exe (native/src/bin/host.rs): the window before
 * the game up at once and the game's Java in the same process, so Windows shows «Cursed Recode» with her icon, not
 * java. It gets this JVM's options and class path, and its output comes here. IntelliJ itself only compiles the Java.
 *
 * The game no longer runs in IntelliJ's own JVM: to debug it, attach a remote debugger (add
 * -agentlib:jdwp=transport=dt_socket,server=y,suspend=n,address=5005 to the VM options; it is passed on).
 */
public final class IdeaBoot {
    private IdeaBoot() {
    }

    public static void main(String[] args) throws Throwable {
        Path run = Path.of("").toAbsolutePath();
        Path root = run.getParent();
        Path nat = root.resolve("native");
        if (stale(nat.resolve("shaders").resolve("launch"))) {
            exec(root, "cmd", "/c", root.resolve("tools").resolve("spirv.cmd").toString());
        }
        exec(nat, "cargo", "build", "--profile", "dev-run");
        Path out = nat.resolve("target").resolve("dev-run");
        Path host = out.resolve("CursedRecode.exe");
        List<String> command = new ArrayList<>();
        command.add(host.toString());
        command.add("--jdk");
        command.add(System.getProperty("java.home"));
        // the DLL of the same build (Native looks in target\release when it is not told)
        command.add("-Dcursed.recode.native=" + out.resolve("cursed_recode.dll"));
        // this JVM's own options (memory, stack, native access, exports, properties), not IntelliJ's agent
        for (String a : ManagementFactory.getRuntimeMXBean().getInputArguments()) {
            if (a.startsWith("-javaagent") || a.startsWith("-Didea.") || a.startsWith("-Dfile.encoding") || a.startsWith("-Dsun.stdout") || a.startsWith("-Dsun.stderr")) {
                continue;
            }
            command.add(a);
        }
        // the class path in a file: it is longer than a command line may be
        Path cp = Files.createTempFile("cursed-recode-cp", ".txt");
        cp.toFile().deleteOnExit();
        Files.writeString(cp, System.getProperty("java.class.path"));
        command.add("--cp-file");
        command.add(cp.toString());
        command.add("net.fabricmc.loader.impl.launch.knot.KnotClient");
        command.addAll(List.of(args));
        ProcessBuilder pb = new ProcessBuilder(command).directory(run.toFile()).inheritIO();
        // the window before the game's own log (its steps and frames, the farewell's), this start's and the last's
        Path twinLog = run.resolve("logs").resolve("twin.log");
        Files.createDirectories(twinLog.getParent());
        if (Files.exists(twinLog)) {
            Files.move(twinLog, twinLog.resolveSibling("twin.prev.log"), java.nio.file.StandardCopyOption.REPLACE_EXISTING);
        }
        pb.environment().put("CURSED_DEV_TWINLOG", twinLog.toString());
        Process p = pb.start();
        System.exit(p.waitFor());
    }

    /** Whether a launch shader's source is newer than its SPIR-V. */
    private static boolean stale(Path dir) throws IOException {
        if (!Files.isDirectory(dir.resolve("spv"))) {
            return true;
        }
        long sources, outputs;
        try (Stream<Path> s = Files.list(dir)) {
            sources = s.filter(p -> p.toString().endsWith(".comp") || p.toString().endsWith(".glsl")).mapToLong(IdeaBoot::modified).max().orElse(0);
        }
        try (Stream<Path> s = Files.list(dir.resolve("spv"))) {
            outputs = s.mapToLong(IdeaBoot::modified).min().orElse(0);
        }
        return sources > outputs;
    }

    private static long modified(Path p) {
        try {
            return Files.getLastModifiedTime(p).toMillis();
        } catch (IOException e) {
            return 0;
        }
    }

    private static void exec(Path dir, String... command) throws IOException, InterruptedException {
        int code = new ProcessBuilder(command).directory(dir.toFile()).inheritIO().start().waitFor();
        if (code != 0) {
            throw new IllegalStateException(String.join(" ", command) + " failed (" + code + ")");
        }
    }
}
