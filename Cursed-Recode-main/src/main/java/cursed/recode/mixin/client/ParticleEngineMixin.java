package cursed.recode.mixin.client;

import cursed.recode.scene.ParticleLight;
import net.minecraft.client.particle.ParticleEngine;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** The particles' extraction: their light kept per block for it (cursed.recode.scene.ParticleLight). */
@Mixin(ParticleEngine.class)
public abstract class ParticleEngineMixin {
    /** No Render (cursed.recode.fx.NoRender): explosions', potions' and fireworks' particles never made. */
    @Inject(method = "createParticle", at = @At("HEAD"), cancellable = true)
    private void cursed$none(net.minecraft.core.particles.ParticleOptions options, double x, double y, double z, double xa, double ya, double za,
                             org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable<net.minecraft.client.particle.Particle> cir) {
        var t = options.getType();
        boolean off = (t == net.minecraft.core.particles.ParticleTypes.EXPLOSION || t == net.minecraft.core.particles.ParticleTypes.EXPLOSION_EMITTER) && cursed.recode.fx.NoRender.on("Explosions")
                || (t == net.minecraft.core.particles.ParticleTypes.ENTITY_EFFECT || t == net.minecraft.core.particles.ParticleTypes.EFFECT || t == net.minecraft.core.particles.ParticleTypes.INSTANT_EFFECT) && cursed.recode.fx.NoRender.on("Potion swirls")
                || (t == net.minecraft.core.particles.ParticleTypes.FIREWORK || t == net.minecraft.core.particles.ParticleTypes.FLASH) && cursed.recode.fx.NoRender.on("Fireworks");
        if (off) {
            cir.setReturnValue(null);
        }
    }

    @Inject(method = "extract", at = @At("HEAD"))
    private void cursed$begin(CallbackInfo ci) {
        ParticleLight.begin();
    }

    @Inject(method = "extract", at = @At("TAIL"))
    private void cursed$end(CallbackInfo ci) {
        ParticleLight.end();
    }
}
