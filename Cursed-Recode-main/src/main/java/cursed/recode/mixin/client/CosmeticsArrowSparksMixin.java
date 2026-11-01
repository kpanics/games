package cursed.recode.mixin.client;

import com.llamalad7.mixinextras.injector.wrapoperation.Operation;
import com.llamalad7.mixinextras.injector.wrapoperation.WrapOperation;
import cursed.recode.cosmetics.Cosmetics;
import net.minecraft.core.particles.ParticleOptions;
import net.minecraft.world.entity.projectile.arrow.AbstractArrow;
import net.minecraft.world.level.Level;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;

/** The sparks a drawn arrow leaves behind it: none from the one a CS rifle's shot let fly (it is not drawn). */
@Mixin(AbstractArrow.class)
public abstract class CosmeticsArrowSparksMixin {
    @WrapOperation(method = "tick", at = @At(value = "INVOKE", target = "Lnet/minecraft/world/level/Level;addParticle(Lnet/minecraft/core/particles/ParticleOptions;DDDDDD)V"))
    private void cursed$noSparks(Level level, ParticleOptions particle, double x, double y, double z, double dx, double dy, double dz, Operation<Void> original) {
        if (!Cosmetics.hidden((AbstractArrow) (Object) this)) {
            original.call(level, particle, x, y, z, dx, dy, dz);
        }
    }
}
