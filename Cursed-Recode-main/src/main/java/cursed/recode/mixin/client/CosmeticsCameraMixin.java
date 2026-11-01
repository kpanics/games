package cursed.recode.mixin.client;

import cursed.recode.cosmetics.Cosmetics;
import net.minecraft.client.Camera;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/**
 * A CS rifle in the hands (native/src/cosmetics/rifle.rs): its scope narrows the view, its shot kicks the camera up
 * and back down. Only what is drawn turns: the player keeps looking where they looked.
 */
@Mixin(Camera.class)
public abstract class CosmeticsCameraMixin {
    @Shadow
    private float xRot;
    @Shadow
    private float yRot;
    @Shadow
    private boolean detached;

    @Shadow
    protected abstract void setRotation(float yRot, float xRot);

    @Inject(method = "calculateFov", at = @At("RETURN"), cancellable = true)
    private void cursed$scope(float partialTicks, CallbackInfoReturnable<Float> cir) {
        if (!this.detached) {
            cir.setReturnValue(Cosmetics.fov(cir.getReturnValueF()));
        }
    }

    @Inject(method = "alignWithEntity", at = @At("TAIL"))
    private void cursed$kick(float partialTicks, CallbackInfo ci) {
        float[] k = Cosmetics.kick();
        if (!this.detached && (k[0] != 0.0F || k[1] != 0.0F)) {
            this.setRotation(this.yRot + k[1], this.xRot + k[0]);
        }
    }
}
