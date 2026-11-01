package cursed.recode.mixin.client;

import net.minecraft.client.gui.components.DebugScreenOverlay;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Custom F3 on: the game's own words of F3 are not drawn (the client's cards show instead, cursed.recode.hud.HudF3). */
@Mixin(DebugScreenOverlay.class)
public abstract class DebugScreenOverlayMixin {
    @Inject(method = "extractRenderState", at = @At("HEAD"), cancellable = true)
    private void cursed$ours(net.minecraft.client.gui.GuiGraphicsExtractor graphics, CallbackInfo ci) {
        if (cursed.recode.hud.HudF3.active) {
            ci.cancel();
        }
    }
}
