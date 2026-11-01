package cursed.recode.hud;

import java.util.List;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.screens.LevelLoadingScreen;
import net.minecraft.client.player.LocalPlayer;
import net.minecraft.core.HolderLookup;
import net.minecraft.world.item.CreativeModeTabs;
import net.minecraft.world.item.ItemStack;

/**
 * The inventory opens at once, the first time too: what the game would make at that moment is made while the world's
 * loading screen is still up (her word: the inventory, the creative one above all, sometimes opens with a hitch).
 *
 * <p>The creative tabs' items (the game builds them when the creative inventory first opens: ~90 ms of the frame) and
 * the search's lists from them, as the creative screen does when it builds them; the classes the inventories load
 * when first opened (the recipe book's, the creative screen's: ~20 ms). Built again whenever what they are built for
 * changes while the screen is up (the permissions come a moment after the world).
 */
public final class Warmup {
    /** The classes the inventories load on their first opening (from the stand's class log, 22.10). */
    private static final String[] CLASSES = {
            "net.minecraft.client.gui.ItemSlotMouseAction",
            "net.minecraft.client.gui.screens.recipebook.RecipeBookComponent",
            "net.minecraft.client.gui.screens.recipebook.CraftingRecipeBookComponent",
            "net.minecraft.client.gui.components.WidgetSprites",
            "net.minecraft.client.gui.screens.recipebook.RecipeBookComponent$TabInfo",
            "net.minecraft.world.entity.player.StackedItemContents",
            "net.minecraft.world.entity.player.StackedContents",
            "net.minecraft.client.gui.screens.recipebook.SlotSelectTime",
            "net.minecraft.client.gui.screens.recipebook.GhostSlots",
            "net.minecraft.client.gui.screens.recipebook.RecipeBookPage",
            "net.minecraft.client.gui.screens.recipebook.OverlayRecipeComponent",
            "net.minecraft.client.gui.screens.recipebook.RecipeButton",
            "net.minecraft.client.gui.screens.inventory.EffectsInInventory",
            "net.minecraft.client.gui.BundleMouseActions",
            "net.minecraft.client.gui.components.Button$CreateNarration",
            "net.minecraft.client.gui.components.Button$OnPress",
            "net.minecraft.client.model.AnimationUtils",
            "net.minecraft.client.gui.screens.inventory.CreativeModeInventoryScreen$ItemPickerMenu",
            "net.minecraft.client.gui.screens.inventory.CreativeModeInventoryScreen$CustomCreativeSlot",
            "net.minecraft.client.gui.screens.inventory.CreativeInventoryListener",
            "net.minecraft.client.renderer.SpriteCoordinateExpander",
            "net.minecraft.util.NullOps",
            "cursed.recode.hud.HudRecipes",
            "cursed.recode.hud.HudCreative",
            "cursed.recode.menus.ItemStage$Frame",
    };
    private static boolean loaded;

    private Warmup() {
    }

    /** Each tick (MinecraftMixin): while a world's loading screen is up, what the inventories need made. */
    public static void tick(Minecraft mc) {
        LocalPlayer p = mc.player;
        if (p == null || mc.level == null || !(mc.gui.screen() instanceof LevelLoadingScreen)) {
            return;
        }
        if (!loaded) {
            loaded = true;
            ClassLoader cl = Warmup.class.getClassLoader();
            for (String name : CLASSES) {
                try {
                    Class.forName(name, false, cl);
                } catch (Throwable ignored) {
                    // a class of another version: nothing to warm
                }
            }
        }
        // as CreativeModeInventoryScreen does: the tabs for these features, permissions and registries, then the
        // search's lists (only when they were built anew: the screen will find them built and not build them again)
        boolean perms = p.canUseGameMasterBlocks() && mc.options.operatorItemsTab().get();
        HolderLookup.Provider registries = p.level().registryAccess();
        if (CreativeModeTabs.tryRebuildTabContents(p.connection.enabledFeatures(), perms, registries)) {
            List<ItemStack> all = List.copyOf(CreativeModeTabs.searchTab().getDisplayItems());
            p.connection.searchTrees().updateCreativeTooltips(registries, all);
            p.connection.searchTrees().updateCreativeTags(all);
        }
    }
}
