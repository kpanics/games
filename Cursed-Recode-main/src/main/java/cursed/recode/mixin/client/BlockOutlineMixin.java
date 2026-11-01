package cursed.recode.mixin.client;

import net.minecraft.client.renderer.LevelRenderer;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** The game's outline of the block looked at: not drawn while Block Outline draws its own (cursed.recode.fx.BlockOutline). */
@Mixin(LevelRenderer.class)
public abstract class BlockOutlineMixin {
    @Inject(method = "submitBlockOutline", at = @At("HEAD"), cancellable = true)
    private void cursed$ours(CallbackInfo ci) {
        if (cursed.recode.fx.BlockOutline.drawing()) {
            ci.cancel();
        }
    }
}
