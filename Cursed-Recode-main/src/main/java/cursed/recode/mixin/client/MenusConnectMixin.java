package cursed.recode.mixin.client;

import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.screens.ConnectScreen;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.multiplayer.ServerData;
import net.minecraft.client.multiplayer.TransferState;
import net.minecraft.client.multiplayer.resolver.ServerAddress;
import org.jspecify.annotations.Nullable;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** The server being connected to, for the menus' connecting page (the game's screen does not keep it). */
@Mixin(ConnectScreen.class)
public abstract class MenusConnectMixin {
    @Inject(method = "startConnecting", at = @At("HEAD"))
    private static void cursed$connecting(Screen parent, Minecraft minecraft, ServerAddress hostAndPort, ServerData data, boolean isQuickPlay, @Nullable TransferState transferState,
            CallbackInfo ci) {
        cursed.recode.menus.NoticePage.connecting = data;
        cursed.recode.launch.Transition.beginEnter(data.name.isBlank() ? data.ip : data.name, 1);
    }
}
