package cursed.recode.mixin.client;

import cursed.recode.hud.HudInv;
import cursed.recode.hud.HudScreen;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.screens.inventory.AbstractContainerScreen;
import net.minecraft.world.inventory.Slot;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** Custom Gui: the hovered slot's light is the HUD's (it slides from slot to slot), not the game's. */
@Mixin(AbstractContainerScreen.class)
public abstract class ContainerScreenMixin {
    @Inject(method = "extractSlotHighlightBack", at = @At("HEAD"), cancellable = true)
    private void cursed$back(GuiGraphicsExtractor graphics, CallbackInfo ci) {
        if (HudScreen.drawing()) {
            ci.cancel();
        }
    }

    @Inject(method = "extractSlotHighlightFront", at = @At("HEAD"), cancellable = true)
    private void cursed$front(GuiGraphicsExtractor graphics, CallbackInfo ci) {
        if (HudScreen.drawing()) {
            ci.cancel();
        }
    }

    /** Custom Inventory: the slot under the pointer is the one under it in the client's layout (clicks, drags, keys). */
    @Inject(method = "getHoveredSlot", at = @At("HEAD"), cancellable = true)
    private void cursed$hovered(double x, double y, CallbackInfoReturnable<Slot> cir) {
        AbstractContainerScreen<?> self = (AbstractContainerScreen<?>) (Object) this;
        if (cursed.recode.hud.HudPack.active(self)) {
            cir.setReturnValue(cursed.recode.hud.HudPack.slotAt(self, x, y));
        } else if (HudInv.active(self)) {
            cir.setReturnValue(HudInv.slotAt(self, x, y));
        } else if (cursed.recode.hud.HudCreative.active(self)) {
            cir.setReturnValue(cursed.recode.hud.HudCreative.slotAt(self, x, y));
        }
    }

    /** Custom Inventory's editor: its pencil and, while it is on, every click, drag and release are its. */
    @Inject(method = "mouseClicked", at = @At("HEAD"), cancellable = true)
    private void cursed$editClick(net.minecraft.client.input.MouseButtonEvent event, boolean doubleClick, CallbackInfoReturnable<Boolean> cir) {
        if (cursed.recode.hud.HudPack.takeClick((AbstractContainerScreen<?>) (Object) this, event)) {
            cir.setReturnValue(true);
        }
    }

    @Inject(method = "mouseDragged", at = @At("HEAD"), cancellable = true)
    private void cursed$editDrag(net.minecraft.client.input.MouseButtonEvent event, double dx, double dy, CallbackInfoReturnable<Boolean> cir) {
        if (cursed.recode.hud.HudPack.takeDrag((AbstractContainerScreen<?>) (Object) this)) {
            cir.setReturnValue(true);
        }
    }

    @Inject(method = "mouseReleased", at = @At("HEAD"), cancellable = true)
    private void cursed$editRelease(net.minecraft.client.input.MouseButtonEvent event, CallbackInfoReturnable<Boolean> cir) {
        if (cursed.recode.hud.HudPack.takeRelease((AbstractContainerScreen<?>) (Object) this, event)) {
            cir.setReturnValue(true);
        }
    }

    /** The wheel over the recipe book turns its recipes. */
    @Inject(method = "mouseScrolled", at = @At("HEAD"), cancellable = true)
    private void cursed$recipesWheel(double x, double y, double scrollX, double scrollY, CallbackInfoReturnable<Boolean> cir) {
        if (cursed.recode.hud.HudPack.recipesScroll((AbstractContainerScreen<?>) (Object) this, scrollY)) {
            cir.setReturnValue(true);
        }
    }

    /** Esc while the editor is on: the editor goes off, the inventory stays. */
    @Inject(method = "keyPressed", at = @At("HEAD"), cancellable = true)
    private void cursed$editKey(net.minecraft.client.input.KeyEvent event, CallbackInfoReturnable<Boolean> cir) {
        if (event.isEscape() && cursed.recode.hud.HudPack.escape((AbstractContainerScreen<?>) (Object) this)) {
            cir.setReturnValue(true);
        }
    }

    /** Custom Inventory: a stack let go outside every block of the layout is thrown. */
    @Inject(method = "hasClickedOutside", at = @At("HEAD"), cancellable = true)
    private void cursed$outside(double mx, double my, int xo, int yo, CallbackInfoReturnable<Boolean> cir) {
        if (cursed.recode.hud.HudPack.active((AbstractContainerScreen<?>) (Object) this)) {
            cir.setReturnValue(cursed.recode.hud.HudPack.outside(mx, my));
        }
    }
}
