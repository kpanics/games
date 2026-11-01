package cursed.recode.mixin.client;

import cursed.recode.cosmetics.Cosmetics;
import net.minecraft.client.Minecraft;
import net.minecraft.client.MouseHandler;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.ModifyArg;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** A CS rifle's scope (native/src/cosmetics/rifle.rs): the mouse slower as the view narrows (CS's
 *  zoom_sensitivity_ratio 1), the wheel its other zoom level instead of the next slot. */
@Mixin(MouseHandler.class)
public abstract class CosmeticsMouseMixin {
    @ModifyArg(method = "turnPlayer", at = @At(value = "INVOKE", target = "Lnet/minecraft/client/player/LocalPlayer;turn(DD)V"), index = 0)
    private double cursed$scopeX(double dx) {
        return dx * Cosmetics.sensitivity();
    }

    @ModifyArg(method = "turnPlayer", at = @At(value = "INVOKE", target = "Lnet/minecraft/client/player/LocalPlayer;turn(DD)V"), index = 1)
    private double cursed$scopeY(double dy) {
        return dy * Cosmetics.sensitivity();
    }

    @Inject(method = "onScroll", at = @At("HEAD"), cancellable = true)
    private void cursed$zoom(long handle, double xoffset, double yoffset, CallbackInfo ci) {
        Minecraft mc = Minecraft.getInstance();
        if (handle == mc.getWindow().handle() && mc.gui.screen() == null && yoffset != 0.0 && Cosmetics.scoped()) {
            Cosmetics.event("zoom");
            ci.cancel();
        }
    }
}
