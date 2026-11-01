package cursed.recode.mixin.client;

import cursed.recode.fx.ItemPhysic;
import net.minecraft.client.particle.ItemPickupParticle;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Constant;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.ModifyConstant;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Item Physic's pickup flight: the picked up item flies a while longer (ItemPickupFlightPathMixin draws its way). */
@Mixin(ItemPickupParticle.class)
public abstract class ItemPickupFlightMixin {
    @ModifyConstant(method = "tick", constant = @Constant(intValue = 3))
    private int cursed$longer(int ticks) {
        return ItemPhysic.pickupTicks();
    }

    /** Gone once it has flown its time (the flight turned off mid-way: the game's own check would miss it). */
    @Inject(method = "tick", at = @At("TAIL"))
    private void cursed$done(CallbackInfo ci) {
        if (((ItemPickupParticleAccess) this).cursed$life() >= ItemPhysic.pickupTicks()) {
            ((net.minecraft.client.particle.Particle) (Object) this).remove();
        }
    }
}
