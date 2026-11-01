package cursed.recode.mixin.client;

import net.minecraft.client.gui.screens.inventory.AbstractRecipeBookScreen;
import net.minecraft.client.gui.screens.recipebook.RecipeBookComponent;
import net.minecraft.client.gui.screens.recipebook.RecipeCollection;
import net.minecraft.world.entity.player.StackedItemContents;
import net.minecraft.world.inventory.RecipeBookMenu;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Accessor;
import org.spongepowered.asm.mixin.gen.Invoker;

import java.util.List;

/** Custom Inventory's recipe book (cursed.recode.hud.HudRecipes): what the game's own book keeps to itself. */
public final class RecipesAccess {
    private RecipesAccess() {
    }

    /** A screen's book. */
    @Mixin(AbstractRecipeBookScreen.class)
    public interface Screen {
        @Accessor("recipeBookComponent")
        RecipeBookComponent<?> cursed$book();
    }

    /** The book's tabs, its menu, which recipes a collection shows from what is at hand, its settings sent. */
    @Mixin(RecipeBookComponent.class)
    public interface Book {
        @Accessor("tabInfos")
        List<RecipeBookComponent.TabInfo> cursed$tabs();

        @Accessor("menu")
        RecipeBookMenu cursed$menu();

        @Invoker("selectMatchingRecipes")
        void cursed$select(RecipeCollection collection, StackedItemContents contents);

        @Invoker("sendUpdateSettings")
        void cursed$send();

        /** The recipe the server could not fill, shown in the grid as what is missing. */
        @Accessor("ghostSlots")
        net.minecraft.client.gui.screens.recipebook.GhostSlots cursed$ghosts();
    }

    /** The missing ingredients by their slot. */
    @Mixin(net.minecraft.client.gui.screens.recipebook.GhostSlots.class)
    public interface Ghosts {
        @Accessor("ingredients")
        it.unimi.dsi.fastutil.objects.Reference2ObjectMap<net.minecraft.world.inventory.Slot, Object> cursed$ingredients();
    }

    /** One slot's: the items it may be (they take turns). */
    @Mixin(targets = "net.minecraft.client.gui.screens.recipebook.GhostSlots$GhostSlot")
    public interface Ghost {
        @Accessor("items")
        List<net.minecraft.world.item.ItemStack> cursed$items();
    }
}
