package cursed.recode.mixin.client;

import cursed.recode.chams.Chams;
import net.minecraft.client.renderer.entity.layers.HumanoidArmorLayer;
import net.minecraft.client.renderer.entity.layers.CustomHeadLayer;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/**
 * Chams (cursed.recode.chams.Chams): Armour, and what is worn on the head, is a part of its own while its layer is submitted, so it goes in the look as
 * the player wants (what no layer draws is the body).
 */
@Mixin({HumanoidArmorLayer.class, CustomHeadLayer.class})
public abstract class ChamsArmourMixin {
    @Inject(method = "submit", at = @At("HEAD"))
    private void cursed$begin(CallbackInfo ci) {
        Chams.partBegin(Chams.ARMOUR);
    }

    @Inject(method = "submit", at = @At("RETURN"))
    private void cursed$end(CallbackInfo ci) {
        Chams.partEnd();
    }
}
