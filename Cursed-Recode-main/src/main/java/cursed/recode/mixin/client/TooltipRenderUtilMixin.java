package cursed.recode.mixin.client;

import cursed.recode.hud.HudScreen;
import net.minecraft.client.gui.screens.inventory.tooltip.TooltipRenderUtil;
import net.minecraft.resources.Identifier;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** Custom Gui: the game's tooltips on the client's plate (one a server's style asks for is left as it is). */
@Mixin(TooltipRenderUtil.class)
public abstract class TooltipRenderUtilMixin {
    @Inject(method = "getBackgroundSprite", at = @At("HEAD"), cancellable = true)
    private static void cursed$background(Identifier style, CallbackInfoReturnable<Identifier> cir) {
        Identifier own = HudScreen.tooltip(style, false);
        if (own != null) {
            cir.setReturnValue(own);
        }
    }

    @Inject(method = "getFrameSprite", at = @At("HEAD"), cancellable = true)
    private static void cursed$frame(Identifier style, CallbackInfoReturnable<Identifier> cir) {
        Identifier own = HudScreen.tooltip(style, true);
        if (own != null) {
            cir.setReturnValue(own);
        }
    }
}
