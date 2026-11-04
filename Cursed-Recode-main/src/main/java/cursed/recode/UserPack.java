package cursed.recode;

import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;
import java.util.Optional;
import java.util.function.Consumer;
import net.fabricmc.loader.api.FabricLoader;
import net.minecraft.network.chat.Component;
import net.minecraft.server.packs.PackLocationInfo;
import net.minecraft.server.packs.PackSelectionConfig;
import net.minecraft.server.packs.PathPackResources;
import net.minecraft.server.packs.repository.Pack;
import net.minecraft.server.packs.repository.PackCompatibility;
import net.minecraft.server.packs.repository.PackSource;
import net.minecraft.world.flag.FeatureFlagSet;

/**
 * The player's own pack (the swords' models, cursed.recode.Swords): {@code <game>/cursed-recode/pack}, just
 * over the game's own and under any server's — a server's own swords (its pack's models) draw as the server
 * made them. The source is always in the repository (the folder may appear at any time); it gives a pack
 * only while the folder has something in it.
 */
public final class UserPack {
    public static final String ID = "cursed-recode-user";

    private UserPack() {
    }

    private static Path dir() {
        return FabricLoader.getInstance().getGameDir().toAbsolutePath().normalize().resolve("cursed-recode").resolve("pack");
    }

    /** Whether the pack is there (and the loader's switch is on). */
    static boolean exists() {
        return Swords.on() && Files.isDirectory(dir().resolve("assets"));
    }

    /** The pack, from the game's folder, when it is there. */
    public static void load(Consumer<Pack> out) {
        if (!exists()) {
            return;
        }
        Path root = dir();
        Component title = Component.literal("Cursed Recode — your models");
        PackLocationInfo info = new PackLocationInfo(ID, title, PackSource.BUILT_IN, Optional.empty());
        Pack.Metadata meta = new Pack.Metadata(title, PackCompatibility.COMPATIBLE, FeatureFlagSet.of(), List.of());
        out.accept(new Pack(info, new PathPackResources.PathResourcesSupplier(root), meta, new PackSelectionConfig(true, Pack.Position.BOTTOM, false)));
    }

    /** The selected packs with the player's own just over the game's own (a player's or a server's pack over it). */
    public static List<Pack> placed(List<Pack> selected) {
        if (!exists()) {
            return selected;
        }
        int own = -1;
        for (int i = 0; i < selected.size(); i++) {
            if (ID.equals(selected.get(i).getId())) {
                own = i;
                break;
            }
        }
        if (own < 0) {
            return selected;
        }
        java.util.List<Pack> list = new java.util.ArrayList<>(selected);
        Pack pack = list.remove(own);
        int at = 0;
        for (int i = 0; i < list.size(); i++) {
            if ("vanilla".equals(list.get(i).getId()) || OwnPack.ID.equals(list.get(i).getId())) {
                at = i + 1;
            }
        }
        list.add(at, pack);
        return List.copyOf(list);
    }
}
