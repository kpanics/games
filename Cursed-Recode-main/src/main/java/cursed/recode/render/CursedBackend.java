package cursed.recode.render;

import com.mojang.renderpearl.api.device.BackendCreationException;
import com.mojang.renderpearl.api.device.GpuBackend;
import com.mojang.renderpearl.api.device.GpuDebugOptions;
import com.mojang.renderpearl.api.device.GpuDevice;
import com.mojang.renderpearl.frontend.FrontendGpuDevice;
import java.util.Arrays;
import java.util.List;
import org.jspecify.annotations.Nullable;
import org.lwjgl.sdl.SDLHints;
import org.lwjgl.sdl.SDLVideo;

/**
 * The only graphics backend of Cursed Recode: Vulkan, done by cursed_recode.dll. The game asks for it in place of
 * Mojang's OpenGL and Vulkan backends (mixin PreferredGraphicsApiMixin) and drives it through its own frontend, which
 * checks every call as it does for Mojang's backends.
 */
public final class CursedBackend implements GpuBackend {
    @Override
    public String getName() {
        // The game puts this name in its log, F3 and crash reports; it is a Vulkan backend.
        return "Vulkan";
    }

    @Override
    public void loadLibrary() throws BackendCreationException {
        Native.load();
    }

    @Override
    public void unloadLibrary() {
    }

    @Override
    public long createWindow(@Nullable String title, int width, int height, long flags) {
        // one program on the taskbar with the window before the game, her icon, our name
        Native.appId();
        long window = this.makeWindow("Cursed Recode", width, height, flags);
        if (window != 0L) {
            long hwnd = org.lwjgl.sdl.SDLProperties.SDL_GetPointerProperty(SDLVideo.SDL_GetWindowProperties(window), SDLVideo.SDL_PROP_WINDOW_WIN32_HWND_POINTER, 0L);
            if (hwnd != 0L) {
                // under the window before the game it appears at once, no animation (LaunchScreen turns it back on)
                Native.windowLook(hwnd, !cursed.recode.launch.Twin.active());
                // our own title bar over its picture, the picture where the window before the game burned its place
                int[] twin = cursed.recode.launch.Twin.active() ? cursed.recode.launch.Twin.place() : null;
                cursed.recode.launch.Chrome.install(hwnd, twin == null ? null : new int[] {twin[0], twin[1], twin[2], twin[3]});
            }
        }
        return window;
    }

    private long makeWindow(String title, int width, int height, long flags) {
        String at = System.getenv("CURSED_DEV_WINDOW_AT");
        int[] twin = cursed.recode.launch.Twin.active() ? cursed.recode.launch.Twin.place() : null;
        if (twin != null) {
            // The window before the game burns this window's place open: the game's window waits there hidden until it
            // draws the menu (cursed.recode.launch.LaunchScreen shows it under the other one).
            if (at != null) {
                SDLHints.SDL_SetHint(SDLHints.SDL_HINT_WINDOW_ACTIVATE_WHEN_SHOWN, "0");
                SDLHints.SDL_SetHint(SDLHints.SDL_HINT_WINDOW_ACTIVATE_WHEN_RAISED, "0");
                SDLHints.SDL_SetHint(SDLHints.SDL_HINT_MOUSE_FOCUS_CLICKTHROUGH, "1");
            }
            long window = SDLVideo.SDL_CreateWindow(title, twin[2], twin[3], SDLVideo.SDL_WINDOW_VULKAN | SDLVideo.SDL_WINDOW_HIDDEN | flags);
            if (window != 0L) {
                SDLVideo.SDL_SetWindowPosition(window, twin[0], twin[1]);
            }
            return window;
        }
        if (at == null) {
            return SDLVideo.SDL_CreateWindow(title, width, height, SDLVideo.SDL_WINDOW_VULKAN | flags);
        }
        // Development only: the test window opens hidden, moves to `x,y` (another monitor) and is shown without taking
        // the focus from whoever is at the PC.
        SDLHints.SDL_SetHint(SDLHints.SDL_HINT_WINDOW_ACTIVATE_WHEN_SHOWN, "0");
        SDLHints.SDL_SetHint(SDLHints.SDL_HINT_WINDOW_ACTIVATE_WHEN_RAISED, "0");
        // A click sent to the inactive test window reaches the game instead of only focusing the window.
        SDLHints.SDL_SetHint(SDLHints.SDL_HINT_MOUSE_FOCUS_CLICKTHROUGH, "1");
        long window = SDLVideo.SDL_CreateWindow(title, width, height, SDLVideo.SDL_WINDOW_VULKAN | SDLVideo.SDL_WINDOW_HIDDEN | flags);
        if (window != 0L) {
            String[] xy = at.split(",");
            SDLVideo.SDL_SetWindowPosition(window, Integer.parseInt(xy[0].trim()), Integer.parseInt(xy[1].trim()));
            SDLVideo.SDL_ShowWindow(window);
        }
        return window;
    }

    @Override
    public GpuDevice createDevice(GpuDebugOptions options) throws BackendCreationException {
        boolean validation = options.useValidationLayers() || Boolean.getBoolean("cursed.recode.validation");
        long r = Native.open(options.logLevel(), options.useLabels(), validation);
        if (r == 0L) {
            String missing = Native.failureMissing();
            List<String> list = missing.isEmpty() ? List.of() : Arrays.asList(missing.split("\n"));
            throw new BackendCreationException(Native.failure(), Native.reason(Native.failureReason()), list);
        }
        return new FrontendGpuDevice(new CursedDevice(r));
    }
}
