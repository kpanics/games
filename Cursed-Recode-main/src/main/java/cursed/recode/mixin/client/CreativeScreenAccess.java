package cursed.recode.mixin.client;

import net.minecraft.client.gui.components.EditBox;
import net.minecraft.client.gui.screens.inventory.CreativeModeInventoryScreen;
import net.minecraft.world.inventory.Slot;
import net.minecraft.world.item.CreativeModeTab;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Accessor;
import org.spongepowered.asm.mixin.gen.Invoker;

/** Custom Inventory in creative (cursed.recode.hud.HudCreative): the creative screen's tab, search, bin, its choosing. */
@Mixin(CreativeModeInventoryScreen.class)
public interface CreativeScreenAccess {
    @Accessor("selectedTab")
    static CreativeModeTab cursed$tab() {
        throw new AssertionError();
    }

    @Accessor("searchBox")
    EditBox cursed$search();

    @Accessor("destroyItemSlot")
    Slot cursed$bin();

    /** Whether the last click was outside the screen (what the game throws a carried stack by). */
    @Accessor("hasClickedOutside")
    void cursed$setOutside(boolean outside);

    @Invoker("selectTab")
    void cursed$select(CreativeModeTab tab);

    @Invoker("refreshSearchResults")
    void cursed$refresh();
}
