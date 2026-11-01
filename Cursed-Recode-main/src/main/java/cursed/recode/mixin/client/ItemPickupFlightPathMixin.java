package cursed.recode.mixin.client;

import cursed.recode.fx.ItemPhysic;
import net.minecraft.client.Camera;
import net.minecraft.client.particle.ItemPickupParticle;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Constant;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.ModifyConstant;
import org.spongepowered.asm.mixin.injection.ModifyVariable;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** Item Physic's pickup flight: over its longer time, up in an arc and into the one who took it; its copies close in
 *  and turn as they go (ItemPhysic.submit, from how far it has flown). */
@Mixin(targets = "net.minecraft.client.particle.ItemPickupParticleGroup$ParticleInstance")
public abstract class ItemPickupFlightPathMixin {
    @Inject(method = "fromParticle", at = @At("HEAD"))
    private static void cursed$flown(ItemPickupParticle particle, Camera camera, float partial, CallbackInfoReturnable<?> cir) {
        ItemPhysic.flying(particle, partial);
    }

    @ModifyConstant(method = "fromParticle", constant = @Constant(floatValue = 3.0F))
    private static float cursed$longer(float ticks) {
        return ItemPhysic.pickupTicks();
    }

    /** Its height on the way: an arc over it. */
    @ModifyVariable(method = "fromParticle", at = @At("STORE"), ordinal = 4)
    private static double cursed$arc(double y) {
        return y + ItemPhysic.arc();
    }
}
