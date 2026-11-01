package cursed.recode.mixin.client;

import net.minecraft.client.renderer.debug.EntityHitboxDebugRenderer;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** The game's F3+B: the client draws the boxes its way (cursed.recode.render.Hitboxes), the game's lines not at all. */
@Mixin(EntityHitboxDebugRenderer.class)
public abstract class EntityHitboxDebugRendererMixin {
    @Inject(method = "emitGizmos", at = @At("HEAD"), cancellable = true)
    private void cursed$ours(CallbackInfo ci) {
        if (cursed.recode.render.Hitboxes.working()) {
            ci.cancel();
        }
    }
}
