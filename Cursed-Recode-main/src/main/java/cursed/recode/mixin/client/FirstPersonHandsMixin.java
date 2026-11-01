package cursed.recode.mixin.client;

import com.mojang.blaze3d.vertex.PoseStack;
import cursed.recode.fx.ViewModel;
import net.minecraft.client.renderer.FirstPersonHandsAndItemsRenderer;
import net.minecraft.client.renderer.SubmitNodeCollector;
import net.minecraft.client.renderer.state.level.FirstPersonHandsAndItemsRenderState;
import net.minecraft.client.renderer.state.level.PlayerRenderState;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.entity.HumanoidArm;
import net.minecraft.world.item.ItemStack;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.Redirect;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** View Model and Swing Animations (cursed.recode.fx.ViewModel): the hands in the first person held and swung ours. */
@Mixin(FirstPersonHandsAndItemsRenderer.class)
public abstract class FirstPersonHandsMixin {
    @Inject(method = "submitArmWithItem", at = @At("HEAD"))
    private void cursed$begin(PlayerRenderState playerState, FirstPersonHandsAndItemsRenderState state, float partialTicks, float xRot, InteractionHand hand,
                              float attack, ItemStack itemStack, float inverseArmHeight, PoseStack poseStack, SubmitNodeCollector collector, int light, CallbackInfo ci) {
        ViewModel.begin(hand, itemStack, attack, inverseArmHeight, partialTicks);
    }

    @Inject(method = "applyItemArmTransform", at = @At("HEAD"))
    private void cursed$beforePose(PoseStack poseStack, HumanoidArm arm, float inverseArmHeight, CallbackInfo ci) {
        ViewModel.beforeBase(poseStack);
    }

    @Inject(method = "applyItemArmTransform", at = @At("TAIL"))
    private void cursed$pose(PoseStack poseStack, HumanoidArm arm, float inverseArmHeight, CallbackInfo ci) {
        ViewModel.pose(poseStack);
    }

    @Redirect(method = "submitArmWithItem", at = @At(value = "INVOKE",
            target = "Lnet/minecraft/client/renderer/FirstPersonHandsAndItemsRenderer;swingArm(FLcom/mojang/blaze3d/vertex/PoseStack;ILnet/minecraft/world/entity/HumanoidArm;)V"))
    private void cursed$swing(FirstPersonHandsAndItemsRenderer self, float animation, PoseStack poseStack, int invert, HumanoidArm arm) {
        if (!ViewModel.swing(poseStack)) {
            ((FirstPersonHandsAccess) self).cursed$swingArm(animation, poseStack, invert, arm);
        }
    }
    @Redirect(method = "submitArmWithItem", at = @At(value = "INVOKE",
            target = "Lnet/minecraft/client/model/effects/SpearAnimations;firstPersonAttack(FLcom/mojang/blaze3d/vertex/PoseStack;ILnet/minecraft/world/entity/HumanoidArm;)V"))
    private void cursed$spear(float animation, PoseStack poseStack, int invert, HumanoidArm arm) {
        if (!ViewModel.classicApplied()) net.minecraft.client.model.effects.SpearAnimations.firstPersonAttack(animation, poseStack, invert, arm);
    }

}
