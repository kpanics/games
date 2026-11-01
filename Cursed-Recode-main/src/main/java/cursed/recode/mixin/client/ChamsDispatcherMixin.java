package cursed.recode.mixin.client;

import com.mojang.blaze3d.vertex.PoseStack;
import cursed.recode.chams.ChamsHooks;
import net.minecraft.client.renderer.SubmitNodeCollector;
import net.minecraft.client.renderer.entity.EntityRenderDispatcher;
import net.minecraft.client.renderer.entity.state.EntityRenderState;
import net.minecraft.client.renderer.state.level.CameraRenderState;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Chams (cursed.recode.chams.Chams): a body's submit is a chams' body's when the player wants it in the look; Target
 *  ESP (cursed.recode.render.TargetEsp) takes a target's boxes from it. */
@Mixin(EntityRenderDispatcher.class)
public abstract class ChamsDispatcherMixin {
    @Inject(method = "submit", at = @At("HEAD"))
    private void cursed$chamsBody(EntityRenderState state, CameraRenderState camera, double x, double y, double z, PoseStack poseStack,
                                  SubmitNodeCollector out, CallbackInfo ci) {
        if (ChamsHooks.inGui()) {
            return;
        }
        ChamsHooks.bodyStart(state);
        cursed.recode.render.TargetEsp.bodyStart(state);
    }

    @Inject(method = "submit", at = @At("RETURN"))
    private void cursed$chamsBodyDone(EntityRenderState state, CameraRenderState camera, double x, double y, double z, PoseStack poseStack,
                                      SubmitNodeCollector out, CallbackInfo ci) {
        if (ChamsHooks.inGui()) {
            return;
        }
        ChamsHooks.bodyDone((EntityRenderDispatcher) (Object) this, state, camera, poseStack, out);
        cursed.recode.render.TargetEsp.bodyDone();
    }
}
