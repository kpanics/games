package cursed.recode.mixin.client;

import net.minecraft.client.player.LocalPlayer;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.item.component.SwingAnimation;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** The player's main hand swung (an attack, a hit on a block, into the air): the cosmetics in the hands play theirs
 *  (native/src/cosmetics/). */
@Mixin(LivingEntity.class)
public abstract class LocalPlayerSwingMixin {
    @Inject(method = "swing(Lnet/minecraft/world/InteractionHand;Lnet/minecraft/world/item/component/SwingAnimation;Z)Z", at = @At("HEAD"))
    private void cursed$swing(InteractionHand hand, SwingAnimation animation, boolean force, CallbackInfoReturnable<Boolean> cir) {
        if (hand == InteractionHand.MAIN_HAND && (Object) this instanceof LocalPlayer) {
            cursed.recode.cosmetics.Cosmetics.event(cursed.recode.cosmetics.Cosmetics.struck(net.minecraft.client.Minecraft.getInstance()) ? "swing hit" : "swing");
        }
    }
}
