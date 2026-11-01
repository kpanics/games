package cursed.recode.mixin.client;

import com.mojang.blaze3d.vertex.PoseStack;
import cursed.recode.motion.LivingMotion;
import cursed.recode.motion.MotionPose;
import net.minecraft.client.renderer.entity.player.AvatarRenderer;
import net.minecraft.client.renderer.entity.state.AvatarRenderState;
import net.minecraft.world.entity.Avatar;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Living Motion: a player's pose taken with its state, the whole model moved under it at its feet. */
@Mixin(AvatarRenderer.class)
public abstract class AvatarRendererMotionMixin {
    @Inject(method = "extractRenderState(Lnet/minecraft/world/entity/Avatar;Lnet/minecraft/client/renderer/entity/state/AvatarRenderState;F)V", at = @At("TAIL"))
    private void cursed$motion(Avatar entity, AvatarRenderState state, float partialTicks, CallbackInfo ci) {
        LivingMotion.extract(entity, state, partialTicks);
        ((cursed.recode.motion.StudioPose) state).cursed$studio(cursed.recode.fx.FakePlayers.studio(entity));
    }

    @Inject(method = "setupRotations(Lnet/minecraft/client/renderer/entity/state/AvatarRenderState;Lcom/mojang/blaze3d/vertex/PoseStack;FF)V", at = @At("TAIL"))
    private void cursed$root(AvatarRenderState state, PoseStack pose, float bodyRot, float entityScale, CallbackInfo ci) {
        float[] p = ((MotionPose) state).cursed$motion();
        if (p != null) {
            LivingMotion.root(p, pose);
        }
    }
}
