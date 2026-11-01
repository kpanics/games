package cursed.recode.launch;

import cursed.recode.render.Native;
import net.minecraft.client.Minecraft;
import net.minecraft.client.multiplayer.ServerData;
import org.jspecify.annotations.Nullable;

/**
 * The game's window has our own title bar (native/src/chrome): Windows' bar is gone, the bar is the top of the window's
 * client area (drawn over the swap chain's top rows every frame), and the game knows only the picture under it as its
 * window — its size and the mouse are offset by the bar (WindowMixin, MouseHandlerMixin, InputConstantsMixin). After
 * its name the bar shows where the game is: the world's or the server's name.
 *
 * Without it (Windows' own bar, as before): CURSED_DEV_NOCHROME, or -Dcursed.recode.nochrome.
 */
public final class Chrome {
    private static boolean on;
    private static String status = "";

    private Chrome() {
    }

    /** Whether the game's window gets the bar (Windows only). */
    public static boolean wanted() {
        return System.getProperty("os.name", "").startsWith("Windows") && !Boolean.getBoolean("cursed.recode.nochrome") && System.getenv("CURSED_DEV_NOCHROME") == null;
    }

    /** The window `hwnd` gets the bar; its picture at `picture` (x y w h) or where SDL put it. */
    public static void install(long hwnd, int @Nullable [] picture) {
        if (hwnd == 0L || !wanted()) {
            return;
        }
        if (picture != null) {
            Native.chromeInstall(hwnd, picture[0], picture[1], picture[2], picture[3], true);
        } else {
            Native.chromeInstall(hwnd, 0, 0, 0, 0, false);
        }
        on = true;
    }

    /** The bar's height now (px; 0 without it or in fullscreen): what the window's picture is offset by. */
    public static int inset() {
        return on ? Native.chromeInset() : 0;
    }

    public static boolean on() {
        return on;
    }

    /** Where the game is, for the bar: the world's name, the server's, or nothing in the menus. */
    public static void tick(Minecraft mc) {
        if (!on) {
            return;
        }
        String now = "";
        if (mc.level != null) {
            if (mc.getSingleplayerServer() != null) {
                now = mc.getSingleplayerServer().getWorldData().getLevelName();
            } else {
                ServerData server = mc.getCurrentServer();
                now = server == null ? "" : server.isRealm() ? "Realms" : server.name;
            }
        }
        if (!now.equals(status)) {
            status = now;
            Native.chromeStatus(now);
        }
    }
}
