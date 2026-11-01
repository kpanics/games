package cursed.recode.render;

import com.mojang.renderpearl.api.device.GpuSurface;
import com.mojang.renderpearl.api.device.SurfaceException;
import com.mojang.renderpearl.api.textures.GpuTextureView;
import com.mojang.renderpearl.backend.api.CommandEncoderBackend;
import com.mojang.renderpearl.backend.api.GpuSurfaceBackend;
import java.util.Collection;
import java.util.Collections;
import java.util.EnumSet;
import java.util.Set;

/** The window's swapchain (vk/surface.rs Surface). */
final class CursedSurface implements GpuSurfaceBackend {
    private final CursedDevice device;
    private final long handle;
    private final Set<GpuSurface.PresentMode> modes;
    /** This frame is not shown: the game's window is hidden (under the window before the game, cursed.recode.launch.Twin). */
    private boolean unseen;

    CursedSurface(CursedDevice device, long handle) {
        this.device = device;
        this.handle = handle;
        int bits = Native.surfaceModes(handle);
        Set<GpuSurface.PresentMode> set = EnumSet.noneOf(GpuSurface.PresentMode.class);
        for (GpuSurface.PresentMode mode : GpuSurface.PresentMode.values()) {
            if ((bits & (1 << mode.ordinal())) != 0) {
                set.add(mode);
            }
        }
        this.modes = Collections.unmodifiableSet(set);
    }

    @Override
    public void configure(GpuSurface.Configuration config) throws SurfaceException {
        // The game's present mode, always: the menus' pace is the frame thread waiting for the monitor's refresh
        // (LaunchScreen.checkPace, native vk/pace.rs), not FIFO — a swap chain made again in another mode came back
        // slow for the whole session (NVIDIA: every frame's GPU work half as long again, 870 FPS against 1290).
        GpuSurface.PresentMode mode = config.presentMode();
        if (Native.surfaceConfigure(this.device.r, this.handle, config.width(), config.height(), mode.ordinal()) != 0) {
            throw new SurfaceException(Native.failure());
        }
    }

    @Override
    public boolean isSuboptimal() {
        return Native.surfaceSuboptimal(this.handle);
    }

    /**
     * The frame thread acquires the image in its turn; if the swapchain went out of date there (or at present), the
     * game hears it here, at the next acquire, and reconfigures.
     */
    @Override
    public void acquireNextTexture() throws SurfaceException {
        if (Native.surfaceFailed(this.handle)) {
            throw new SurfaceException("Failed to acquire image, swapchain out of date");
        }
        // A hidden window's frames are drawn (the menu under the window before the game) but not shown: there is nothing
        // to show them on, and with the game's vsync on Windows let a hidden window present only every 60 ms or so —
        // its frames never came evenly enough to take over (a first start, found 15.10 in a release's trial).
        this.unseen = hidden();
        if (this.unseen) {
            return;
        }
        this.device.encoder.acquire(this.handle);
    }

    @Override
    public void blitFromTexture(CommandEncoderBackend commandEncoder, GpuTextureView textureView) {
        if (!this.unseen) {
            this.device.encoder.blit(this.handle, CursedView.handle(textureView));
        }
    }

    @Override
    public void present() {
        if (!this.unseen) {
            this.device.encoder.present(this.handle);
        }
    }

    /** Whether the game's window is hidden now. */
    private static boolean hidden() {
        net.minecraft.client.Minecraft mc = net.minecraft.client.Minecraft.getInstance();
        if (mc == null) {
            return false;
        }
        return (org.lwjgl.sdl.SDLVideo.SDL_GetWindowFlags(mc.getWindow().handle()) & org.lwjgl.sdl.SDLVideo.SDL_WINDOW_HIDDEN) != 0;
    }

    @Override
    public Collection<GpuSurface.PresentMode> supportedPresentModes() {
        return this.modes;
    }

    @Override
    public void close() {
        Native.surfaceClose(this.device.r, this.handle);
    }
}
