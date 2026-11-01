package cursed.recode.launch;

import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import net.minecraft.client.Minecraft;
import org.jspecify.annotations.Nullable;

/** The server joined last (the menu's «Continue»): its name and address, in <game dir>/cursed-recode/last-server. */
public final class LastServer {
    public record Entry(String name, String address) {
    }

    private static @Nullable Entry cached;
    private static boolean read;

    private LastServer() {
    }

    private static Path file() {
        return Minecraft.getInstance().gameDirectory.toPath().resolve("cursed-recode").resolve("last-server");
    }

    public static @Nullable Entry get() {
        if (!read) {
            read = true;
            try {
                Path f = file();
                if (Files.isRegularFile(f)) {
                    String[] lines = Files.readString(f, StandardCharsets.UTF_8).split("\n");
                    if (lines.length >= 2 && !lines[1].isBlank()) {
                        cached = new Entry(lines[0].strip(), lines[1].strip());
                    }
                }
            } catch (Exception ignored) {
                cached = null;
            }
        }
        return cached;
    }

    public static void remember(String name, String address) {
        cached = new Entry(name == null ? address : name, address);
        read = true;
        try {
            Path f = file();
            Files.createDirectories(f.getParent());
            Files.writeString(f, cached.name() + "\n" + cached.address() + "\n", StandardCharsets.UTF_8);
        } catch (Exception ignored) {
            // the menu only shows less
        }
    }
}
