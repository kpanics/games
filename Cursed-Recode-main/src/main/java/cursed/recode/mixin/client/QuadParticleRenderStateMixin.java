package cursed.recode.mixin.client;

import com.mojang.blaze3d.vertex.VertexConsumer;
import cursed.recode.scene.NativeParticles;
import net.minecraft.client.particle.SingleQuadParticle;
import net.minecraft.client.renderer.state.level.QuadParticleRenderState;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Particles' quads are written natively (cursed.recode.scene.NativeParticles). */
@Mixin(QuadParticleRenderState.class)
public abstract class QuadParticleRenderStateMixin {
    @Inject(method = "buildLayer", at = @At("HEAD"), cancellable = true)
    private void cursed$build(SingleQuadParticle.Layer layer, VertexConsumer bufferBuilder, CallbackInfo ci) {
        if (NativeParticles.build((QuadParticleRenderState) (Object) this, layer, bufferBuilder)) {
            ci.cancel();
        }
    }
}
