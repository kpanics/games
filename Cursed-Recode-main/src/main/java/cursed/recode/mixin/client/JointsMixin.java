package cursed.recode.mixin.client;

import com.mojang.blaze3d.vertex.PoseStack;
import cursed.recode.motion.Joints;
import net.minecraft.client.model.HumanoidModel;
import net.minecraft.client.model.Model;
import net.minecraft.client.model.player.PlayerModel;
import net.minecraft.client.renderer.entity.state.AvatarRenderState;
import net.minecraft.world.entity.HumanoidArm;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Living Motion's knees and elbows (Joints): straight again when the game resets a model's pose (each frame's own,
 *  the first person's arm), a held item in a bent arm's hand going with the forearm (the player's model's own hand). */
public final class JointsMixin {
    private JointsMixin() {
    }

    @Mixin(Model.class)
    public abstract static class Reset {
        @Inject(method = "resetPose", at = @At("TAIL"))
        private void cursed$straight(CallbackInfo ci) {
            if ((Object) this instanceof HumanoidModel<?> m) {
                Joints j = Joints.ifCut(m);
                if (j != null) {
                    j.reset();
                }
            }
        }
    }

    /** The first person's arm: the model is everyone's of its kind, so its elbow is straightened here (the game resets
     *  only the arm itself), its sleeve's lower half shown as the sleeve is. */
    @Mixin(net.minecraft.client.renderer.entity.player.AvatarRenderer.class)
    public abstract static class FirstPerson {
        @Inject(method = "renderHand", at = @At("HEAD"))
        private void cursed$straight(PoseStack pose, net.minecraft.client.renderer.SubmitNodeCollector out, int light, net.minecraft.resources.Identifier skin,
                                     net.minecraft.client.model.geom.ModelPart arm, boolean sleeve, CallbackInfo ci) {
            Object m = ((net.minecraft.client.renderer.entity.LivingEntityRenderer<?, ?, ?>) (Object) this).getModel();
            if (m instanceof HumanoidModel<?> h) {
                Joints j = Joints.ifCut(h);
                if (j != null) {
                    j.firstPerson(sleeve);
                }
            }
        }
    }

    @Mixin(PlayerModel.class)
    public abstract static class Hand {
        @Inject(method = "translateToHand(Lnet/minecraft/client/renderer/entity/state/AvatarRenderState;Lnet/minecraft/world/entity/HumanoidArm;Lcom/mojang/blaze3d/vertex/PoseStack;)V",
                at = @At("TAIL"))
        private void cursed$hand(AvatarRenderState state, HumanoidArm arm, PoseStack pose, CallbackInfo ci) {
            Joints j = Joints.ifCut((HumanoidModel<?>) (Object) this);
            if (j != null) {
                j.hand(arm, pose);
            }
        }
    }
}
