package cursed.recode.mixin.client;

import cursed.recode.fx.FreeLook;
import net.minecraft.client.Camera;
import net.minecraft.client.DeltaTracker;
import net.minecraft.client.Minecraft;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.level.Level;
import net.minecraft.world.phys.Vec3;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.Unique;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.ModifyArg;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** Free Look (cursed.recode.fx.FreeLook): the camera turned where it looks and as far behind as it says. */
@Mixin(Camera.class)
public abstract class FreeLookCameraMixin {
    @Shadow private Vec3 position;
    @Shadow private Level level;
    @Shadow private Entity entity;
    @Shadow protected abstract void setPosition(Vec3 position);
    @Unique private Vec3 cursed$eye;

    @Inject(method = "alignWithEntity", at = @At("HEAD"))
    private void cursed$start(float partialTicks, CallbackInfo ci) {
        cursed$eye = null;
    }

    @Inject(method = "getMaxZoom", at = @At("HEAD"))
    private void cursed$captureEye(float distance, CallbackInfoReturnable<Float> cir) {
        cursed$eye = position;
    }

    @Inject(method = "alignWithEntity", at = @At("TAIL"))
    private void cursed$height(float partialTicks, CallbackInfo ci) {
        if (FreeLook.on() && cursed$eye != null) {
            setPosition(FreeLook.raised(level, entity, cursed$eye, position));
        }
    }

    @Inject(method = "update", at = @At("HEAD"))
    private void cursed$frame(DeltaTracker deltaTracker, CallbackInfo ci) {
        FreeLook.frame(Minecraft.getInstance());
    }

    // the camera's turn as the entity's look (a minecart's rider, anyone else)
    @ModifyArg(method = "alignWithEntity", at = @At(value = "INVOKE", target = "Lnet/minecraft/client/Camera;setRotation(FF)V", ordinal = 0), index = 0)
    private float cursed$yawCart(float yaw) {
        return FreeLook.yaw(yaw);
    }

    @ModifyArg(method = "alignWithEntity", at = @At(value = "INVOKE", target = "Lnet/minecraft/client/Camera;setRotation(FF)V", ordinal = 0), index = 1)
    private float cursed$pitchCart(float pitch) {
        return FreeLook.pitch(pitch);
    }

    @ModifyArg(method = "alignWithEntity", at = @At(value = "INVOKE", target = "Lnet/minecraft/client/Camera;setRotation(FF)V", ordinal = 1), index = 0)
    private float cursed$yaw(float yaw) {
        return FreeLook.yaw(yaw);
    }

    @ModifyArg(method = "alignWithEntity", at = @At(value = "INVOKE", target = "Lnet/minecraft/client/Camera;setRotation(FF)V", ordinal = 1), index = 1)
    private float cursed$pitch(float pitch) {
        return FreeLook.pitch(pitch);
    }

    @ModifyArg(method = "alignWithEntity", at = @At(value = "INVOKE", target = "Lnet/minecraft/client/Camera;getMaxZoom(F)F"), index = 0)
    private float cursed$distance(float dist) {
        return FreeLook.distance(dist);
    }
}
