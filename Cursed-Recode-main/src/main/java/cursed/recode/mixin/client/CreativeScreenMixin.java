package cursed.recode.mixin.client;

import cursed.recode.hud.HudCreative;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.screens.inventory.CreativeModeInventoryScreen;
import net.minecraft.client.input.MouseButtonEvent;
import net.minecraft.world.inventory.ContainerInput;
import net.minecraft.world.inventory.Slot;
import net.minecraft.world.item.CreativeModeTab;
import org.jspecify.annotations.Nullable;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/**
 * Custom Inventory in creative (cursed.recode.hud.HudCreative): the creative inventory drawn by the client. Its
 * background and contents are the client's; its tabs and scroll bar are not where the game would put them (the
 * client's rail and wheel choose and scroll); a click on the rail, the search, the bin or for a favourite is the
 * client's; a grid item's click takes a stack (the right button one, Shift into the hotbar).
 */
@Mixin(CreativeModeInventoryScreen.class)
public abstract class CreativeScreenMixin {
    private CreativeModeInventoryScreen cursed$self() {
        return (CreativeModeInventoryScreen) (Object) this;
    }

    @Inject(method = "extractBackground", at = @At("HEAD"), cancellable = true)
    private void cursed$background(GuiGraphicsExtractor graphics, int mouseX, int mouseY, float a, CallbackInfo ci) {
        if (HudCreative.active(this.cursed$self())) {
            HudCreative.background(this.cursed$self(), graphics);
            ci.cancel();
        }
    }

    @Inject(method = "extractRenderState", at = @At("HEAD"), cancellable = true)
    private void cursed$contents(GuiGraphicsExtractor graphics, int mouseX, int mouseY, float a, CallbackInfo ci) {
        if (HudCreative.active(this.cursed$self())) {
            HudCreative.contents(this.cursed$self(), graphics, mouseX, mouseY);
            ci.cancel();
        }
    }

    @Inject(method = "mouseClicked", at = @At("HEAD"), cancellable = true)
    private void cursed$click(MouseButtonEvent event, boolean doubleClick, CallbackInfoReturnable<Boolean> cir) {
        if (HudCreative.active(this.cursed$self()) && HudCreative.click(this.cursed$self(), event)) {
            cir.setReturnValue(true);
        }
    }

    /** A set's name being typed takes the keys and the letters. */
    @Inject(method = "keyPressed", at = @At("HEAD"), cancellable = true)
    private void cursed$key(net.minecraft.client.input.KeyEvent event, CallbackInfoReturnable<Boolean> cir) {
        if (HudCreative.active(this.cursed$self()) && HudCreative.key(event)) {
            cir.setReturnValue(true);
        }
    }

    @Inject(method = "charTyped", at = @At("HEAD"), cancellable = true)
    private void cursed$typed(net.minecraft.client.input.CharacterEvent event, CallbackInfoReturnable<Boolean> cir) {
        if (HudCreative.active(this.cursed$self()) && HudCreative.typed(event.codepoint())) {
            cir.setReturnValue(true);
        }
    }

    @Inject(method = "mouseScrolled", at = @At("HEAD"), cancellable = true)
    private void cursed$scroll(double x, double y, double scrollX, double scrollY, CallbackInfoReturnable<Boolean> cir) {
        if (HudCreative.active(this.cursed$self()) && HudCreative.scroll(this.cursed$self(), x, y, scrollY)) {
            cir.setReturnValue(true);
        }
    }

    @Inject(method = "slotClicked", at = @At("HEAD"), cancellable = true)
    private void cursed$slot(@Nullable Slot slot, int slotId, int buttonNum, ContainerInput input, CallbackInfo ci) {
        if (HudCreative.slotClicked(this.cursed$self(), slot, buttonNum, input)) {
            ci.cancel();
        }
    }

    @Inject(method = "checkTabClicked", at = @At("HEAD"), cancellable = true)
    private void cursed$tab(CreativeModeTab tab, double xm, double ym, CallbackInfoReturnable<Boolean> cir) {
        if (HudCreative.active(this.cursed$self())) {
            cir.setReturnValue(false);
        }
    }

    @Inject(method = "insideScrollbar", at = @At("HEAD"), cancellable = true)
    private void cursed$bar(double xm, double ym, CallbackInfoReturnable<Boolean> cir) {
        if (HudCreative.active(this.cursed$self())) {
            cir.setReturnValue(false);
        }
    }

    @Inject(method = "hasClickedOutside", at = @At("HEAD"), cancellable = true)
    private void cursed$outside(double mx, double my, int xo, int yo, CallbackInfoReturnable<Boolean> cir) {
        if (HudCreative.active(this.cursed$self())) {
            boolean outside = HudCreative.outside(mx, my);
            ((CreativeScreenAccess) this).cursed$setOutside(outside);
            cir.setReturnValue(outside);
        }
    }
}
