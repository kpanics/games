package cursed.recode.mixin.client;

import net.minecraft.client.gui.screens.inventory.AbstractContainerScreen;
import net.minecraft.world.inventory.Slot;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Accessor;

/** Where a container screen's picture is and the slot under the pointer: Custom Gui (cursed.recode.hud.HudScreen). */
@Mixin(AbstractContainerScreen.class)
public interface ContainerScreenAccess {
    @Accessor("leftPos")
    int cursed$left();

    @Accessor("topPos")
    int cursed$top();

    @Accessor("imageWidth")
    int cursed$width();

    @Accessor("imageHeight")
    int cursed$height();

    @Accessor("hoveredSlot")
    Slot cursed$hovered();

    /** Custom Inventory: the slot under the pointer is the one under it in the client's layout. */
    @Accessor("hoveredSlot")
    void cursed$setHovered(Slot slot);

    /** Custom Inventory: a stack being spread over slots by dragging, and over which. */
    @Accessor("isQuickCrafting")
    boolean cursed$quick();

    @Accessor("quickCraftSlots")
    java.util.Set<Slot> cursed$quickSlots();

    /** Custom Inventory: how a dragged stack is spread (evenly, one each, a stack each). */
    @Accessor("quickCraftingType")
    int cursed$quickType();
}
