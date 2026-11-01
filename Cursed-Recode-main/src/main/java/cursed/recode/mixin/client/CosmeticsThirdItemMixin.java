package cursed.recode.mixin.client;

import com.llamalad7.mixinextras.injector.wrapoperation.Operation;
import com.llamalad7.mixinextras.injector.wrapoperation.WrapOperation;
import com.llamalad7.mixinextras.sugar.Local;
import com.mojang.blaze3d.vertex.PoseStack;
import net.minecraft.client.renderer.SubmitNodeCollector;
import net.minecraft.client.renderer.entity.layers.ItemInHandLayer;
import net.minecraft.client.renderer.entity.state.ArmedEntityRenderState;
import net.minecraft.client.renderer.item.ItemStackRenderState;
import net.minecraft.world.entity.HumanoidArm;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;

/** Cosmetics from behind (cursed.recode.cosmetics.Cosmetics.third): the player's own CS item drawn by the engine at the
 *  hold the game would draw its item at; the game's own left out. */
@Mixin(ItemInHandLayer.class)
public abstract class CosmeticsThirdItemMixin {
    @WrapOperation(method = "submitArmWithItem", at = @At(value = "INVOKE",
            target = "Lnet/minecraft/client/renderer/item/ItemStackRenderState;submit(Lcom/mojang/blaze3d/vertex/PoseStack;Lnet/minecraft/client/renderer/SubmitNodeCollector;III)V"))
    private void cursed$third(ItemStackRenderState item, PoseStack pose, SubmitNodeCollector out, int light, int overlay, int outline, Operation<Void> original,
                              @Local(argsOnly = true) ArmedEntityRenderState state, @Local(argsOnly = true) HumanoidArm arm) {
        if (!cursed.recode.cosmetics.Cosmetics.third(state, arm, pose)) {
            original.call(item, pose, out, light, overlay, outline);
        }
    }
}
