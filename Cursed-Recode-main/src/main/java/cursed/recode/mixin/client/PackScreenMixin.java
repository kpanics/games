package cursed.recode.mixin.client;

import cursed.recode.hud.HudPack;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.gui.screens.inventory.AbstractContainerScreen;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/**
 * Custom Inventory (cursed.recode.hud.HudPack): a container it lays out is drawn by it whole, instead of the screen's
 * background and contents (any container screen's: its own drawing and its subclass's alike); what the game defers
 * (a tooltip with a picture of its own) still comes after.
 */
@Mixin(Screen.class)
public abstract class PackScreenMixin {
    @Inject(method = "init(II)V", at = @At("TAIL"))
    private void cursed$laidOut(int width, int height, CallbackInfo ci) {
        Screen self = (Screen) (Object) this;
        if (self instanceof AbstractContainerScreen<?>) {
            HudPack.init(self);
        }
    }

    @Inject(method = "extractRenderStateWithTooltipAndSubtitles", at = @At("HEAD"), cancellable = true)
    private void cursed$pack(GuiGraphicsExtractor graphics, int mouseX, int mouseY, float a, CallbackInfo ci) {
        Screen self = (Screen) (Object) this;
        if (self instanceof AbstractContainerScreen<?> s && HudPack.active(self)) {
            graphics.nextStratum();
            HudPack.render(s, graphics, mouseX, mouseY, a);
            graphics.extractDeferredElements(mouseX, mouseY, a);
            ci.cancel();
        }
    }
}
