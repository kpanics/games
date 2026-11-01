package cursed.recode.mixin.client;

import com.mojang.blaze3d.vertex.PoseStack;
import cursed.recode.fx.ItemPhysic;
import net.minecraft.client.renderer.SubmitNodeCollector;
import net.minecraft.client.renderer.entity.ItemEntityRenderer;
import net.minecraft.client.renderer.entity.state.ItemEntityRenderState;
import net.minecraft.client.renderer.state.level.CameraRenderState;
import net.minecraft.world.entity.item.ItemEntity;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Item Physic: a dropped item's copies worked out with its state, drawn as the bodies lie (the game's spin and bob
 *  not). */
@Mixin(ItemEntityRenderer.class)
public abstract class ItemEntityRendererPhysicMixin {
    @Inject(method = "extractRenderState(Lnet/minecraft/world/entity/item/ItemEntity;Lnet/minecraft/client/renderer/entity/state/ItemEntityRenderState;F)V", at = @At("TAIL"))
    private void cursed$physic(ItemEntity entity, ItemEntityRenderState state, float partialTicks, CallbackInfo ci) {
        ItemPhysic.extract(entity, state, partialTicks);
    }

    @Inject(method = "submit(Lnet/minecraft/client/renderer/entity/state/ItemEntityRenderState;Lcom/mojang/blaze3d/vertex/PoseStack;Lnet/minecraft/client/renderer/SubmitNodeCollector;Lnet/minecraft/client/renderer/state/level/CameraRenderState;)V",
            at = @At("HEAD"), cancellable = true)
    private void cursed$bodies(ItemEntityRenderState state, PoseStack pose, SubmitNodeCollector out, CameraRenderState camera, CallbackInfo ci) {
        if (ItemPhysic.submit(state, pose, out)) {
            ci.cancel();
        }
    }
}
