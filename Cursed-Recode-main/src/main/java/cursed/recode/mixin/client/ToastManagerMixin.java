package cursed.recode.mixin.client;

import cursed.recode.scene.HudCache;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.components.toasts.ToastManager;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** The toasts are the HUD texture's too (cursed.recode.scene.HudCache): a frame that keeps it extracts none. */
@Mixin(ToastManager.class)
public abstract class ToastManagerMixin {
    @Inject(method = "extractRenderState", at = @At("HEAD"), cancellable = true)
    private void cursed$kept(GuiGraphicsExtractor graphics, CallbackInfo ci) {
        if (HudCache.kept()) {
            ci.cancel();
        }
    }
}
