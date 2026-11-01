package cursed.recode.mixin.client;

import cursed.recode.hud.HudLayer;
import net.minecraft.world.entity.LivingEntity;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** A totem popped (entity event 35): the HUD counts it on the target and says so. */
@Mixin(LivingEntity.class)
public abstract class LivingEntityHudMixin {
    @Inject(method = "handleEntityEvent", at = @At("HEAD"))
    private void cursed$totem(byte id, CallbackInfo ci) {
        LivingEntity self = (LivingEntity) (Object) this;
        if (id == 35 && self.level().isClientSide()) {
            HudLayer.totem(self);
        }
    }
}
