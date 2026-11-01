package cursed.recode.mixin.client;

import cursed.recode.scene.ParticleLight;
import net.minecraft.client.multiplayer.ClientLevel;
import net.minecraft.client.particle.Particle;
import org.spongepowered.asm.mixin.Final;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** A particle's light from the frame's per-block answers while the particles are extracted (cursed.recode.scene.ParticleLight). */
@Mixin(Particle.class)
public abstract class ParticleMixin {
    @Shadow @Final protected ClientLevel level;
    @Shadow protected double x;
    @Shadow protected double y;
    @Shadow protected double z;

    @Inject(method = "getLightCoords", at = @At("HEAD"), cancellable = true)
    private void cursed$light(float a, CallbackInfoReturnable<Integer> cir) {
        if (ParticleLight.on()) {
            cir.setReturnValue(ParticleLight.light(this.level, this.x, this.y, this.z));
        }
    }
}
