package cursed.recode.render;

import com.mojang.renderpearl.api.textures.GpuTexture;
import org.jspecify.annotations.Nullable;

/**
 * Hands the client's menu (gui/render.rs) its frame in its turn: laid out while the screen is extracted (before the
 * world is drawn in 26.3), drawn just before the game's GUI (GameRendererMixin), so its glass blurs this frame's world.
 */
public final class GuiFrame {
    private static @Nullable GpuTexture out;
    private static int pending;
    private static boolean trim;
    private static @Nullable GpuTexture hudOut;
    private static int hudPending;
    private static @Nullable GpuTexture menusOut;
    private static int menusPending;
    /** Frames of the client's own screens (Custom Inventory's picture under its items and over them). */
    private static final @Nullable GpuTexture[] extraOut = new GpuTexture[2];
    private static final int[] extraPending = new int[2];

    private GuiFrame() {
    }

    /** Whether our renderer draws (the menu needs it). */
    public static boolean active() {
        return CursedDevice.current != null;
    }

    /** The menu's frame `id` (Native.guiFrame) goes into `texture` this frame. */
    public static void send(GpuTexture texture, int id) {
        out = texture;
        pending = id;
    }

    /** The HUD's frame `id` (Native.hudFrame) goes into `texture` this frame (before the menu's). */
    public static void sendHud(GpuTexture texture, int id) {
        hudOut = texture;
        hudPending = id;
    }

    /** The menus' frame `id` (Native.menusFrame) goes into `texture` this frame (after the menu's). */
    public static void sendMenus(GpuTexture texture, int id) {
        menusOut = texture;
        menusPending = id;
    }

    /** A frame of the client's own screen `id` into `texture` this frame (after the menus'), in its place `slot`. */
    public static void sendExtra(int slot, GpuTexture texture, int id) {
        extraOut[slot] = texture;
        extraPending[slot] = id;
    }

    /** The menu closed: its pictures may go. */
    public static void closed() {
        out = null;
        pending = 0;
        trim = true;
    }

    /** Just before the game draws its GUI: the world is in `picture`. */
    public static void flush(@Nullable GpuTexture picture) {
        CursedDevice d = CursedDevice.current;
        if (d != null && hudPending != 0 && hudOut != null && !hudOut.isClosed()) {
            d.encoder.gui(CursedTexture.handle(hudOut), picture != null && !picture.isClosed() ? CursedTexture.handle(picture) : 0L, hudPending);
        }
        hudPending = 0;
        if (d != null && pending != 0 && out != null && !out.isClosed()) {
            d.encoder.gui(CursedTexture.handle(out), picture != null && !picture.isClosed() ? CursedTexture.handle(picture) : 0L, pending);
        } else if (d != null && trim) {
            d.encoder.gui(0L, 0L, 0);
        }
        pending = 0;
        trim = false;
        if (d != null && menusPending != 0 && menusOut != null && !menusOut.isClosed()) {
            d.encoder.gui(CursedTexture.handle(menusOut), picture != null && !picture.isClosed() ? CursedTexture.handle(picture) : 0L, menusPending);
        }
        menusPending = 0;
        for (int i = 0; i < extraOut.length; i++) {
            GpuTexture t = extraOut[i];
            if (d != null && extraPending[i] != 0 && t != null && !t.isClosed()) {
                d.encoder.gui(CursedTexture.handle(t), picture != null && !picture.isClosed() ? CursedTexture.handle(picture) : 0L, extraPending[i]);
            }
            extraPending[i] = 0;
        }
    }
}
