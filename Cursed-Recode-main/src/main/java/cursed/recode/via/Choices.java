package cursed.recode.via;

import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Locale;
import java.util.Map;
import java.util.TreeMap;
import java.util.concurrent.ConcurrentHashMap;
import net.minecraft.client.Minecraft;

/**
 * The version chosen by hand for a server (its address as the list has it): its protocol, 0 for «as it says». Kept in
 * cursed-recode/via.txt, a line each: the address, a tab, the protocol.
 */
public final class Choices {
    private static final Map<String, Integer> CHOSEN = new ConcurrentHashMap<>();
    private static boolean loaded;
    /** The file could not be read: it is not written over (what it keeps would be lost). */
    private static boolean unreadable;

    private Choices() {
    }

    private static String key(String address) {
        return address.trim().toLowerCase(Locale.ROOT);
    }

    private static Path file() {
        return Minecraft.getInstance().gameDirectory.toPath().resolve("cursed-recode").resolve("via.txt");
    }

    private static synchronized void load() {
        if (loaded) {
            return;
        }
        loaded = true;
        try {
            Path f = file();
            if (!Files.exists(f)) {
                return;
            }
            for (String line : Files.readAllLines(f, StandardCharsets.UTF_8)) {
                String[] p = line.split("\t", 2);
                if (p.length == 2) {
                    try {
                        int protocol = Integer.parseInt(p[1].trim());
                        if (protocol > 0) {
                            CHOSEN.put(key(p[0]), protocol);
                        }
                    } catch (NumberFormatException ignored) {
                    }
                }
            }
        } catch (Exception e) {
            unreadable = true;
            cursed.recode.CursedRecode.LOGGER.warn("Via: the chosen versions could not be read", e);
        }
    }

    private static synchronized void save() {
        if (unreadable) {
            return;
        }
        try {
            Path f = file();
            Files.createDirectories(f.getParent());
            StringBuilder b = new StringBuilder();
            for (var e : new TreeMap<>(CHOSEN).entrySet()) {
                b.append(e.getKey()).append('\t').append(e.getValue()).append('\n');
            }
            Files.writeString(f, b.toString(), StandardCharsets.UTF_8);
        } catch (Exception e) {
            cursed.recode.CursedRecode.LOGGER.warn("Via: the chosen versions could not be saved", e);
        }
    }

    public static int of(String address) {
        if (address == null) {
            return 0;
        }
        load();
        return CHOSEN.getOrDefault(key(address), 0);
    }

    public static void choose(String address, int protocol) {
        if (address == null) {
            return;
        }
        load();
        Compatibility.reset(address);
        if (protocol > 0) {
            CHOSEN.put(key(address), protocol);
        } else {
            CHOSEN.remove(key(address));
        }
        save();
    }
}
