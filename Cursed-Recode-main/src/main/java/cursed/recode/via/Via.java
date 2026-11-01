package cursed.recode.via;

import com.mojang.logging.LogUtils;
import com.viaversion.vialoader.ViaLoader;
import com.viaversion.vialoader.impl.platform.ViaVersionPlatformImpl;
import com.viaversion.viaversion.api.connection.UserConnection;
import com.viaversion.viaversion.api.platform.ViaInjector;
import com.viaversion.viaversion.api.protocol.version.ProtocolVersion;
import com.viaversion.viaversion.api.protocol.version.VersionProvider;
import com.viaversion.viaversion.connection.UserConnectionImpl;
import com.viaversion.viaversion.exception.CancelCodecException;
import com.viaversion.viaversion.exception.CancelDecoderException;
import com.viaversion.viaversion.exception.CancelEncoderException;
import com.viaversion.viaversion.libs.gson.JsonObject;
import com.viaversion.viaversion.protocol.ProtocolPipelineImpl;
import com.viaversion.viaversion.util.PipelineUtil;
import cursed.recode.render.Native;
import io.netty.buffer.ByteBuf;
import io.netty.channel.Channel;
import io.netty.channel.ChannelHandlerContext;
import io.netty.channel.ChannelPipeline;
import io.netty.channel.ChannelPromise;
import io.netty.handler.codec.MessageToMessageDecoder;
import io.netty.handler.codec.MessageToMessageEncoder;
import io.netty.util.AttributeKey;
import java.io.File;
import java.net.InetSocketAddress;
import java.util.List;
import net.minecraft.client.Minecraft;
import net.minecraft.client.multiplayer.ServerData;
import net.minecraft.network.protocol.handshake.ClientIntent;
import org.slf4j.Logger;

/**
 * Servers of older versions: which version the server being joined speaks — the one chosen for it by hand, else its
 * status as the server list has it, else its status asked now — and, when it is older than the client's, ViaVersion
 * (with ViaLoader's platform) put into its connection: each packet off the wire (decrypted, split, decompressed) goes
 * through it before the game's decoder, each the game encoded before it is compressed and framed. The game's pipeline
 * keeps its inbound and outbound handlers apart (splitter, decoder; prepender, encoder), so the translation is two
 * handlers here, not ViaLoader's one codec.
 */
public final class Via {
    private static final Logger LOGGER = LogUtils.getLogger();
    /** The client's own protocol (26.3): nothing to translate. */
    public static final int OWN = 777;
    /** The handlers' names (ViaVersion finds them by these to send packets of its own). */
    static final String IN = "via_in";
    static final String OUT = "via_out";
    /** Packets for the client held while the game is between two protocols (see Hold). */
    static final String HOLD = "via_hold";
    /** The server's version, on its connection's channel (the version provider reads it). */
    private static final AttributeKey<ProtocolVersion> TARGET = AttributeKey.valueOf("cursed_via_target");
    /** The server the connect screen is joining (its thread connects after this is set). */
    private record Join(ServerData data, net.minecraft.client.gui.screens.Screen parent, boolean quickPlay,
                        net.minecraft.client.multiplayer.TransferState transfer) {}
    private record Attempt(Join join, int protocol, long at) {}
    private record Retry(Join join, long at) {}
    private static final AttributeKey<Attempt> ATTEMPT = AttributeKey.valueOf("cursed_via_attempt");
    private static final java.util.concurrent.atomic.AtomicReference<Retry> RETRY = new java.util.concurrent.atomic.AtomicReference<>();
    private static volatile Join aimed;
    private static volatile Join latest;
    private static boolean started;

    private Via() {
    }

    /** The connect screen starts joining a server. */
    public static void aim(ServerData data, net.minecraft.client.gui.screens.Screen parent, boolean quickPlay,
                           net.minecraft.client.multiplayer.TransferState transfer) {
        RETRY.set(null);
        latest = aimed = new Join(data, parent, quickPlay, transfer);
    }

    /** Called on the channel's event loop, before the game closes a failed connection. */
    public static void decodeFailed(Channel channel, Throwable cause) {
        if (channel == null || !(cause instanceof io.netty.handler.codec.DecoderException)) return;
        Attempt attempt = channel.attr(ATTEMPT).getAndSet(null);
        if (attempt == null || attempt.join() != latest || attempt.join().transfer() != null) return;
        ServerData data = attempt.join().data();
        if (data == null) return;
        long age = (System.nanoTime() - attempt.at()) / 1_000_000L;
        if (Compatibility.failed(data.ip, attempt.protocol(), Choices.of(data.ip), age, cause.getMessage())) {
            LOGGER.info("Via: {} advertised a protocol with incompatible packets; Auto will retry 1.21.11 once", data.ip);
            RETRY.set(new Retry(attempt.join(), System.nanoTime()));
        }
    }

    /** Back on the game thread: wait until the failed connection is completely closed. */
    public static void tick(Minecraft mc) {
        Retry retry = RETRY.get();
        if (retry == null) return;
        if (retry.join() != latest || System.nanoTime() - retry.at() > 20_000_000_000L
                || Choices.of(retry.join().data().ip) != 0
                || Compatibility.automatic(retry.join().data().ip, OWN) != Compatibility.FALLBACK) { RETRY.compareAndSet(retry, null); return; }
        if (mc.level != null || !(mc.gui.screen() instanceof net.minecraft.client.gui.screens.DisconnectedScreen)) return;
        // Proxies commonly throttle rapid logins. Give the closed connection time to leave
        // their rate window, then make just the single compatibility attempt.
        if (System.nanoTime() - retry.at() < 5_000_000_000L) return;
        if (!RETRY.compareAndSet(retry, null)) return;
        Join join = retry.join();
        net.minecraft.client.gui.screens.ConnectScreen.startConnecting(join.parent(), mc,
            net.minecraft.client.multiplayer.resolver.ServerAddress.parseString(join.data().ip), join.data(), join.quickPlay(), null);
    }

    /** ViaVersion started, once (its config in the game's config folder). */
    private static synchronized void start() {
        if (started) {
            return;
        }
        started = true;
        File dir = new File(Minecraft.getInstance().gameDirectory, "config/recode-via");
        ViaLoader.init(new ViaVersionPlatformImpl(dir), null, new Injector(), null);
        // each connection's server version: the one set on its channel
        com.viaversion.viaversion.api.Via.getManager().getProviders().use(VersionProvider.class, (VersionProvider) c -> {
            ProtocolVersion v = c.getChannel() == null ? null : c.getChannel().attr(TARGET).get();
            return v != null ? v : ProtocolVersion.getProtocol(OWN);
        });
        LOGGER.info("Via: ready, {} versions", ProtocolVersion.getProtocols().size());
    }

    /**
     * The oldest server joined through it: 1.16.4-1.16.5 (754). Servers before 1.20.2 have no configuration phase:
     * ViaVersion makes one up, keeping the server's packets back meanwhile, and gives them to the client when it says
     * it finished; Hold makes the client take them only once it can answer them.
     */
    public static final int OLDEST = 754;

    /** The oldest version a server can be set to in the list: 1.16.4-1.16.5. */
    public static final int LISTED = 754;

    /** The versions a server can be set to, newest first: the client's own, then each release down to 1.16.4. */
    public static List<ProtocolVersion> releases() {
        List<ProtocolVersion> out = new java.util.ArrayList<>();
        for (ProtocolVersion v : ProtocolVersion.getProtocols()) {
            if (v.getVersionType() == com.viaversion.viaversion.api.protocol.version.VersionType.RELEASE && v.getVersion() >= LISTED && v.getVersion() <= OWN) {
                out.add(v);
            }
        }
        out.sort(java.util.Comparator.comparingInt(ProtocolVersion::getVersion).reversed());
        return out;
    }

    /** A protocol's version as people say it ("1.21.4", "1.20.3-1.20.4"); "" for one not known. */
    public static String name(int protocol) {
        return protocol > 0 && ProtocolVersion.isRegistered(protocol) ? ProtocolVersion.getProtocol(protocol).getName() : "";
    }

    /** Whether a server of this protocol can be joined through the translation (a version ViaVersion knows, older). */
    public static boolean supported(int protocol) {
        if (protocol < OLDEST || protocol == OWN) {
            return false;
        }
        start();
        return ProtocolVersion.isRegistered(protocol) && ProtocolVersion.getProtocol(protocol).olderThan(ProtocolVersion.getProtocol(OWN));
    }

    /**
     * A connection about to say what it is for (cursed.recode.mixin.client.ViaConnectionMixin): the translation in its
     * pipeline when it is to play on a server of an older version.
     */
    public static void attach(Channel channel, ClientIntent intent) {
        if (intent == ClientIntent.STATUS || channel == null || !(channel.remoteAddress() instanceof InetSocketAddress at)) {
            return;
        }
        Join join = aimed;
        aimed = null;
        ServerData data = join == null ? null : join.data();
        int protocol = protocolOf(data, at);
        LOGGER.info("Via: negotiated {} for {}", protocol, data == null ? at.getHostString() : data.ip);
        if (join != null) channel.attr(ATTEMPT).set(new Attempt(join, protocol, System.nanoTime()));
        if (!supported(protocol)) {
            return;
        }
        ProtocolVersion version = ProtocolVersion.getProtocol(protocol);
        channel.attr(TARGET).set(version);
        UserConnection user = new UserConnectionImpl(channel, true);
        new ProtocolPipelineImpl(user);
        ChannelPipeline p = channel.pipeline();
        // after the splitter (the decompressor goes between them when it comes), before the encoder (the compressor
        // goes between it and the prepender)
        p.addAfter("splitter", IN, new In(user));
        p.addAfter(IN, HOLD, new Hold());
        p.addAfter("prepender", OUT, new Out(user));
        LOGGER.info("Via: {} speaks {} ({}), translated", at, version.getName(), protocol);
    }

    /** The server's protocol: chosen by hand, else as the list's status said, else asked now (3 s at most). */
    private static int protocolOf(ServerData data, InetSocketAddress at) {
        int chosen = data == null ? 0 : Choices.of(data.ip);
        if (chosen > 0) {
            return chosen;
        }
        if (data != null && (data.state() == ServerData.State.SUCCESSFUL || data.state() == ServerData.State.INCOMPATIBLE)) {
            return Compatibility.automatic(data.ip, data.protocol);
        }
        if (data != null) {
            int known = Compatibility.automatic(data.ip, OWN);
            if (known != OWN) return known;
        }
        int asked = Native.viaPing(at.getHostString(), at.getPort(), 3000);
        return Compatibility.automatic(data == null ? at.getHostString() : data.ip, asked > 0 ? asked : OWN);
    }

    /** From the server: its packets as the client's version has them. */
    private static final class In extends MessageToMessageDecoder<ByteBuf> {
        private final UserConnection user;

        In(UserConnection user) {
            this.user = user;
        }

        @Override
        protected void decode(ChannelHandlerContext ctx, ByteBuf msg, List<Object> out) throws Exception {
            if (!user.checkIncomingPacket(msg.readableBytes())) {
                throw CancelDecoderException.generate(null);
            }
            ByteBuf b = ctx.alloc().buffer(msg.readableBytes()).writeBytes(msg);
            try {
                if (user.shouldTransformPacket()) {
                    user.transformIncoming(b, CancelDecoderException::generate);
                }
                out.add(b.retain());
            } catch (Throwable e) {
                if (!PipelineUtil.containsCause(e, CancelCodecException.class)) {
                    LOGGER.error("Via: a packet from the server could not be translated", e);
                }
                throw e;
            } finally {
                b.release();
            }
        }

        @Override
        public void channelRead(ChannelHandlerContext ctx, Object msg) throws Exception {
            try {
                super.channelRead(ctx, msg);
            } catch (Throwable e) {
                if (!PipelineUtil.containsCause(e, CancelCodecException.class)) {
                    throw e;
                }
            }
        }
    }

    /**
     * The packets for the client (the server's, translated, and those ViaVersion sends of its own), held while the game
     * is between two protocols: it said it finished one (its encoder or decoder is "unconfigured") and has not set up
     * the next yet. ViaVersion gives the client the packets it kept back from a server older than 1.20.2 the moment the
     * client says it finished configuring — inside that write, before the client's next protocol is set up: a packet
     * the client answers there at once (a keep-alive) had no encoder to go through, and the connection broke. Held, they
     * go on in their order once both are set up (30 s at most: then all of them).
     */
    private static final class Hold extends io.netty.channel.ChannelInboundHandlerAdapter {
        /** Longer than any wait between two protocols (the client builds its registries in one). */
        private static final long MOST = 30_000_000_000L;
        private final java.util.ArrayDeque<Object> held = new java.util.ArrayDeque<>();
        private boolean waiting;
        private long from;

        private static boolean between(ChannelHandlerContext ctx) {
            ChannelPipeline p = ctx.pipeline();
            return p.get("outbound_config") != null || p.get("inbound_config") != null;
        }

        @Override
        public void channelRead(ChannelHandlerContext ctx, Object msg) {
            if (this.held.isEmpty() && !between(ctx)) {
                ctx.fireChannelRead(msg);
                return;
            }
            this.held.add(msg);
            this.go(ctx);
        }

        /** The held ones on, in their order, until the game is between two protocols again (then a look 1 ms on). */
        private void go(ChannelHandlerContext ctx) {
            if (this.waiting) {
                return;
            }
            while (!this.held.isEmpty()) {
                if (between(ctx) && ctx.channel().isActive()) {
                    long now = System.nanoTime();
                    if (this.from == 0) {
                        this.from = now;
                    }
                    if (now - this.from < MOST) {
                        this.waiting = true;
                        ctx.channel().eventLoop().schedule(() -> {
                            this.waiting = false;
                            this.go(ctx);
                        }, 1, java.util.concurrent.TimeUnit.MILLISECONDS);
                        return;
                    }
                }
                ctx.fireChannelRead(this.held.poll());
            }
            this.from = 0;
        }

        @Override
        public void handlerRemoved(ChannelHandlerContext ctx) {
            while (!this.held.isEmpty()) {
                io.netty.util.ReferenceCountUtil.release(this.held.poll());
            }
        }
    }

    /** From the client: its packets as the server's version has them. */
    private static final class Out extends MessageToMessageEncoder<ByteBuf> {
        private final UserConnection user;

        Out(UserConnection user) {
            this.user = user;
        }

        @Override
        protected void encode(ChannelHandlerContext ctx, ByteBuf msg, List<Object> out) throws Exception {
            if (!user.checkOutgoingPacket()) {
                throw CancelEncoderException.generate(null);
            }
            ByteBuf b = ctx.alloc().buffer(msg.readableBytes()).writeBytes(msg);
            try {
                if (user.shouldTransformPacket()) {
                    user.transformOutgoing(b, CancelEncoderException::generate);
                }
                out.add(b.retain());
            } catch (Throwable e) {
                if (!PipelineUtil.containsCause(e, CancelCodecException.class)) {
                    LOGGER.error("Via: a packet of the client could not be translated", e);
                }
                throw e;
            } finally {
                b.release();
            }
        }

        @Override
        public void write(ChannelHandlerContext ctx, Object msg, ChannelPromise promise) throws Exception {
            try {
                super.write(ctx, msg, promise);
            } catch (Throwable e) {
                if (!PipelineUtil.containsCause(e, CancelCodecException.class)) {
                    throw e;
                }
                promise.setSuccess();
            }
        }
    }

    /** Where ViaVersion finds a connection's handlers (it sends packets of its own from them); nothing to inject. */
    private static final class Injector implements ViaInjector {
        @Override
        public void inject() {
        }

        @Override
        public void uninject() {
        }

        @Override
        public ProtocolVersion getServerProtocolVersion() {
            return ProtocolVersion.getProtocol(OWN);
        }

        @Override
        public String getEncoderName() {
            return OUT;
        }

        @Override
        public String getDecoderName() {
            return IN;
        }

        @Override
        public JsonObject getDump() {
            return new JsonObject();
        }
    }
}
