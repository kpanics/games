package cursed.recode.mixin.client;

import net.minecraft.client.renderer.debug.ChunkBorderRenderer;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** The game's F3+G: the client draws the chunks' borders its way (cursed.recode.fx.ChunkGrid), the game's lines not at all. */
@Mixin(ChunkBorderRenderer.class)
public abstract class ChunkBorderRendererMixin {
    @Inject(method = "emitGizmos", at = @At("HEAD"), cancellable = true)
    private void cursed$ours(CallbackInfo ci) {
        if (cursed.recode.fx.Fx.working()) {
            ci.cancel();
        }
    }
}
