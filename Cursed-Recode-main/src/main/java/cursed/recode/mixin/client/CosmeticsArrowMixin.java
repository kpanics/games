package cursed.recode.mixin.client;

import cursed.recode.cosmetics.Cosmetics;
import net.minecraft.client.renderer.culling.Frustum;
import net.minecraft.client.renderer.entity.EntityRenderDispatcher;
import net.minecraft.world.entity.Entity;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** The arrow a CS rifle's shot let fly is not drawn: its tracer is the shot (her word, 24.10). */
@Mixin(EntityRenderDispatcher.class)
public abstract class CosmeticsArrowMixin {
    @Inject(method = "shouldRender", at = @At("HEAD"), cancellable = true)
    private <E extends Entity> void cursed$noArrow(E entity, Frustum frustum, double x, double y, double z, float partialTicks, CallbackInfoReturnable<Boolean> cir) {
        if (Cosmetics.hidden(entity)) {
            cir.setReturnValue(false);
        }
    }
}
