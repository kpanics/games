package cursed.recode.menus;

import com.google.gson.JsonObject;
import com.mojang.blaze3d.platform.InputConstants;
import cursed.recode.gui.ClickGui;
import cursed.recode.render.Native;
import java.nio.charset.StandardCharsets;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.function.Consumer;
import net.minecraft.client.Minecraft;
import net.minecraft.client.input.KeyEvent;

/**
 * The game's screens as the client draws them (native/src/menus): the menus' numbers for screens, their data handed
 * over, the keys and buttons in the menus' terms, what they ask for read back.
 */
public final class MenuHost {
    private static final AtomicInteger IDS = new AtomicInteger(1);
    private static String lang = "";

    private MenuHost() {
    }

    public static int nextId() {
        return IDS.getAndIncrement();
    }

    /** Screen `id` opened as the menus' page `kind`, with its data. */
    public static void open(int id, String kind, JsonObject data) {
        syncLanguage();
        Native.menusOpen(id, kind, data.toString().getBytes(StandardCharsets.UTF_8));
    }

    public static void data(int id, JsonObject data) {
        Native.menusData(id, data.toString().getBytes(StandardCharsets.UTF_8));
    }

    public static void closed(int id) {
        Native.menusClose(id);
    }

    /** The game's language changed from the client's menu (its language card). */
    public static void language(String code) {
        SettingsPage.language(code);
    }

    /** The client's own words follow the game's language. */
    public static void syncLanguage() {
        String code = Minecraft.getInstance().options.languageCode;
        if (!code.equals(lang)) {
            lang = code;
            Native.menusLang(code);
        }
    }

    /** Alt Manager's faces of the main menu's card (it draws the menus itself, not through a page). */
    public static void altFaces(net.minecraft.client.gui.GuiGraphicsExtractor graphics) {
        Extras.altFaces(graphics);
    }

    /** A key to the menus: by SDL's name for it, with shift (1), ctrl (2), alt (4). */
    public static void key(KeyEvent event) {
        int mods = (event.hasShiftDown() ? 1 : 0) | (event.hasControlDown() ? 2 : 0) | (event.hasAltDown() ? 4 : 0);
        Native.menusKey(event.isEscape() ? "Escape" : ClickGui.keyName(event.key()), mods);
    }

    /** SDL's buttons (left 1, middle 2, right 3, then 4, 5) as the menus' (left 0, right 1, middle 2, then 3, 4). */
    public static int button(int sdl) {
        return switch (sdl) {
            case InputConstants.MOUSE_BUTTON_LEFT -> 0;
            case InputConstants.MOUSE_BUTTON_RIGHT -> 1;
            case InputConstants.MOUSE_BUTTON_MIDDLE -> 2;
            default -> sdl - 1;
        };
    }

    /**
     * What a page says each frame (up to 15 numbers), then the game's menu background blur (the world frosted that much
     * behind the menus), then the options the menu's valley follows: render distance, clouds, their distance,
     * particles, brightness, field of view, the graphics preset, the blur (0..10).
     */
    public static float[] inp(float[] page) {
        float[] v = new float[24];
        System.arraycopy(page, 0, v, 0, Math.min(15, page.length));
        var o = Minecraft.getInstance().options;
        v[15] = o.menuBackgroundBlurriness().get() / 10.0F;
        v[16] = o.renderDistance().get();
        v[17] = o.cloudStatus().get().ordinal();
        v[18] = o.cloudRange().get();
        v[19] = o.particles().get().ordinal();
        v[20] = o.gamma().get().floatValue();
        v[21] = o.fov().get();
        v[22] = o.graphicsPreset().get().ordinal();
        v[23] = o.menuBackgroundBlurriness().get();
        return v;
    }

    /** What the menus asked for since the last time, each to `act`. */
    public static void poll(Consumer<String> act) {
        byte[] b = Native.menusPoll();
        if (b == null || b.length == 0) {
            return;
        }
        for (String line : new String(b, StandardCharsets.UTF_8).split("\n")) {
            if (!line.isEmpty()) {
                act.accept(line);
            }
        }
    }
}
