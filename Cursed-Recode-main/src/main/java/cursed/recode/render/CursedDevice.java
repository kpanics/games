package cursed.recode.render;

import com.mojang.logging.LogUtils;
import com.mojang.renderpearl.api.GpuFormat;
import com.mojang.renderpearl.api.buffers.GpuBuffer;
import com.mojang.renderpearl.api.commands.GpuQueryPool;
import com.mojang.renderpearl.api.device.DeviceFeatures;
import com.mojang.renderpearl.api.device.DeviceInfo;
import com.mojang.renderpearl.api.device.DeviceLimits;
import com.mojang.renderpearl.api.device.DeviceType;
import com.mojang.renderpearl.api.device.HintsAndWorkarounds;
import com.mojang.renderpearl.api.textures.AddressMode;
import com.mojang.renderpearl.api.textures.FilterMode;
import com.mojang.renderpearl.api.textures.GpuSampler;
import com.mojang.renderpearl.api.textures.GpuTexture;
import com.mojang.renderpearl.api.textures.GpuTextureView;
import com.mojang.renderpearl.backend.api.BackendRenderPipeline;
import com.mojang.renderpearl.backend.api.CommandEncoderBackend;
import com.mojang.renderpearl.backend.api.GpuDeviceBackend;
import com.mojang.renderpearl.backend.api.GpuSurfaceBackend;
import java.nio.ByteBuffer;
import java.nio.charset.StandardCharsets;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.OptionalDouble;
import java.util.Set;
import java.util.function.BooleanSupplier;
import java.util.function.Supplier;
import org.jspecify.annotations.Nullable;
import org.lwjgl.sdl.SDLProperties;
import org.lwjgl.sdl.SDLVideo;
import org.lwjgl.system.MemoryUtil;
import org.slf4j.Logger;

final class CursedDevice implements GpuDeviceBackend {
    private static final Logger LOGGER = LogUtils.getLogger();
    final long r;
    final boolean debug;
    private final DeviceInfo info;
    final CursedEncoder encoder;
    /** The one device of the running game (null until it is made, and with Mojang's renderers in dev comparisons). */
    static volatile @Nullable CursedDevice current;

    CursedDevice(long r) {
        this.r = r;
        this.debug = Native.number(r, 11) != 0;
        Set<String> extensions = new LinkedHashSet<>(List.of(Native.text(r, 3).split("\n")));
        DeviceType type = switch ((int) Native.number(r, 0)) {
            case 1 -> DeviceType.INTEGRATED;
            case 2 -> DeviceType.DISCRETE;
            case 3 -> DeviceType.VIRTUAL;
            case 4 -> DeviceType.CPU;
            default -> DeviceType.OTHER;
        };
        boolean multiDraw = Native.number(r, 10) != 0;
        this.info = new DeviceInfo(
                Native.text(r, 0),
                Native.text(r, 1),
                Native.text(r, 2),
                true,
                "Vulkan",
                Float.intBitsToFloat((int) Native.number(r, 1)),
                new DeviceLimits(
                        (int) Native.number(r, 2),
                        (int) Native.number(r, 3),
                        (int) Native.number(r, 4),
                        Native.number(r, 5),
                        (int) Native.number(r, 6),
                        (int) Native.number(r, 7),
                        (int) Native.number(r, 8)),
                new DeviceFeatures(Native.number(r, 9) != 0, true, multiDraw, false, true, true, true, true),
                java.util.Collections.unmodifiableSet(extensions),
                new HintsAndWorkarounds(false, false, false, false),
                type);
        this.encoder = new CursedEncoder(this);
        current = this;
        LOGGER.info("Cursed Recode renderer on {} (Resizable BAR memory: {})", this.info.name(), Native.number(r, 12) != 0);
        this.logMessages();
    }

    /** Validation and driver messages the DLL collected, into the game's log. */
    void logMessages() {
        String messages = Native.messages();
        if (messages == null) {
            return;
        }
        for (String line : messages.split("\n")) {
            if (line.startsWith("E ")) {
                LOGGER.error("[Vulkan] {}", line.substring(2));
            } else if (line.startsWith("W ")) {
                LOGGER.warn("[Vulkan] {}", line.substring(2));
            } else {
                LOGGER.info("[Vulkan] {}", line.length() > 2 ? line.substring(2) : line);
            }
        }
    }

    /** Names an object for validation messages and RenderDoc; kind 0 buffer, 1 texture. */
    void name(int kind, long object, @Nullable String label) {
        if (!this.debug || label == null || label.isEmpty()) {
            return;
        }
        byte[] bytes = label.getBytes(StandardCharsets.UTF_8);
        ByteBuffer buf = MemoryUtil.memAlloc(bytes.length);
        try {
            buf.put(bytes).flip();
            Native.name(this.r, kind, object, MemoryUtil.memAddress(buf), bytes.length);
        } finally {
            MemoryUtil.memFree(buf);
        }
    }

    @Override
    public GpuSurfaceBackend createSurface(long windowHandle, BooleanSupplier isIconified) {
        int props = SDLVideo.SDL_GetWindowProperties(windowHandle);
        long hwnd = SDLProperties.SDL_GetPointerProperty(props, SDLVideo.SDL_PROP_WINDOW_WIN32_HWND_POINTER, 0L);
        if (hwnd == 0L) {
            throw new IllegalStateException("The window has no Win32 handle");
        }
        return new CursedSurface(this, Native.surfaceCreate(this.r, hwnd));
    }

    @Override
    public CommandEncoderBackend createCommandEncoder() {
        return this.encoder;
    }

    @Override
    public GpuSampler createSampler(AddressMode u, AddressMode v, FilterMode min, FilterMode mag, int maxAnisotropy, OptionalDouble maxLod) {
        return new CursedSampler(this, u, v, min, mag, maxAnisotropy, maxLod);
    }

    @Override
    public GpuTexture createTexture(@Nullable String label, @GpuTexture.Usage int usage, GpuFormat format, int width, int height, int depthOrLayers, int mipLevels) {
        return new CursedTexture(this, usage, this.debug && label != null ? label : "", format, width, height, depthOrLayers, mipLevels);
    }

    @Override
    public GpuTextureView createTextureView(GpuTexture texture, int baseMipLevel, int mipLevels) {
        return new CursedView(this, (CursedTexture) texture, baseMipLevel, mipLevels);
    }

    @Override
    public GpuBuffer createBuffer(@Nullable Supplier<String> label, @GpuBuffer.Usage int usage, long size) {
        CursedBuffer b = new CursedBuffer(this, Native.bufferCreate(this.r, usage, size), usage, size);
        if (this.debug && label != null) {
            this.name(0, b.handle, label.get());
        }
        return b;
    }

    @Override
    public GpuBuffer createBuffer(@Nullable Supplier<String> label, @GpuBuffer.Usage int usage, ByteBuffer data) {
        int length = data.remaining();
        // The first content is copied in, so the buffer is a copy destination even if the game did not say.
        long handle = Native.bufferCreate(this.r, usage | GpuBuffer.USAGE_COPY_DST, length);
        this.encoder.initBuffer(handle, data);
        CursedBuffer b = new CursedBuffer(this, handle, usage | GpuBuffer.USAGE_COPY_DST, length);
        if (this.debug && label != null) {
            this.name(0, b.handle, label.get());
        }
        return b;
    }

    @Override
    public List<String> getLastDebugMessages() {
        return List.of();
    }

    @Override
    public boolean isDebuggingEnabled() {
        return this.debug;
    }

    @Override
    public BackendRenderPipeline.Pending compilePipeline(BackendRenderPipeline.CreateInfo createInfo) {
        long start = System.nanoTime();
        CursedPipeline pipeline = CursedPipeline.compile(this, createInfo);
        PIPELINE_NANOS.add(System.nanoTime() - start);
        PIPELINES.increment();
        return () -> pipeline;
    }

    static final java.util.concurrent.atomic.LongAdder PIPELINE_NANOS = new java.util.concurrent.atomic.LongAdder();
    static final java.util.concurrent.atomic.LongAdder PIPELINES = new java.util.concurrent.atomic.LongAdder();

    /** How many pipelines were made and how long it took (summed over threads). */
    public static String pipelineStats() {
        return PIPELINES.sum() + " pipelines in " + PIPELINE_NANOS.sum() / 1_000_000 + " ms";
    }

    @Override
    public void close() {
        current = null;
        this.logMessages();
        Native.close(this.r);
    }

    @Override
    public GpuQueryPool createTimestampQueryPool(int size) {
        return new CursedQueries(this, size);
    }

    @Override
    public long getTimestampCalibrationOffset() {
        double period = this.info.timestampPeriod();
        long gpu = Native.timestampNow(this.r);
        long now = System.nanoTime();
        return now - (period == 1.0 ? gpu : (long) (gpu * period));
    }

    @Override
    public DeviceInfo getDeviceInfo() {
        return this.info;
    }
}
