package cursed.recode.mixin.client;

import cursed.recode.fx.FreeLook;
import net.minecraft.client.player.LocalPlayer;
import net.minecraft.world.entity.Entity;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Free Look (cursed.recode.fx.FreeLook): while it looks, the mouse turns the camera, not the player. */
@Mixin(Entity.class)
public abstract class FreeLookTurnMixin {
    @Inject(method = "turn", at = @At("HEAD"), cancellable = true)
    private void cursed$look(double xo, double yo, CallbackInfo ci) {
        if ((Object) this instanceof LocalPlayer && FreeLook.turn(xo, yo)) {
            ci.cancel();
        }
    }
}
