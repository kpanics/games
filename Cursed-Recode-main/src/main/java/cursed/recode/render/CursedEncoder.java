package cursed.recode.render;

import com.mojang.blaze3d.systems.RenderSystem;
import com.mojang.renderpearl.api.buffers.GpuBuffer;
import com.mojang.renderpearl.api.buffers.GpuBufferSlice;
import com.mojang.renderpearl.api.buffers.TransientMemory;
import com.mojang.renderpearl.api.commands.GpuFence;
import com.mojang.renderpearl.api.commands.GpuQueryPool;
import com.mojang.renderpearl.api.commands.RenderPassDescriptor;
import com.mojang.renderpearl.api.textures.GpuTexture;
import com.mojang.renderpearl.backend.api.CommandEncoderBackend;
import com.mojang.renderpearl.backend.api.RenderPassBackend;
import java.nio.ByteBuffer;
import java.util.ArrayDeque;
import java.util.Optional;
import java.util.OptionalDouble;
import java.util.function.LongConsumer;
import org.joml.Vector4fc;
import org.lwjgl.system.MemoryUtil;

/**
 * The game's one command encoder. It does not record anything itself: every command goes into the frame's stream
 * (`ops`), which the DLL's frame thread records while the game already builds the next frame (vk/frame.rs lists the
 * commands; this class and CursedPass write the same list).
 */
final class CursedEncoder implements CommandEncoderBackend {
    static final int PASS_BEGIN = 100;
    static final int PASS_END = 101;
    static final int CLEAR_COLOR = 110;
    static final int CLEAR_DEPTH = 111;
    static final int CLEAR_BOTH = 112;
    static final int CLEAR_REGION = 113;
    static final int COPY_BUFFER = 114;
    static final int BUFFER_TO_TEXTURE = 115;
    static final int TEXTURE_TO_BUFFER = 116;
    static final int TEXTURE_TO_TEXTURE = 117;
    static final int TIMESTAMP = 118;
    static final int INIT_TEXTURE = 119;
    static final int INIT_COPY = 120;
    static final int CLOSE = 121;
    static final int TERRAIN = 122;
    static final int ACQUIRE = 123;
    static final int BLIT = 124;
    static final int PRESENT = 126;
    static final int MODELS = 127;
    static final int ENGINE = 128;
    static final int UPSCALE = 129;
    static final int COPY_REGIONS = 130;
    static final int LAUNCH = 131;
    static final int GUI = 132;
    static final int INK = 133;
    static final int TARGET = 134;
    static final int HITBOXES = 135;
    static final int FX = 136;
    static final int VIEWMODEL = 137;
    static final int THIRD = 138;
    /** A piece goes over to the frame thread once the stream is this long (a pass may span pieces). */
    private static final int PIECE = 512 * 1024;

    private final CursedDevice device;
    private final long r;
    private final long reply;
    private final CursedTransient transientMemory;
    private final CursedPass pass;
    final Pack ops = new Pack(PIECE + 64 * 1024);
    private final Pack descriptor = new Pack(512);
    private boolean inPass;
    /** The frame being described, the same number the frame thread counts (it starts at 2). */
    long submitIndex;
    /** Whether GPU work was described since the last submit (createFence: whose completion a fence waits for). */
    private boolean described;
    /** Readbacks waiting for their frame to finish on the GPU: the frame, then what to run. */
    private final ArrayDeque<Object[]> callbacks = new ArrayDeque<>();

    CursedEncoder(CursedDevice device) {
        this.device = device;
        this.r = device.r;
        this.reply = Native.reply(this.r);
        this.submitIndex = Native.number(this.r, 13);
        this.transientMemory = new CursedTransient(device, this);
        this.pass = new CursedPass(device, this);
    }

    /** Room for a command of `bytes`; the stream goes over first if it is long enough. */
    Pack op(int code, int bytes) {
        if (code != PRESENT && code != CLOSE && code != ACQUIRE) {
            this.described = true;
        }
        if (this.ops.length() >= PIECE) {
            this.flush();
        }
        this.ops.ensure(bytes + 4);
        return this.ops.i32(code);
    }

    /** What is written so far goes to the frame thread. */
    void flush() {
        if (!this.ops.isEmpty()) {
            Native.frameOps(this.r, this.ops.address(), this.ops.length());
            this.ops.reset();
        }
    }

    /** Closes an object in its turn in the frame (after the commands that use it); off the render thread, at once. */
    void close(int kind, long handle) {
        if (RenderSystem.isOnRenderThread()) {
            this.op(CLOSE, 12).i32(kind).i64(handle);
        } else {
            Native.closeLater(this.r, kind, handle);
        }
    }

    /** Copies `length` bytes at `address` into the game's staging memory; the piece is in the reply. */
    private void stage(long address, long length) {
        Native.stage(this.r, address, length);
    }

    private long stagedBuffer() {
        return MemoryUtil.memGetLong(this.reply);
    }

    private long stagedOffset() {
        return MemoryUtil.memGetLong(this.reply + 8);
    }

    void initTexture(long texture) {
        if (RenderSystem.isOnRenderThread()) {
            this.op(INIT_TEXTURE, 8).i64(texture);
            return;
        }
        // Made off the render thread: its first layout goes over at once, ahead of any frame that can use it.
        ByteBuffer b = MemoryUtil.memAlloc(12).order(java.nio.ByteOrder.LITTLE_ENDIAN);
        try {
            b.putInt(INIT_TEXTURE).putLong(texture).flip();
            Native.frameOps(this.r, MemoryUtil.memAddress(b), 12);
        } finally {
            MemoryUtil.memFree(b);
        }
    }

    /** A new buffer's first content. */
    void initBuffer(long buffer, ByteBuffer data) {
        int length = data.remaining();
        withAddress(data, address -> this.stage(address, length));
        this.op(INIT_COPY, 32).i64(this.stagedBuffer()).i64(this.stagedOffset()).i64(buffer).i64(length);
    }

    /** Model jobs (cursed.recode.scene.ModelJobs) fill their vertices in `target` (vk/models.rs). */
    void models(long jobs, int count, long target) {
        Native.stageStorage(this.r, jobs, (long) count * cursed.recode.scene.ModelJobs.JOB);
        this.op(MODELS, 28).i64(this.stagedBuffer()).i64(this.stagedOffset()).i32(count).i64(target);
    }

    void terrain(Pack visible, int count, boolean ordered, long number, double cameraX, double cameraY, double cameraZ, float[] viewProj) {
        int bytes = visible.length();
        Pack p = this.op(TERRAIN, 104 + bytes).i32(count).i32(ordered ? 1 : 0).i64(number).f64(cameraX).f64(cameraY).f64(cameraZ);
        for (float v : viewProj) {
            p.f32(v);
        }
        p.copy(visible.address(), bytes);
    }

    /** The engine's frame (engine/gpu.rs): the camera, the world's height, the sun, the moon and the sky, its switches. */
    void engine(double cx, double cy, double cz, int minY, int height, float[] sun, float[] sunColor, float[] moon, float[] sky, float time, int flags, float scale, float[] mover,
                float[] casters, int casterCount, float cloudDrift, float cloudHeight, boolean clouds, float wet) {
        Pack p = this.op(ENGINE, 144 + casterCount * 24).f64(cx).f64(cy).f64(cz).i32(minY).i32(height);
        for (float[] v : new float[][]{sun, sunColor, moon, sky}) {
            p.f32(v[0]).f32(v[1]).f32(v[2]).f32(v[3]);
        }
        p.f32(time).i32(flags).f32(scale).f32(mover[0]).f32(mover[1]).f32(mover[2]).f32(mover[3]);
        // The shadow casters: centre from the camera's block, half size.
        p.i32(casterCount);
        for (int i = 0; i < casterCount * 6; i++) {
            p.f32(casters[i]);
        }
        // The clouds: how far they drifted along x, their bottom, whether they are shown.
        p.f32(cloudDrift).f32(cloudHeight).f32(clouds ? 1.0F : 0.0F);
        // How wet the ground is (0..1).
        p.f32(wet);
    }

    /** The level's frames put together into `out` (engine/upscale.rs): its colour and depth, this frame's move. */
    void upscale(long color, long depth, long out, float jx, float jy) {
        this.op(UPSCALE, 32).i64(color).i64(depth).i64(out).f32(jx).f32(jy);
    }

    /** Chams' ink over the level's picture (engine/ink.rs): the fog's colour, the bodies that let ink go. */
    void ink(long color, long depth, float[] fog, float[] bodies, int count, float[] boxes, int boxCount) {
        Pack p = this.op(INK, 16 + 16 + 4 + count * 108 + 4 + boxCount * 64).i64(color).i64(depth).f32(fog[0]).f32(fog[1]).f32(fog[2]).f32(fog[3]).i32(count);
        for (int i = 0; i < count * 27; i++) {
            p.f32(bodies[i]);
        }
        // the liquid's boxes (cursed.recode.chams.ChamsLiquid), 16 numbers each
        p.i32(boxCount);
        for (int i = 0; i < boxCount * 16; i++) {
            p.f32(boxes[i]);
        }
    }

    /** Target ESP over the level's picture (engine/tesseract.rs): each target's id and what it is doing, its feet (from
     *  the camera), its width, height and body's yaw, its model's boxes this frame (16 numbers each). */
    void target(long color, long depth, int[] ints, float[] floats, float[][] boxes, int count) {
        int bytes = 16 + 4;
        for (int i = 0; i < count; i++) {
            bytes += 36 + ints[i * 3 + 2] * 64;
        }
        Pack p = this.op(TARGET, bytes).i64(color).i64(depth).i32(count);
        for (int i = 0; i < count; i++) {
            p.i32(ints[i * 3]).i32(ints[i * 3 + 1]);
            for (int k = 0; k < 6; k++) {
                p.f32(floats[i * 6 + k]);
            }
            int n = ints[i * 3 + 2];
            p.i32(n);
            for (int k = 0; k < n * 16; k++) {
                p.f32(boxes[i][k]);
            }
        }
    }

    /** Hitboxes over the level's picture (engine/hitboxes.rs): each box's id, kind and bits, its lowest and highest
     *  corners (from the camera), its eyes' height, its look, its health and its hurt (12 numbers each). */
    void hitboxes(long color, long depth, int[] ints, float[] floats, int count) {
        Pack p = this.op(HITBOXES, 20 + count * 60).i64(color).i64(depth).i32(count);
        for (int i = 0; i < count; i++) {
            p.i32(ints[i * 3]).i32(ints[i * 3 + 1]).i32(ints[i * 3 + 2]);
            for (int k = 0; k < 12; k++) {
                p.f32(floats[i * 12 + k]);
            }
        }
    }

    /** The world's effects over the level's picture (engine/fx.rs): each shape's four vectors and its colour (20
     *  numbers each). */
    void fx(long color, long depth, float[] floats, int count) {
        Pack p = this.op(FX, 20 + count * 80).i64(color).i64(depth).i32(count);
        for (int i = 0; i < count * 20; i++) {
            p.f32(floats[i]);
        }
    }

    /** The cosmetics in the hands over the picture (native/src/cosmetics/): the view's yaw and pitch, the player's speed,
     *  flags (on the ground, sprinting, sneaking), the light at the eye (sky, block: 0..1). */
    void viewmodel(long color, float yaw, float pitch, float vx, float vy, float vz, int flags, float sky, float block) {
        this.op(VIEWMODEL, 8 + 32).i64(color).f32(yaw).f32(pitch).f32(vx).f32(vy).f32(vz).i32(flags).f32(sky).f32(block);
    }

    /** CS's item in the player's hand seen from behind (native/src/cosmetics/view.rs third): the level's picture and
     *  depth, the hold (16 numbers, from the camera), the light at the player (sky, block: 0..1). */
    void third(long color, long depth, float[] pose, float sky, float block) {
        Pack p = this.op(THIRD, 16 + 64 + 8).i64(color).i64(depth);
        for (int i = 0; i < 16; i++) {
            p.f32(pose[i]);
        }
        p.f32(sky).f32(block);
    }

    /** The main menu's picture (launch/mod.rs) into `out`: the card's second, the menu's state, its lines of light. */
    void launch(long out, float t, float fx, float life, float dt, int flags, float gather, float[] cursor, float[] look, float boost, float flashC, float fade, float ui,
                float left, float footTop, float[] hover, float hoverSparks, float[] lines, int lineCount, float[] icons, float[] glyphs, int glyphCount,
                float @org.jspecify.annotations.Nullable [] knobs, int phase, float phaseT, float load, float loadedAt, long world) {
        Pack p = this.op(LAUNCH, 8 + 4 * 22 + 40 + 8 + lineCount * 32 + 96 + glyphCount * 64 + 4 + (knobs != null ? 29 * 4 : 0) + 24).i64(out).f32(t).f32(fx).f32(life).f32(dt).i32(flags).f32(gather);
        p.f32(cursor[0]).f32(cursor[1]).f32(cursor[2]).f32(look[0]).f32(look[1]);
        p.f32(boost).f32(flashC).f32(fade).f32(ui).f32(left).f32(footTop);
        for (int i = 0; i < 10; i++) {
            p.f32(hover[i]);
        }
        p.f32(hoverSparks).i32(lineCount);
        for (int i = 0; i < lineCount * 8; i++) {
            p.f32(lines[i]);
        }
        for (int i = 0; i < 24; i++) {
            p.f32(icons[i]);
        }
        p.i32(glyphCount);
        for (int i = 0; i < glyphCount * 16; i++) {
            p.f32(glyphs[i]);
        }
        // the menus' screens' world: 24 knobs and the camera's place (5); none: the main menu's
        p.i32(knobs != null ? 1 : 0);
        if (knobs != null) {
            for (int i = 0; i < 29; i++) {
                p.f32(knobs[i]);
            }
        }
        // v12: the scene, its second, the loading, the world's frame
        p.i32(phase).f32(phaseT).f32(load).f32(loadedAt).i64(world);
    }

    /** The client's menu (gui/render.rs) into `out`, its glass over `picture` (the game's, 0 none): frame `id` made by
     *  Native.guiFrame; 0 lets its pictures go. */
    void gui(long out, long picture, int id) {
        this.op(GUI, 20).i64(out).i64(picture).i32(id);
    }

    void acquire(long surface) {
        this.op(ACQUIRE, 8).i64(surface);
    }

    void blit(long surface, long view) {
        this.op(BLIT, 16).i64(surface).i64(view);
    }

    /** Present follows the frame's submit at once: it goes over now, not with the next frame. */
    void present(long surface) {
        this.op(PRESENT, 8).i64(surface);
        this.flush();
    }

    @Override
    public void submit() {
        if (this.inPass) {
            throw new IllegalStateException("Cannot submit while a render pass is open");
        }
        cursed.recode.dev.FeatureTimes.start("java waits for the frame thread");
        long completed = Native.frameSubmit(this.r, this.ops.address(), this.ops.length());
        cursed.recode.dev.FeatureTimes.stop("java waits for the frame thread");
        this.ops.reset();
        this.submitIndex++;
        this.described = false;
        while (!this.callbacks.isEmpty() && (long) this.callbacks.peekFirst()[0] <= completed) {
            ((Runnable) this.callbacks.pollFirst()[1]).run();
        }
        if (this.device.debug) {
            this.device.logMessages();
        }
    }

    @Override
    public TransientMemory transientMemory() {
        return this.transientMemory;
    }

    @Override
    public RenderPassBackend createRenderPass(RenderPassDescriptor descriptor) {
        Pack p = this.descriptor;
        p.reset();
        p.str(this.device.debug ? descriptor.label().get() : "");
        p.i32(descriptor.colorAttachments().size());
        for (RenderPassDescriptor.Attachment<Optional<Vector4fc>> a : descriptor.colorAttachments()) {
            if (a == null) {
                p.i32(0).i64(0L).i32(0).f32(0).f32(0).f32(0).f32(0);
                continue;
            }
            p.i32(1).i64(CursedView.handle(a.textureView()));
            if (a.clearValue().isPresent()) {
                Vector4fc c = a.clearValue().get();
                p.i32(1).f32(c.x()).f32(c.y()).f32(c.z()).f32(c.w());
            } else {
                p.i32(0).f32(0).f32(0).f32(0).f32(0);
            }
        }
        RenderPassDescriptor.Attachment<OptionalDouble> depth = descriptor.depthAttachment();
        if (depth != null) {
            p.i32(1).i64(CursedView.handle(depth.textureView()));
            if (depth.clearValue().isPresent()) {
                p.i32(1).f64(depth.clearValue().getAsDouble());
            } else {
                p.i32(0).f64(0.0);
            }
        } else {
            p.i32(0);
        }
        p.i32(descriptor.renderArea().x()).i32(descriptor.renderArea().y()).i32(descriptor.renderArea().width()).i32(descriptor.renderArea().height());
        int n = p.length();
        this.op(PASS_BEGIN, 8 + n).i32(n).copy(p.address(), n);
        this.inPass = true;
        this.pass.begin();
        return this.pass;
    }

    @Override
    public void submitRenderPass() {
        if (!this.inPass) {
            throw new IllegalStateException("Cannot submit a renderpass if one hasn't been started!");
        }
        this.op(PASS_END, 0);
        this.inPass = false;
    }

    @Override
    public void clearColorTexture(GpuTexture colorTexture, Vector4fc c) {
        this.op(CLEAR_COLOR, 24).i64(CursedTexture.handle(colorTexture)).f32(c.x()).f32(c.y()).f32(c.z()).f32(c.w());
    }

    @Override
    public void clearColorAndDepthTextures(GpuTexture colorTexture, Vector4fc c, GpuTexture depthTexture, double clearDepth) {
        this.op(CLEAR_BOTH, 40).i64(CursedTexture.handle(colorTexture)).f32(c.x()).f32(c.y()).f32(c.z()).f32(c.w())
                .i64(CursedTexture.handle(depthTexture)).f64(clearDepth);
    }

    @Override
    public void clearColorAndDepthTextures(GpuTexture colorTexture, Vector4fc c, GpuTexture depthTexture, double clearDepth, int x, int y, int width, int height, int mipLevel) {
        this.op(CLEAR_REGION, 60).i64(CursedTexture.handle(colorTexture)).f32(c.x()).f32(c.y()).f32(c.z()).f32(c.w())
                .i64(CursedTexture.handle(depthTexture)).f64(clearDepth).i32(x).i32(y).i32(width).i32(height).i32(mipLevel);
    }

    @Override
    public void clearDepthTexture(GpuTexture depthTexture, double clearDepth) {
        this.op(CLEAR_DEPTH, 16).i64(CursedTexture.handle(depthTexture)).f64(clearDepth);
    }

    /** Calls `use` with the native address of `data`'s remaining bytes (copied off the Java heap if needed). */
    static void withAddress(ByteBuffer data, LongConsumer use) {
        if (data.isDirect()) {
            use.accept(MemoryUtil.memAddress(data));
            return;
        }
        ByteBuffer copy = MemoryUtil.memAlloc(data.remaining());
        try {
            copy.put(data.duplicate()).flip();
            use.accept(MemoryUtil.memAddress(copy));
        } finally {
            MemoryUtil.memFree(copy);
        }
    }

    @Override
    public void writeToBuffer(GpuBufferSlice destination, ByteBuffer data) {
        int length = data.remaining();
        withAddress(data, address -> this.stage(address, length));
        this.op(COPY_BUFFER, 40).i64(this.stagedBuffer()).i64(this.stagedOffset())
                .i64(CursedBuffer.handle(destination.buffer())).i64(destination.offset()).i64(length);
    }

    /** count pieces of source into target, each three longs in regions (source offset, target offset, size): one command. */
    void copyRegions(GpuBuffer source, GpuBuffer target, long[] regions, int count) {
        if (count == 0) {
            return;
        }
        Pack p = this.op(COPY_REGIONS, 20 + count * 24).i64(CursedBuffer.handle(source)).i64(CursedBuffer.handle(target)).i32(count);
        for (int i = 0, n = count * 3; i < n; i++) {
            p.i64(regions[i]);
        }
    }

    @Override
    public void copyToBuffer(GpuBufferSlice source, GpuBufferSlice target) {
        this.op(COPY_BUFFER, 40).i64(CursedBuffer.handle(source.buffer())).i64(source.offset())
                .i64(CursedBuffer.handle(target.buffer())).i64(target.offset()).i64(source.length());
    }

    @Override
    public void writeToTexture(GpuTexture destination, ByteBuffer source, int mipLevel, int depthOrLayer, int destX, int destY, int width, int height) {
        int length = source.remaining();
        withAddress(source, address -> this.stage(address, length));
        this.op(BUFFER_TO_TEXTURE, 64).i64(this.stagedBuffer()).i64(this.stagedOffset()).i32(0).i32(0).i32(width).i32(height)
                .i32(destination.getFormat().blockSize()).i64(CursedTexture.handle(destination)).i32(destX).i32(destY).i32(width).i32(height)
                .i32(mipLevel).i32(depthOrLayer);
    }

    @Override
    public void copyBufferToTexture(GpuBufferSlice source, int sourceX, int sourceY, int sourceWidth, int sourceHeight, GpuTexture destination,
                                    int destinationX, int destinationY, int copyWidth, int copyHeight, int mipLevel, int arrayLayer) {
        this.op(BUFFER_TO_TEXTURE, 64).i64(CursedBuffer.handle(source.buffer())).i64(source.offset()).i32(sourceX).i32(sourceY).i32(sourceWidth).i32(sourceHeight)
                .i32(destination.getFormat().blockSize()).i64(CursedTexture.handle(destination)).i32(destinationX).i32(destinationY).i32(copyWidth).i32(copyHeight)
                .i32(mipLevel).i32(arrayLayer);
    }

    @Override
    public void copyTextureToBuffer(GpuTexture source, GpuBuffer destination, long offset, Runnable callback, int mipLevel) {
        this.copyTextureToBuffer(source, destination, offset, callback, mipLevel, 0, 0, source.getWidth(mipLevel), source.getHeight(mipLevel));
    }

    @Override
    public void copyTextureToBuffer(GpuTexture source, GpuBuffer destination, long offset, Runnable callback, int mipLevel, int x, int y, int width, int height) {
        this.op(TEXTURE_TO_BUFFER, 44).i64(CursedTexture.handle(source)).i64(CursedBuffer.handle(destination)).i64(offset)
                .i32(mipLevel).i32(x).i32(y).i32(width).i32(height);
        this.callbacks.addLast(new Object[] {this.submitIndex, callback});
    }

    @Override
    public void copyTextureToTexture(GpuTexture source, GpuTexture destination, int mipLevel, int destX, int destY, int sourceX, int sourceY, int width, int height) {
        this.op(TEXTURE_TO_TEXTURE, 44).i64(CursedTexture.handle(source)).i64(CursedTexture.handle(destination))
                .i32(mipLevel).i32(destX).i32(destY).i32(sourceX).i32(sourceY).i32(width).i32(height);
    }

    /** Dev (CURSED_DEV_WAITS): who waits for the GPU, how long and for a frame how far back (every 5 s). */
    private static final boolean DEV_WAITS = cursed.recode.dev.Dev.on("CURSED_DEV_WAITS");
    private static final java.util.Map<String, long[]> DEV_WAIT_BY = new java.util.HashMap<>();
    private static long devWaitSince;
    private static int devWaitCount;

    private static void devWait(long ns, long back) {
        if (ns < 5_000 && (devWaitCount++ & 63) != 0) {
            return;
        }
        StackTraceElement[] st = Thread.currentThread().getStackTrace();
        StringBuilder who = new StringBuilder();
        for (int i = 3, n = 0; i < st.length && n < 4; i++) {
            String c = st[i].getClassName();
            if (c.contains("CursedEncoder") || c.contains("FrontendCommandEncoder")) {
                continue;
            }
            who.append(c.substring(c.lastIndexOf('.') + 1)).append('.').append(st[i].getMethodName()).append(' ');
            n++;
        }
        long[] v = DEV_WAIT_BY.computeIfAbsent(who + "back " + back, k -> new long[2]);
        v[0] += ns;
        v[1]++;
        long now = System.nanoTime();
        if (now - devWaitSince > 5_000_000_000L) {
            devWaitSince = now;
            DEV_WAIT_BY.entrySet().stream().sorted((x, y) -> Long.compare(y.getValue()[0], x.getValue()[0])).limit(6)
                    .forEach(e -> cursed.recode.CursedRecode.LOGGER.info("[waits] {} us in {} waits: {}", e.getValue()[0] / 1000, e.getValue()[1], e.getKey()));
            DEV_WAIT_BY.clear();
        }
    }

    @Override
    public GpuFence createFence() {
        // A fence waits for the work described before it. Made right after a submit (the game's ring buffers rotate
        // at the frame's end, after it), that is the frame just submitted, not the next one: waiting for the next one
        // let the game describe only two frames ahead of the GPU instead of three (measured 12.10: the game's thread
        // waited 230 µs a frame in MappableRingBuffer with the GPU idle 90 µs of each).
        long frame = this.described ? this.submitIndex : this.submitIndex - 1;
        return new GpuFence() {
            private boolean completed;

            @Override
            public boolean awaitCompletion(long timeoutNs) {
                if (!this.completed) {
                    cursed.recode.dev.FeatureTimes.start("java waits for the GPU");
                    long t0 = DEV_WAITS ? System.nanoTime() : 0L;
                    this.completed = Native.await(CursedEncoder.this.r, frame, timeoutNs);
                    cursed.recode.dev.FeatureTimes.stop("java waits for the GPU");
                    if (DEV_WAITS) {
                        devWait(System.nanoTime() - t0, CursedEncoder.this.submitIndex - frame);
                    }
                }
                return this.completed;
            }

            @Override
            public void close() {
                this.completed = true;
            }
        };
    }

    @Override
    public void writeTimestamp(GpuQueryPool pool, int index) {
        this.op(TIMESTAMP, 12).i64(CursedQueries.handle(pool)).i32(index);
    }
}
