package cursed.recode.mixin.client;

import cursed.recode.fx.FakePlayers;
import net.minecraft.client.Minecraft;
import net.minecraft.client.player.AbstractClientPlayer;
import net.minecraft.world.entity.player.PlayerSkin;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** Fake Player's doubles (cursed.recode.fx.FakePlayers) wear the player's skin, or the nick's once it has come. */
@Mixin(AbstractClientPlayer.class)
public abstract class FakeSkinMixin {
    @Inject(method = "getSkin", at = @At("HEAD"), cancellable = true)
    private void cursed$double(CallbackInfoReturnable<PlayerSkin> cir) {
        if (FakePlayers.number((AbstractClientPlayer) (Object) this) >= 0) {
            PlayerSkin s = FakePlayers.skin(Minecraft.getInstance(), (AbstractClientPlayer) (Object) this);
            if (s != null) {
                cir.setReturnValue(s);
            }
        }
    }
}
