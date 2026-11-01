package cursed.recode.menus;

import cursed.recode.render.Native;
import java.util.Map;
import java.util.WeakHashMap;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.screens.ChatScreen;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.gui.screens.inventory.AbstractCommandBlockEditScreen;
import net.minecraft.client.gui.screens.inventory.AbstractContainerScreen;
import net.minecraft.client.gui.screens.inventory.AbstractSignEditScreen;
import net.minecraft.client.gui.screens.inventory.BookEditScreen;
import net.minecraft.client.gui.screens.inventory.BookViewScreen;
import net.minecraft.client.gui.screens.inventory.JigsawBlockEditScreen;
import net.minecraft.client.gui.screens.inventory.StructureBlockEditScreen;
import net.minecraft.client.gui.screens.inventory.TestBlockEditScreen;
import net.minecraft.client.gui.screens.inventory.TestInstanceBlockEditScreen;
import net.minecraft.client.input.CharacterEvent;
import net.minecraft.client.input.KeyEvent;
import net.minecraft.client.input.MouseButtonEvent;
import org.jspecify.annotations.Nullable;

/**
 * Which of the game's screens the client draws (native/src/menus) and the game's input and drawing turned to them.
 * The game's screen stays the game's (its own logic, its widgets made by its init, its tick): only its picture and
 * what the mouse and the keys do to it are the menus' (cursed.recode.mixin.client.Menus*Mixin hook here).
 */
public final class Menus {
    private static final Map<Screen, Host> HOSTS = new WeakHashMap<>();
    /** CURSED_DEV_VANILLASCREENS: the game's own screens, as they are. */
    private static final boolean OFF = cursed.recode.dev.Dev.on("CURSED_DEV_VANILLASCREENS");
    /** The screen shown before each change of screens going on (its page sinks away when no other comes). */
    private static final java.util.ArrayDeque<Host[]> BEFORE = new java.util.ArrayDeque<>();
    /** The screens gone in the last moments (their pages still sinking): until when each draws its extras. */
    private static final Map<Host, Long> LEAVING = new java.util.LinkedHashMap<>();
    /** Frames the menus still draw over the game once their last screen went (its page sinking away). */
    private static long leftoverUntil;

    private Menus() {
    }

    /** The page that draws `s`, or null: the game draws it (inventories, chat, signs, books, the client's own). */
    static @Nullable String kind(Screen s) {
        if (OFF || s instanceof AbstractContainerScreen<?> || s instanceof ChatScreen || s instanceof AbstractSignEditScreen || s instanceof BookEditScreen
                || s instanceof BookViewScreen || s instanceof AbstractCommandBlockEditScreen || s instanceof StructureBlockEditScreen || s instanceof JigsawBlockEditScreen
                || s instanceof TestBlockEditScreen || s instanceof TestInstanceBlockEditScreen) {
            return null;
        }
        // (the license's: the loader's page, as at the start)
        if (s instanceof cursed.recode.LicenseScreen) {
            return "loader";
        }
        String pkg = s.getClass().getPackageName();
        if (pkg.startsWith("cursed.recode")) {
            return null;
        }
        return Kinds.of(s);
    }

    /** The host of `s` (made the first time it is drawn), or null when the game draws it. */
    public static @Nullable Host host(@Nullable Screen s) {
        if (s == null) {
            return null;
        }
        Host h = HOSTS.get(s);
        if (h == null && !HOSTS.containsKey(s)) {
            String kind = kind(s);
            h = kind != null ? new Host(s, kind) : null;
            HOSTS.put(s, h);
        }
        return h;
    }

    // ---- the hooks

    public static void extract(Screen s, GuiGraphicsExtractor graphics, int mouseX, int mouseY, float a) {
        Host h = host(s);
        if (h != null && cursed.recode.render.GuiFrame.active()) {
            h.extract(graphics);
        } else {
            s.extractRenderStateWithTooltipAndSubtitles(graphics, mouseX, mouseY, a);
        }
    }

    public static boolean mouseClicked(Screen s, MouseButtonEvent event, boolean doubleClick) {
        Host h = host(s);
        return h != null ? h.mouse(event, 1, doubleClick) : s.mouseClicked(event, doubleClick);
    }

    public static boolean mouseReleased(Screen s, MouseButtonEvent event) {
        Host h = host(s);
        return h != null ? h.mouse(event, 2, false) : s.mouseReleased(event);
    }

    public static boolean mouseScrolled(Screen s, double x, double y, double scrollX, double scrollY) {
        Host h = host(s);
        if (h != null) {
            Native.menusScroll((float) scrollY);
            return true;
        }
        return s.mouseScrolled(x, y, scrollX, scrollY);
    }

    public static void mouseMoved(Screen s, double x, double y) {
        if (host(s) == null) {
            s.mouseMoved(x, y);
        }
    }

    public static boolean mouseDragged(Screen s, MouseButtonEvent event, double dx, double dy) {
        return host(s) != null || s.mouseDragged(event, dx, dy);
    }

    public static boolean keyPressed(Screen s, KeyEvent event) {
        // an inventory's settings: the colour wheel's hex being typed takes every key
        if (cursed.recode.hud.HudPack.panelKeys(s)) {
            int k = event.key();
            Native.hudKey(event.isEscape(), k == com.mojang.blaze3d.platform.InputConstants.KEY_RETURN || k == com.mojang.blaze3d.platform.InputConstants.KEY_NUMPADENTER,
                    k == com.mojang.blaze3d.platform.InputConstants.KEY_BACKSPACE);
            return true;
        }
        Host h = host(s);
        return h != null ? h.key(event) : s.keyPressed(event);
    }

    public static boolean keyReleased(Screen s, KeyEvent event) {
        Host h = host(s);
        // the game mode switcher switches when F3 is let go: its screen hears that
        return h != null && !h.kind.equals("gamemode") && !h.kind.equals("credits") || s.keyReleased(event);
    }

    public static boolean charTyped(Screen s, CharacterEvent event) {
        if (cursed.recode.hud.HudPack.panelKeys(s)) {
            Native.hudChar(event.codepoint());
            return true;
        }
        Host h = host(s);
        if (h != null) {
            Native.menusChar(event.codepoint());
            return true;
        }
        return s.charTyped(event);
    }

    private static long changingAt;

    /** The game is about to change its screen (Gui.setScreen). */
    public static void changing(@Nullable Screen old) {
        changingAt = System.nanoTime();
        // a change can happen inside another (back to the game: setScreen(null) grabs the mouse, which sets it again):
        // each change keeps its own screen before it
        BEFORE.push(new Host[] {old != null ? HOSTS.get(old) : null});
    }

    /** CURSED_DEV_NOLOOK: the camera stays where it is for the advancements (to measure without it). */
    private static final boolean NO_LOOK = cursed.recode.dev.Dev.on("CURSED_DEV_NOLOOK");
    /** How far the camera has looked up for the advancements (0..1), and when it was asked last. */
    private static float lookUp;
    private static long lookAsked;

    /**
     * How far the camera looks up into the sky now (0..1, eased in and out): the advancements are a look up, it rises
     * as they open and comes down as they go (cursed.recode.mixin.client.MenusCameraMixin).
     */
    public static float lookUp() {
        Minecraft mc = Minecraft.getInstance();
        long now = System.nanoTime();
        float dt = lookAsked == 0 ? 0.0F : Math.min(0.1F, (now - lookAsked) / 1.0E9F);
        lookAsked = now;
        Screen s = mc.gui.screen();
        boolean up = s instanceof net.minecraft.client.gui.screens.advancements.AdvancementsScreen && host(s) != null && !NO_LOOK;
        lookUp = up ? Math.min(1.0F, lookUp + dt / 1.25F) : Math.max(0.0F, lookUp - dt / 0.9F);
        return lookUp * lookUp * (3.0F - 2.0F * lookUp);
    }

    /** It changed: a page no other comes after sinks away over the game. */
    public static void changed(@Nullable Screen now) {
        Host[] top = BEFORE.poll();
        if (cursed.recode.dev.Dev.on("CURSED_DEV_STALLS") && System.nanoTime() - changingAt > 8_000_000L) {
            cursed.recode.CursedRecode.LOGGER.info("[stall] setScreen({}) took {} ms", now == null ? "none" : now.getClass().getSimpleName(), (System.nanoTime() - changingAt) / 1_000_000);
        }
        Host was = top != null ? top[0] : null;
        if (cursed.recode.dev.Dev.on("CURSED_DEV_MENUS")) {
            cursed.recode.CursedRecode.LOGGER.info("[menus dev] screen {} -> {}", was == null ? "-" : was.kind, now == null ? "none" : now.getClass().getSimpleName());
        }
        // a screen shown before (the game went back to it): its page opens again
        Host back = host(now);
        if (back != null && back != was) {
            back.reshow();
        }
        if (was != null && was != host(now)) {
            LEAVING.put(was, System.nanoTime() + 900_000_000L);
        }
        if (was != null && host(now) == null) {
            was.gone();
            Native.menusClose(was.id);
            leftoverUntil = System.nanoTime() + 1_200_000_000L;
        } else if (was != null) {
            was.gone();
        }
    }

    /** The extras (items, faces) of the pages still sinking away, each by its own numbers; not `current`'s. */
    static void leavingExtras(@Nullable Host current, GuiGraphicsExtractor graphics, float[] said) {
        long now = System.nanoTime();
        LEAVING.values().removeIf(until -> until < now);
        for (Host h : LEAVING.keySet()) {
            if (h != current) {
                Extras.page = h.id;
                h.extrasLeaving(graphics, said);
            }
        }
    }

    /** Over the game (the HUD's turn): the last page sinking away once its screen went. */
    public static void leftovers(GuiGraphicsExtractor graphics) {
        if (leftoverUntil == 0 || TabList.active()) {
            return;
        }
        Minecraft mc = Minecraft.getInstance();
        if (System.nanoTime() > leftoverUntil || host(mc.gui.screen()) != null) {
            leftoverUntil = 0;
            return;
        }
        Host.leftover(graphics);
    }
}
