package cursed.recode.mixin.client;

import cursed.recode.menus.Menus;
import net.minecraft.client.Camera;
import net.minecraft.util.Mth;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/**
 * The advancements are a look up (menus/adv.rs): while they are up the camera rises to the sky, and comes down again
 * as they go. Only what is drawn turns: the player keeps looking where they looked.
 */
@Mixin(Camera.class)
public abstract class MenusCameraMixin {
    @Shadow
    private float xRot;
    @Shadow
    private float yRot;
    @Shadow
    private boolean detached;

    @Shadow
    protected abstract void setRotation(float yRot, float xRot);

    @Inject(method = "alignWithEntity", at = @At("TAIL"))
    private void cursed$lookUp(float partialTicks, CallbackInfo ci) {
        float k = Menus.lookUp();
        if (k > 0.0F && !this.detached) {
            this.setRotation(this.yRot, Mth.lerp(k, this.xRot, -80.0F));
        }
    }
}
