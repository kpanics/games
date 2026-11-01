package cursed.recode.mixin.client;

import net.minecraft.client.player.LocalPlayer;
import net.minecraft.world.entity.LivingEntity;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** Swing Animations' speed: how long the player's own swing lasts on their screen (the server's swing is its own). */
@Mixin(LivingEntity.class)
public abstract class SwingDurationMixin {
    @Inject(method = "getModifiedSwingDuration", at = @At("RETURN"), cancellable = true)
    private void cursed$speed(net.minecraft.world.item.component.SwingAnimation animation, CallbackInfoReturnable<Integer> cir) {
        if ((Object) this instanceof LocalPlayer) {
            cir.setReturnValue(cursed.recode.fx.ViewModel.duration(cir.getReturnValueI()));
        }
    }
}
