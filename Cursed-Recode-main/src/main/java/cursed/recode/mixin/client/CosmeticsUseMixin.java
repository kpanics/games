package cursed.recode.mixin.client;

import cursed.recode.cosmetics.Cosmetics;
import net.minecraft.client.multiplayer.MultiPlayerGameMode;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.InteractionResult;
import net.minecraft.world.entity.player.Player;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/**
 * A bow or a crossbow shown as a CS rifle (native/src/cosmetics/rifle.rs): not drawn while its bolt is still coming
 * back, and its shot told — the bow let go with an arrow, the loaded crossbow fired.
 */
@Mixin(MultiPlayerGameMode.class)
public abstract class CosmeticsUseMixin {
    @Inject(method = "useItem", at = @At("HEAD"), cancellable = true)
    private void cursed$bolt(Player player, InteractionHand hand, CallbackInfoReturnable<InteractionResult> cir) {
        int what = Cosmetics.use(player, hand);
        if (what < 0) {
            cir.setReturnValue(InteractionResult.FAIL);
        }
    }

    @Inject(method = "releaseUsingItem", at = @At("HEAD"))
    private void cursed$shot(Player player, CallbackInfo ci) {
        Cosmetics.released(player);
    }
}
