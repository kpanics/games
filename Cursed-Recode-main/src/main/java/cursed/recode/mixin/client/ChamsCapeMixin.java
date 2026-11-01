package cursed.recode.mixin.client;

import cursed.recode.chams.Chams;
import net.minecraft.client.renderer.entity.layers.CapeLayer;
import net.minecraft.client.renderer.entity.layers.WingsLayer;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/**
 * Chams (cursed.recode.chams.Chams): The cape and the elytra is a part of its own while its layer is submitted, so it goes in the look as
 * the player wants (what no layer draws is the body).
 */
@Mixin({CapeLayer.class, WingsLayer.class})
public abstract class ChamsCapeMixin {
    @Inject(method = "submit", at = @At("HEAD"))
    private void cursed$begin(CallbackInfo ci) {
        Chams.partBegin(Chams.CAPE);
    }

    @Inject(method = "submit", at = @At("RETURN"))
    private void cursed$end(CallbackInfo ci) {
        Chams.partEnd();
    }
}
