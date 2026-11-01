package cursed.recode.mixin.client;

import cursed.recode.fx.NoRender;
import net.minecraft.client.renderer.ScreenEffectRenderer;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** No Render (cursed.recode.fx.NoRender): the fire, the water, a block's inside over the view; the totem's animation. */
@Mixin(ScreenEffectRenderer.class)
public abstract class NoRenderScreenMixin {
    @Inject(method = "submitFire", at = @At("HEAD"), cancellable = true)
    private static void cursed$fire(CallbackInfo ci) {
        if (NoRender.on("Fire")) {
            ci.cancel();
        }
    }

    @Inject(method = "submitWater", at = @At("HEAD"), cancellable = true)
    private static void cursed$water(CallbackInfo ci) {
        if (NoRender.on("Water overlay")) {
            ci.cancel();
        }
    }

    @Inject(method = "submitBlockSprite", at = @At("HEAD"), cancellable = true)
    private static void cursed$inside(CallbackInfo ci) {
        if (NoRender.on("Inside a block")) {
            ci.cancel();
        }
    }

    @Inject(method = "renderItemActivationAnimation", at = @At("HEAD"), cancellable = true)
    private void cursed$totem(CallbackInfo ci) {
        if (NoRender.on("Totem animation")) {
            ci.cancel();
        }
    }
}
