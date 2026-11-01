package cursed.recode.mixin.client;

import net.minecraft.client.renderer.entity.player.AvatarRenderer;
import net.minecraft.client.renderer.entity.state.AvatarRenderState;
import net.minecraft.world.entity.Avatar;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Cosmetics from behind (cursed.recode.cosmetics.Cosmetics.thirdPose): a CS rifle held in both hands, as CS's agents
 *  hold theirs. */
@Mixin(AvatarRenderer.class)
public abstract class CosmeticsThirdPoseMixin {
    @Inject(method = "extractRenderState(Lnet/minecraft/world/entity/Avatar;Lnet/minecraft/client/renderer/entity/state/AvatarRenderState;F)V", at = @At("TAIL"))
    private void cursed$pose(Avatar entity, AvatarRenderState state, float partialTicks, CallbackInfo ci) {
        cursed.recode.cosmetics.Cosmetics.thirdPose(entity, state);
    }
}
