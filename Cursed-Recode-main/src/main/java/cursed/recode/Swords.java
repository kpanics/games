package cursed.recode;

import com.google.gson.Gson;
import com.google.gson.JsonObject;
import com.mojang.logging.LogUtils;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;
import java.util.Comparator;
import java.util.stream.Stream;
import net.fabricmc.loader.api.FabricLoader;
import net.minecraft.client.Minecraft;
import org.slf4j.Logger;

/**
 * The swords' models (native/src/gui/swords.rs): a custom 3D model of the player's in place of the game's
 * swords. The chosen model is written into {@code <game>/cursed-recode/pack} — a pack of the game's own,
 * just over the game's and under any server's (cursed.recode.UserPack) — and the resources reload: the
 * menu's «sword&nbsp;&lt;name&gt;» ask (GuiAsks), «Vanilla» putting the game's own swords back. The model
 * itself and its pictures live in {@code <game>/cursed-recode/swords/<name>/} (model.json, textures/),
 * untouched; the pack only points the game's swords at them.
 */
public final class Swords {
    private static final Logger LOGGER = LogUtils.getLogger();
    private static final Gson GSON = new Gson();
    /** The game's swords a model takes the place of. */
    private static final String[] SWORDS = {"wooden_sword", "stone_sword", "iron_sword", "golden_sword", "diamond_sword", "netherite_sword"};

    private Swords() {
    }

    /** The loader's switch (CURSED_SWORDS=0): the whole thing off — the game's own swords. */
    static boolean on() {
        return !"0".equals(System.getenv("CURSED_SWORDS"));
    }

    private static Path base() {
        return FabricLoader.getInstance().getGameDir().toAbsolutePath().normalize().resolve("cursed-recode");
    }

    /** A model chosen in the menu («Vanilla»: the game's own swords back): the pack written — in the background, for
     * writing files is not the game's thread's work — and the resources reloaded on the game's thread after. */
    public static void apply(Minecraft mc, String name) {
        if (!on()) {
            return;
        }
        String pick = name.isEmpty() || name.equals("Vanilla") ? "Vanilla" : name;
        Thread worker = new Thread(() -> {
            synchronized (Swords.class) {
                try {
                    Path pack = base().resolve("pack");
                    if (pick.equals("Vanilla")) {
                        delete(pack);
                        write(base().resolve("swords.txt"), "Vanilla");
                    } else {
                        Path model = base().resolve("swords").resolve(pick);
                        if (!Files.isRegularFile(model.resolve("model.json"))) {
                            LOGGER.warn("[swords] {} has no model.json: the game's own swords", model);
                            delete(pack);
                            write(base().resolve("swords.txt"), "Vanilla");
                        } else {
                            make(pack, pick, model);
                            write(base().resolve("swords.txt"), pick);
                        }
                    }
                } catch (IOException e) {
                    LOGGER.error("[swords] the pack", e);
                }
            }
            // the reload is the game's own (its thread); scheduled from here, it runs on the game's loop
            mc.schedule(mc::reloadResourcePacks);
        }, "Cursed swords");
        worker.setDaemon(true);
        worker.start();
    }

    /** The pack of one model: the game's swords point at it, it and its pictures under the client's own name. */
    private static void make(Path pack, String name, Path model) throws IOException {
        delete(pack);
        // the game's swords point at the model
        for (String sword : SWORDS) {
            Path at = pack.resolve("assets/minecraft/items").resolve(sword + ".json");
            Files.createDirectories(at.getParent());
            Files.writeString(at, "{\"model\":{\"type\":\"minecraft:model\",\"model\":\"cursed-recode:sword/" + name + "\"}}");
        }
        // the model itself; without a pose of its own the game's handheld holding (the parent's transforms)
        JsonObject m = GSON.fromJson(Files.readString(model.resolve("model.json")), JsonObject.class);
        if (m != null) {
            if (!m.has("parent") && !m.has("display")) {
                m.addProperty("parent", "minecraft:item/handheld");
            }
            Path at = pack.resolve("assets/cursed-recode/models/sword").resolve(name + ".json");
            Files.createDirectories(at.getParent());
            Files.writeString(at, m.toString());
        }
        // its pictures
        Path textures = model.resolve("textures");
        if (Files.isDirectory(textures)) {
            try (Stream<Path> files = Files.walk(textures)) {
                files.filter(Files::isRegularFile).forEach(p -> {
                    Path to = pack.resolve("assets/cursed-recode/textures/sword").resolve(textures.relativize(p).toString());
                    try {
                        Files.createDirectories(to.getParent());
                        Files.copy(p, to, StandardCopyOption.REPLACE_EXISTING);
                    } catch (IOException e) {
                        LOGGER.warn("[swords] {}: {}", p, e.toString());
                    }
                });
            }
        }
    }

    private static void write(Path at, String what) throws IOException {
        Files.createDirectories(at.getParent());
        Files.writeString(at, what);
    }

    private static void delete(Path dir) {
        if (!Files.exists(dir)) {
            return;
        }
        try (Stream<Path> walk = Files.walk(dir)) {
            walk.sorted(Comparator.reverseOrder()).forEach(p -> {
                try {
                    Files.delete(p);
                } catch (IOException e) {
                    LOGGER.warn("[swords] {} left: {}", p, e.toString());
                }
            });
        } catch (IOException e) {
            LOGGER.warn("[swords] {} left: {}", dir, e.toString());
        }
    }
}
