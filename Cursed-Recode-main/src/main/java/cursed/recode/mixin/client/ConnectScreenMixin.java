package cursed.recode.mixin.client;

import cursed.recode.launch.LastServer;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.screens.ConnectScreen;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.multiplayer.ServerData;
import net.minecraft.client.multiplayer.TransferState;
import net.minecraft.client.multiplayer.resolver.ServerAddress;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** A server joined is the menu's «Continue» next time (cursed.recode.launch.LastServer), and the one Via looks at. */
@Mixin(ConnectScreen.class)
public abstract class ConnectScreenMixin {
    @Inject(method = "startConnecting", at = @At("HEAD"))
    private static void cursed$remember(Screen parent, Minecraft minecraft, ServerAddress hostAndPort, ServerData data, boolean isQuickPlay, TransferState transferState,
                                        CallbackInfo ci) {
        if (data != null && data.ip != null && !data.ip.isBlank()) {
            LastServer.remember(data.name, data.ip);
        }
        // the server's version for the translator (cursed.recode.via.Via) when it is older than the client's
        cursed.recode.via.Via.aim(data, parent, isQuickPlay, transferState);
    }
}
