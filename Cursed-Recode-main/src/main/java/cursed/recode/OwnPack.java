package cursed.recode;

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
 * The client's own resources (its assets: the words of its settings, its pictures, its changes to the game's words) as
 * a pack of the game's, always on, just over the game's own (a player's or a server's pack goes over it). Nothing else
 * gives them to the game: the client is a mod with no Fabric API (whose resource loader would) — on the test stand,
 * with Fabric API on its classpath, Fabric's own pack does it and this one stays out.
 */
public final class OwnPack {
    public static final String ID = "cursed-recode";

    private OwnPack() {
    }

    /** Whether the client gives its resources itself (Fabric API's resource loader is not there). */
    public static boolean wanted() {
        return !FabricLoader.getInstance().isModLoaded("fabric-resource-loader-v0") && !FabricLoader.getInstance().isModLoaded("fabric-resource-loader-v1");
    }

    /** The pack, from where the client is (its jar, or its classes' folder). */
    public static void load(Consumer<Pack> out) {
        Path root = FabricLoader.getInstance().getModContainer(ID).flatMap(m -> m.getRootPaths().stream().findFirst()).orElse(null);
        if (root == null) {
            CursedRecode.LOGGER.warn("[own pack] where the client is is not known: its words and pictures are not there");
            return;
        }
        Component title = Component.literal("Cursed Recode");
        PackLocationInfo info = new PackLocationInfo(ID, title, PackSource.BUILT_IN, Optional.empty());
        Pack.Metadata meta = new Pack.Metadata(title, PackCompatibility.COMPATIBLE, FeatureFlagSet.of(), List.of());
        out.accept(new Pack(info, new PathPackResources.PathResourcesSupplier(root), meta, new PackSelectionConfig(true, Pack.Position.BOTTOM, false)));
    }

    /** The selected packs with the client's just over the game's own (its words over the game's, a player's or a
     *  server's pack over it). */
    public static List<Pack> placed(List<Pack> selected) {
        int own = -1;
        int vanilla = -1;
        for (int i = 0; i < selected.size(); i++) {
            String id = selected.get(i).getId();
            if (ID.equals(id)) {
                own = i;
            } else if ("vanilla".equals(id)) {
                vanilla = i;
            }
        }
        if (own < 0 || own == vanilla + 1) {
            return selected;
        }
        List<Pack> list = new java.util.ArrayList<>(selected);
        Pack pack = list.remove(own);
        int at = 0;
        for (int i = 0; i < list.size(); i++) {
            if ("vanilla".equals(list.get(i).getId())) {
                at = i + 1;
            }
        }
        list.add(at, pack);
        return List.copyOf(list);
    }
}
