package cursed.recode.mixin.client;

import cursed.recode.chams.ChamsHooks;
import net.minecraft.client.renderer.FirstPersonHandsAndItemsRenderer;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Chams (cursed.recode.chams.Chams): the first person's hands and what they hold are a chams' body too. */
@Mixin(FirstPersonHandsAndItemsRenderer.class)
public abstract class ChamsHandsMixin {
    @Inject(method = "submitHandsWithItems", at = @At("HEAD"))
    private void cursed$chamsHands(CallbackInfo ci) {
        ChamsHooks.handsStart();
    }

    @Inject(method = "submitHandsWithItems", at = @At("RETURN"))
    private void cursed$chamsHandsDone(CallbackInfo ci) {
        ChamsHooks.handsDone();
    }
}
