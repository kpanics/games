package cursed.recode.mixin.client;

import io.netty.channel.Channel;
import net.minecraft.network.ClientboundPacketListener;
import net.minecraft.network.Connection;
import net.minecraft.network.ProtocolInfo;
import net.minecraft.network.protocol.handshake.ClientIntent;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** A connection about to send its handshake: the translator in it for a server of an older version (cursed.recode.via.Via). */
@Mixin(Connection.class)
public abstract class ViaConnectionMixin {
    @Shadow
    private Channel channel;

    @Inject(method = "exceptionCaught", at = @At("HEAD"))
    private void cursed$decodeReport(io.netty.channel.ChannelHandlerContext ctx, Throwable cause, CallbackInfo ci) {
        if (cause instanceof io.netty.handler.codec.DecoderException) {
            // The game's normal log keeps only the outer packet name. Preserve its actual
            // decoding cause so a server/proxy format error can be diagnosed from reports.
            com.mojang.logging.LogUtils.getLogger().warn("Server packet decoding failed", cause);
            cursed.recode.via.Via.decodeFailed(this.channel, cause);
        }
    }

    @Inject(method = "initiateServerboundConnection", at = @At("HEAD"))
    private void cursed$via(String hostName, int port, ProtocolInfo<?> outbound, ProtocolInfo<?> inbound, ClientboundPacketListener listener, ClientIntent intent, CallbackInfo ci) {
        // A connect future can finish before channelActive has assigned Connection.channel.
        // Queue translation before vanilla queues its handshake, rather than skipping it when
        // this field is still null (particularly common on a quick reconnect).
        ((Connection) (Object) this).runOnceConnected(connection -> cursed.recode.via.Via.attach(this.channel, intent));
    }
}
