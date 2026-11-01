package cursed.recode.mixin.client;

import cursed.recode.play.StreamerMode;
import net.minecraft.client.Minecraft;
import net.minecraft.client.player.AbstractClientPlayer;
import net.minecraft.client.resources.DefaultPlayerSkin;
import net.minecraft.world.entity.player.PlayerSkin;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** Streamer Mode (cursed.recode.play.StreamerMode): the player's own skin the game's default on their screen, while it
 *  hides it (the others see it as it is). */
@Mixin(AbstractClientPlayer.class)
public abstract class StreamerSkinMixin {
    @Inject(method = "getSkin", at = @At("HEAD"), cancellable = true)
    private void cursed$skin(CallbackInfoReturnable<PlayerSkin> cir) {
        if (StreamerMode.hidesSkin() && (Object) this == Minecraft.getInstance().player) {
            cir.setReturnValue(DefaultPlayerSkin.get(((AbstractClientPlayer) (Object) this).getUUID()));
        }
    }
}
