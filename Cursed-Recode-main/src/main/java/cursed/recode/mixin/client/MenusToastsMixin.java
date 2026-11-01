package cursed.recode.mixin.client;

import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.components.toasts.ToastManager;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** The game's toasts are the menus' (cursed.recode.menus.Toasts); a kind they do not know the game still draws. */
@Mixin(ToastManager.class)
public abstract class MenusToastsMixin {
    @Inject(method = "extractRenderState", at = @At("HEAD"), cancellable = true)
    private void cursed$toasts(GuiGraphicsExtractor graphics, CallbackInfo ci) {
        if (!cursed.recode.dev.Dev.on("CURSED_DEV_VANILLASCREENS")) {
            cursed.recode.menus.Toasts.extract((ToastManager) (Object) this, graphics);
            ci.cancel();
        }
    }
}
