package cursed.recode.mixin.client;

import com.mojang.blaze3d.vertex.PoseStack;
import cursed.recode.motion.Joints;
import cursed.recode.motion.LivingMotion;
import cursed.recode.motion.MotionPose;
import net.minecraft.client.model.HumanoidModel;
import net.minecraft.client.renderer.entity.state.HumanoidRenderState;
import net.minecraft.world.entity.HumanoidArm;
import org.objectweb.asm.Opcodes;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/**
 * Living Motion on a player's model (and its armour's, posed from the same state): its walk where the game sets its
 * own (right after it, before riding, the held items, the blows and the crouch change it), the body's after the game.
 */
@Mixin(HumanoidModel.class)
public abstract class HumanoidModelMotionMixin {
    @Inject(method = "setupAnim(Lnet/minecraft/client/renderer/entity/state/HumanoidRenderState;)V",
            at = @At(value = "FIELD", target = "Lnet/minecraft/client/renderer/entity/state/HumanoidRenderState;isPassenger:Z", opcode = Opcodes.GETFIELD, ordinal = 0))
    private void cursed$walk(HumanoidRenderState state, CallbackInfo ci) {
        if (state instanceof MotionPose m && m.cursed$motion() != null) {
            LivingMotion.limbs((HumanoidModel<?>) (Object) this, m.cursed$motion());
        }
    }

    @Inject(method = "setupAnim(Lnet/minecraft/client/renderer/entity/state/HumanoidRenderState;)V", at = @At("TAIL"))
    private void cursed$body(HumanoidRenderState state, CallbackInfo ci) {
        HumanoidModel<?> model = (HumanoidModel<?>) (Object) this;
        if (state instanceof MotionPose m && m.cursed$motion() != null) {
            LivingMotion.body(model, state, m.cursed$motion());
        }
        cursed.recode.fx.FakePlayers.studioModel(model, state);
        Joints j = Joints.ifCut(model);
        if (j != null) {
            j.showAsOver();
        }
    }

    /** A held item in a bent arm's hand goes with the forearm. */
    @Inject(method = "translateToHand(Lnet/minecraft/client/renderer/entity/state/HumanoidRenderState;Lnet/minecraft/world/entity/HumanoidArm;Lcom/mojang/blaze3d/vertex/PoseStack;)V",
            at = @At("TAIL"))
    private void cursed$hand(HumanoidRenderState state, HumanoidArm arm, PoseStack pose, CallbackInfo ci) {
        Joints j = Joints.ifCut((HumanoidModel<?>) (Object) this);
        if (j != null) {
            j.hand(arm, pose);
        }
    }
}
