package cursed.recode.mixin.client;

import cursed.recode.hud.HudLayer;
import net.minecraft.resources.Identifier;
import net.minecraft.world.item.ItemCooldowns;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** A cooldown started: the HUD's Cooldowns element learns how long it is. */
@Mixin(ItemCooldowns.class)
public abstract class ItemCooldownsMixin {
    @Inject(method = "addCooldown(Lnet/minecraft/resources/Identifier;I)V", at = @At("HEAD"))
    private void cursed$started(Identifier group, int time, CallbackInfo ci) {
        HudLayer.cooldown((ItemCooldowns) (Object) this, group, time);
    }
}
