package cursed.recode.launch;

import com.mojang.blaze3d.systems.RenderSystem;
import com.mojang.renderpearl.api.GpuFormat;
import com.mojang.renderpearl.api.textures.FilterMode;
import com.mojang.renderpearl.api.textures.GpuTexture;
import com.mojang.renderpearl.api.textures.GpuTextureView;
import cursed.recode.render.LaunchFrame;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import org.jspecify.annotations.Nullable;

/**
 * The ways into and out of a world («ink», launch/ink.rs ie and ix).
 *
 * Into one: the menu goes, the valley goes into its living outlines (the launch's picture), the world's name over them
 * (the loading page, menus/notice.rs), the outlines' light the world's loading; once the world is there its outlines
 * come over the valley's and the world fills in from them (the launch's picture over the game, its frame the game's own
 * each frame, before its interface).
 *
 * Out of one: the world (live, without its interface) goes into its outlines, which go over into the valley's; the
 * window before the game takes the way out from there on the same picture (it draws the valley, not the world:
 * Twin.farewell) and the ink runs down off the desktop. Behind it the world is left (saved as Save and Quit does) and
 * the game ends. Started straight into a world (no main menu), the game's window takes over from the window before the
 * game once the world draws.
 */
public final class Transition {
    private Transition() {
    }

    /** The way out: when it began (ns; 0 none), when the window before the game took it (0 not yet, -1 none, -2 done). */
    private static long exitAt, farewellAt;
    private static @Nullable GpuTexture shot, out;
    private static @Nullable GpuTextureView shotView, outView;
    private static long last;
    /** The window before the game takes the way out before the pane thaws (launch/glass.rs gx THAW_FROM 3.0); alone it ends dark. */
    private static final float HAND = 1.5F, ALONE = 3.0F;

    public static boolean exiting() {
        return exitAt != 0L;
    }

    /** Into a world: when it began (ns; 0 none), its name, from where (0 a world of one's own, 1 a server), the menus'
     *  clock then (the loading page shows the name from it), how far it has loaded, when it had (its seconds; -1 not yet). */
    private static long enterAt;
    private static String enterName = "";
    private static int enterKind;
    private static float enterSince, enterLoad, loadedAt = -1.0F;
    /** How far the point shows the world loaded (toward what is known, never back, never a leap), when it last moved;
     *  how far it had shown when the world was there. */
    private static float shownLoad, loadedLoad;
    private static long shownAt;
    private static boolean tilesHeard;

    /** A world is being gone into: `name` (a world's, a server's), `kind` 0 one's own, 1 a server. */
    public static void beginEnter(String name, int kind) {
        enterAt = System.nanoTime();
        enterName = name == null ? "" : name;
        enterKind = kind;
        enterSince = LaunchScreen.menusNow();
        enterLoad = 0.0F;
        loadedAt = -1.0F;
        shownLoad = loadedLoad = 0.0F;
        shownAt = 0L;
        tilesHeard = false;
        cursed.recode.render.Native.launchMusic(5);
    }

    public static boolean entering() {
        return enterAt != 0L;
    }

    /** The way in is on and the world is not there yet: the launch's picture is the menus' background even over a level. */
    public static boolean enterBackground() {
        return enterAt != 0L && loadedAt < 0.0F;
    }

    /** Since when a world has been played (ns; 0 none since a level was last gone): a loading screen after that is a
     *  portal's, not a way in. */
    private static long playedSince;

    public static boolean played() {
        return playedSince != 0L && System.nanoTime() - playedSince > 1_000_000_000L;
    }

    private static float enterT() {
        return (System.nanoTime() - enterAt) / 1e9F;
    }

    /**
     * The loading page (NoticePage), each time it says: the world's name as it knows it, how far (0..1, negative
     * none); begins the way in when nothing began it (a world just made). Its part for the page: the name, from where,
     * since when.
     */
    public static com.google.gson.JsonObject enterNotice(String about, float progress, boolean server) {
        if (enterAt == 0L) {
            beginEnter(about, server ? 1 : 0);
        }
        if (enterName.isEmpty() && about != null && !about.isEmpty()) {
            enterName = about;
        }
        if (progress >= 0.0F) {
            enterLoad = Math.max(enterLoad, progress);
        }
        com.google.gson.JsonObject j = new com.google.gson.JsonObject();
        j.addProperty("name", enterName);
        j.addProperty("kind", enterKind);
        j.addProperty("since", enterSince);
        return j;
    }

    /** How far the world has loaded: what the game says, or a guess by the time while it says nothing (connecting). */
    private static float enterLoaded() {
        if (enterLoad > 0.0F) {
            return enterLoad;
        }
        float t = enterT();
        return Math.max(0.0F, Math.min(0.9F, (t - 2.0F) / 6.0F));
    }

    /** How far the point shows the world loaded: it goes toward what is known (the gap closed over ~0.3 s, at most
     *  0.8 of the line a second), never back (a guess the game's word comes under), never a leap. */
    private static float shownLoad() {
        long now = System.nanoTime();
        float dt = shownAt == 0L ? 0.0F : Math.min(0.1F, (now - shownAt) / 1e9F);
        shownAt = now;
        float goal = enterLoaded();
        if (goal > shownLoad) {
            shownLoad = Math.min(goal, shownLoad + Math.min((goal - shownLoad) / 0.3F, 0.8F) * dt);
        }
        return shownLoad;
    }

    /** The menus' valley under the loading page (MenuWorld): the scene into the world. */
    public static void enterScene() {
        if (enterAt != 0L && loadedAt < 0.0F) {
            LaunchFrame.scene(LaunchFrame.ENTER, enterT(), shownLoad(), -1.0F, null);
        }
    }

    /** The window was closed in a world: its frame kept (what is on screen), the way out begins. */
    public static void exitWorld() {
        if (exitAt != 0L) {
            return;
        }
        Minecraft mc = Minecraft.getInstance();
        try {
            GpuTexture src = mc.gameRenderer.mainRenderTarget().getColorTexture();
            if (src != null && LaunchFrame.active()) {
                int w = src.getWidth(0), h = src.getHeight(0);
                shot = RenderSystem.getDevice().createTexture("Cursed way out frame", GpuTexture.USAGE_COPY_DST | GpuTexture.USAGE_TEXTURE_BINDING, GpuFormat.RGBA8_UNORM, w, h, 1, 1);
                shotView = RenderSystem.getDevice().createTextureView(shot);
                RenderSystem.getDevice().createCommandEncoder().copyTextureToTexture(src, shot, 0, 0, 0, 0, 0, w, h);
            }
        } catch (Throwable e) {
            cursed.recode.CursedRecode.LOGGER.warn("[quit] the world's frame could not be kept", e);
        }
        exitAt = System.nanoTime();
        last = exitAt;
        farewellAt = 0L;
        cursed.recode.render.Native.launchMusic(5);
        cursed.recode.CursedRecode.LOGGER.info("[quit] the way out of the world begins");
    }

    /** Over everything, each frame (MenusGuiMixin): the way out's picture, or the way in's once the world is there. */
    public static void draw(GuiGraphicsExtractor graphics) {
        Minecraft mc0 = Minecraft.getInstance();
        if (enterAt != 0L && loadedAt < 0.0F && mc0.level != null && mc0.gui.screen() == null) {
            // the world is there from this frame: no frame of it bare before the picture over it
            loadedLoad = shownLoad;
            loadedAt = enterT();
            last = System.nanoTime();
        }
        if (enterAt != 0L && loadedAt >= 0.0F && exitAt == 0L && LaunchFrame.active()) {
            drawEnter(graphics);
            return;
        }
        if (exitAt == 0L || !LaunchFrame.active()) {
            return;
        }
        Minecraft mc = Minecraft.getInstance();
        int w = mc.getWindow().getWidth(), h = mc.getWindow().getHeight();
        if (w <= 0 || h <= 0) {
            return;
        }
        if (out == null || out.getWidth(0) != w || out.getHeight(0) != h) {
            release(false);
            out = RenderSystem.getDevice().createTexture("Cursed way out", GpuTexture.USAGE_COPY_DST | GpuTexture.USAGE_TEXTURE_BINDING, GpuFormat.RGBA8_UNORM, w, h, 1, 1);
            outView = RenderSystem.getDevice().createTextureView(out);
        }
        long now = System.nanoTime();
        float dt = Math.min(0.1F, (now - last) / 1e9F);
        last = now;
        float e = (now - exitAt) / 1e9F;
        float[] anchor = anchor(w, h);
        LaunchFrame.scene(LaunchFrame.EXIT_WORLD, e, 1.0F, -1.0F, shot);
        LaunchFrame.send(out, LaunchScreen.cardNow(), LaunchScreen.fxNow(), LaunchScreen.lifeNow(), dt, LaunchFrame.LAUNCHED | LaunchFrame.MENUS_FIRE, 1.0F,
                new float[] {0.5F, 0.5F, 0.0F}, new float[] {0.0F, 0.0F}, 0.0F, 0.0F, 1.0F, anchor[0], anchor[1], anchor[2], new float[10], 0.0F, new float[0], 0,
                NO_ICONS, new float[0], 0);
        graphics.nextStratum();
        graphics.blit(outView, RenderSystem.getSamplerCache().getClampToEdge(FilterMode.NEAREST), 0, 0, graphics.guiWidth(), graphics.guiHeight(), 0.0F, 1.0F, 0.0F, 1.0F);
    }

    private static final float[] NO_ICONS = {0, 0, 0, -1, 0, 0, 0, -1, 0, 0, 0, -1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0};

    /** The world there: its frame (the game's, this frame) put together in blocks from the line over the launch's picture. */
    private static void drawEnter(GuiGraphicsExtractor graphics) {
        Minecraft mc = Minecraft.getInstance();
        int w = mc.getWindow().getWidth(), h = mc.getWindow().getHeight();
        if (w <= 0 || h <= 0) {
            return;
        }
        if (out == null || out.getWidth(0) != w || out.getHeight(0) != h) {
            release(false);
            out = RenderSystem.getDevice().createTexture("Cursed way in", GpuTexture.USAGE_COPY_DST | GpuTexture.USAGE_TEXTURE_BINDING, GpuFormat.RGBA8_UNORM, w, h, 1, 1);
            outView = RenderSystem.getDevice().createTextureView(out);
        }
        long now = System.nanoTime();
        float dt = Math.min(0.1F, (now - last) / 1e9F);
        last = now;
        float t = enterT();
        // («ink», launch/ink.rs ie: the world's outlines come from `le`, it fills in from them from le + 0.5)
        float le = Math.max(loadedAt, 1.6F);
        if (!tilesHeard && t >= le + 0.5F) {
            tilesHeard = true;
            cursed.recode.render.Native.launchMusic(6);
        }
        float[] anchor = anchor(w, h);
        // (the world's name was the loading page's: it goes with the page)
        NAME.clear();
        // (its frame is kept as the game draws its interface, beforeGui: none yet the first frame)
        LaunchFrame.scene(LaunchFrame.ENTER, t, loadedLoad, loadedAt, shot);
        LaunchFrame.send(out, LaunchScreen.cardNow(), LaunchScreen.fxNow(), LaunchScreen.lifeNow(), dt, LaunchFrame.LAUNCHED | LaunchFrame.MENUS_FIRE, 1.0F,
                new float[] {0.5F, 0.5F, 0.0F}, new float[] {0.0F, 0.0F}, 0.0F, 0.0F, 1.0F, anchor[0], anchor[1], anchor[2], new float[10], 0.0F, new float[0], 0,
                NO_ICONS, NAME.data, NAME.count);
        graphics.nextStratum();
        graphics.blit(outView, RenderSystem.getSamplerCache().getClampToEdge(FilterMode.NEAREST), 0, 0, graphics.guiWidth(), graphics.guiHeight(), 0.0F, 1.0F, 0.0F, 1.0F);
    }

    private static final LaunchText.Sink NAME = new LaunchText.Sink();

    private static float smooth(float a, float b, float x) {
        float k = Math.max(0.0F, Math.min(1.0F, (x - a) / (b - a)));
        return k * k * (3.0F - 2.0F * k);
    }

    /**
     * The world's name over the horizon's line, `wp` risen (as the loading page has it, menus/notice.rs draw_enter):
     * its name, and under it a world of one's own or a server, each rising out of its line.
     */
    private static void name(LaunchText.Sink out, int w, int h, float ui, float wp) {
        float tanH = Math.max(0.5095F, 0.42F * h / w);
        // the tip: where the horizon is for the menus' camera at rest (launch/timeline.rs layout)
        float tipX = w * 0.5F, tipY = h * (0.5F - 0.5F * 0.08923F / tanH);
        float vh = Math.min(h, w * 0.62F);
        String n1 = enterName.toUpperCase(java.util.Locale.ROOT);
        String n2 = net.minecraft.client.resources.language.I18n.get(enterKind == 1 ? "menu.multiplayer" : "menu.singleplayer").toUpperCase(java.util.Locale.ROOT);
        float s1 = Math.max(26.0F * ui, Math.min(62.0F * ui, h * 0.054F));
        float s2 = Math.max(8.0F * ui, Math.min(12.0F * ui, h * 0.011F));
        float bottom = tipY - Math.max(18.0F * ui, 0.04F * vh);
        float top2 = bottom - s2 * 1.3F;
        float top1 = top2 - h * 0.011F - s1 * 1.06F;
        float q1 = Math.max(0.0F, Math.min(1.0F, easeOut(wp * 1.15F))), q2 = Math.max(0.0F, Math.min(1.0F, easeOut(wp * 1.15F - 0.15F)));
        rising(out, LaunchText.Face.MAIN, n1, tipX - LaunchText.width(LaunchText.Face.MAIN, n1, s1, 0.08F) / 2.0F, top1, s1, 1.06F, 0.08F, 0.902F, q1);
        rising(out, LaunchText.Face.MONO, n2, tipX - LaunchText.width(LaunchText.Face.MONO, n2, s2, 0.3F) / 2.0F, top2, s2, 1.3F, 0.3F, 0.54F, q2);
    }

    private static float easeOut(float x) {
        float k = Math.max(0.0F, Math.min(1.0F, x));
        return 1.0F - (1.0F - k) * (1.0F - k) * (1.0F - k);
    }

    /** Text risen `q` out of its line (menus/ui.rs rising_text). */
    private static void rising(LaunchText.Sink out, LaunchText.Face face, String s, float x, float top, float size, float line, float ls, float bright, float q) {
        float baseline = LaunchText.baseline(face, top, size, line);
        float lh = Math.max(line * size, baseline + 1.1F * LaunchText.descent(face, size) - top);
        float base = baseline + (1.0F - q) * 1.08F * lh;
        LaunchText.emit(out, face, s, x, base, size, ls, bright, 1.0F, 0.0F, -1e6F, top - 0.1F * size, 1e6F, top + lh);
    }

    /** Where the menu would put its caption (launch/timeline.rs menu_anchor): ui px, its left, the build line's top. */
    static float[] anchor(int w, int h) {
        float ui = Math.max(1.0F, h / 1300.0F);
        boolean narrow = w < 760.0F * ui;
        float vh = h / 100.0F;
        float left = narrow ? 20.0F * ui : w * 0.062F;
        float small = Math.max(8.0F * ui, Math.min(12.0F * ui, h * 1.05F / 100.0F));
        return new float[] {ui, left, h - 5.0F * vh - small * 1.3F - 0.4F * vh};
    }

    /**
     * Each frame as the game draws its interface (GuiRendererMixin; the level is drawn by then, nothing over it yet):
     * while the world is put together, its frame kept (taken later, the picture over it would be in it as well).
     */
    public static void beforeGui() {
        // the world's frame as it is now, before the game's interface over it: into a world once it is there, and out of
        // one (its outlines are of the living world, not of its last frame with the HUD)
        boolean way = enterAt != 0L && loadedAt >= 0.0F || exitAt != 0L;
        if (!way || !LaunchFrame.active()) {
            return;
        }
        Minecraft mc = Minecraft.getInstance();
        if (mc.level == null) {
            return;
        }
        GpuTexture src = mc.gameRenderer.mainRenderTarget().getColorTexture();
        if (src == null) {
            return;
        }
        int sw = src.getWidth(0), sh = src.getHeight(0);
        if (shot == null || shot.getWidth(0) != sw || shot.getHeight(0) != sh) {
            release(true);
            shot = RenderSystem.getDevice().createTexture("Cursed way in frame", GpuTexture.USAGE_COPY_DST | GpuTexture.USAGE_TEXTURE_BINDING, GpuFormat.RGBA8_UNORM, sw, sh, 1, 1);
            shotView = RenderSystem.getDevice().createTextureView(shot);
        }
        RenderSystem.getDevice().createCommandEncoder().copyTextureToTexture(src, shot, 0, 0, 0, 0, 0, sw, sh);
    }

    /** Frames the world has drawn under the window before the game (started straight into it: no main menu). */
    private static int straightFrames;

    /** Each frame (GuiRendererMixin): the way in's end; the window before the game taking the way out, the world left. */
    public static void frame() {
        Minecraft mc = Minecraft.getInstance();
        if (Twin.active() && mc.level != null && !(mc.gui.screen() instanceof LaunchScreen)) {
            // started straight into a world (a launcher's quick play, a server to join): no main menu to take over from
            // the window before the game, which would wait for it for ever — the game's window is shown once the world
            // draws, and the other goes over it
            Twin.ready();
            straightFrames++;
            long hwnd = hwnd();
            if (straightFrames == 8) {
                org.lwjgl.sdl.SDLVideo.SDL_ShowWindow(mc.getWindow().handle());
                if (hwnd != 0L) {
                    cursed.recode.render.Native.windowFront(hwnd);
                }
            } else if (straightFrames == 12) {
                Twin.release();
                if (hwnd != 0L) {
                    cursed.recode.render.Native.windowLook(hwnd, true);
                }
            }
        }
        if (mc.level == null) {
            playedSince = 0L;
        } else if (mc.gui.screen() == null && playedSince == 0L) {
            playedSince = System.nanoTime();
        }
        if (enterAt != 0L) {
            float t = enterT();
            var screen = mc.gui.screen();
            if (loadedAt < 0.0F && mc.level != null && screen == null) {
                // the world is there: its frame is put together from now
                loadedLoad = shownLoad;
                loadedAt = t;
                last = System.nanoTime();
            } else if (loadedAt < 0.0F && mc.level == null && t > 1.0F && (screen == null || !cursed.recode.menus.NoticePage.loading(screen))) {
                // not gone into after all (cancelled, an error): the valley as it was
                enterAt = 0L;
            } else if (loadedAt >= 0.0F && t >= Math.max(loadedAt, 1.6F) + 2.2F) {
                enterAt = 0L;
                release(true);
            }
        }
        if (exitAt == 0L) {
            return;
        }
        long now = System.nanoTime();
        float e = (now - exitAt) / 1e9F;
        if (farewellAt == 0L && e >= HAND) {
            long hwnd = hwnd();
            int w = mc.getWindow().getWidth(), h = mc.getWindow().getHeight();
            float[] a = anchor(w, h);
            float[] menu = {LaunchScreen.cardNow(), LaunchScreen.fxNow(), a[1], a[2], e, 1.0F};
            float[] view = {0, 0, 0.5F, 0.5F, 0, 0, 0};
            farewellAt = hwnd != 0L && Twin.farewell(hwnd, menu, view) ? now : -1L;
        } else if (farewellAt > 0L && Twin.handedOver(farewellAt, now) || farewellAt == -1L && e >= ALONE) {
            // over this window (or dark by itself): it hides, the world is left (saved), the game ends
            farewellAt = -2L;
            org.lwjgl.sdl.SDLVideo.SDL_HideWindow(mc.getWindow().handle());
            mc.schedule(() -> {
                if (mc.level != null) {
                    mc.disconnectFromWorld(net.minecraft.client.multiplayer.ClientLevel.DEFAULT_QUIT_MESSAGE);
                }
                mc.stop();
            });
        }
    }

    static long hwnd() {
        long handle = Minecraft.getInstance().getWindow().handle();
        return org.lwjgl.sdl.SDLProperties.SDL_GetPointerProperty(org.lwjgl.sdl.SDLVideo.SDL_GetWindowProperties(handle),
                org.lwjgl.sdl.SDLVideo.SDL_PROP_WINDOW_WIN32_HWND_POINTER, 0L);
    }

    private static void release(boolean all) {
        if (outView != null) {
            outView.close();
            outView = null;
        }
        if (out != null) {
            out.close();
            out = null;
        }
        if (all) {
            if (shotView != null) {
                shotView.close();
                shotView = null;
            }
            if (shot != null) {
                shot.close();
                shot = null;
            }
        }
    }
}
