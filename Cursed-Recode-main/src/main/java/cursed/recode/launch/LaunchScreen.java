package cursed.recode.launch;

import com.mojang.blaze3d.systems.RenderSystem;
import com.mojang.renderpearl.api.GpuFormat;
import com.mojang.renderpearl.api.textures.FilterMode;
import com.mojang.renderpearl.api.textures.GpuTexture;
import com.mojang.renderpearl.api.textures.GpuTextureView;
import cursed.recode.launch.LaunchText.Face;
import cursed.recode.render.LaunchFrame;
import java.util.ArrayList;
import java.util.List;
import java.util.Locale;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.components.PlayerFaceExtractor;
import net.minecraft.client.gui.screens.ConnectScreen;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.gui.screens.multiplayer.JoinMultiplayerScreen;
import net.minecraft.client.gui.screens.options.OptionsScreen;
import net.minecraft.client.gui.screens.worldselection.SelectWorldScreen;
import net.minecraft.client.input.KeyEvent;
import net.minecraft.client.input.MouseButtonEvent;
import net.minecraft.client.multiplayer.ServerData;
import net.minecraft.client.multiplayer.resolver.ServerAddress;
import net.minecraft.client.resources.DefaultPlayerSkin;
import net.minecraft.network.chat.Component;
import net.minecraft.util.ARGB;
import org.jspecify.annotations.Nullable;

/**
 * The main menu, as the approved preview has it (Desktop\Cursed Recode Launch): a valley at dusk where the logo has
 * set, the menu on the left rising from lines of light, the account in the top right, the build line and what's new
 * at the bottom. Under the hovered word a line catches fire; Quit is held. The picture is the launch's (launch/mod.rs),
 * the menu over it the menus' (native/src/menus/main_menu.rs: layout, motion, input); this screen keeps the clocks,
 * the window before the game, the way out, and does what the menu asks.
 *
 * The first time in a session the card plays («ink», launch/ink.rs) and the menu rises after it; nothing skips it (her
 * word, 15.10: it plays to its end).
 */
public final class LaunchScreen extends Screen {
    // the card's schedule (launch/timeline.rs tl): where it starts without the window before the game (that window
    // already open), when the game's window may take over, when the menu takes the pointer, when it has settled
    private static final float START = 2.6F, HAND_FROM = 2.65F, LIVE = 12.65F, SETTLE = 13.45F;
    /** The way out of the menu (timeline.rs XM): the window before the game takes over before the window closes. */
    private static final float EXIT_HAND = 1.5F, EXIT_ALONE = 3.0F;
    /** Development: the launch's frame gaps over 40 ms in the log. */
    private static final boolean TIMES = net.fabricmc.loader.api.FabricLoader.getInstance().isDevelopmentEnvironment();
    /** CURSED_DEV_MOUSE=x,y (0..1 of the window): the cursor stays there (hover tests without input). */
    private static final float @Nullable [] DEV_MOUSE = cursed.recode.dev.Dev.on("CURSED_DEV_MOUSE")
            ? new float[] {Float.parseFloat(System.getenv("CURSED_DEV_MOUSE").split(",")[0]), Float.parseFloat(System.getenv("CURSED_DEV_MOUSE").split(",")[1])} : null;
    /** CURSED_DEV_QUIT=seconds: the way out starts by itself that long after the menu settled (tests without input). */
    private static final float AUTO_QUIT = cursed.recode.dev.Dev.on("CURSED_DEV_QUIT") ? Float.parseFloat(System.getenv("CURSED_DEV_QUIT")) : 0.0F;
    /** The card's clock, kept across openings: the world drifts on, the menu rises once a session. */
    private static long cardT0;
    private static boolean started;
    /** How long the card waited for the game in the window before the game: the forge's clock runs that far ahead. */
    private static float fxShift;
    /** Frames shown after the window came up under the window before the game, before that one goes. */
    private static final int SHOW_AT = 3, DRAWN = 4;

    private @Nullable GpuTexture texture;
    private @Nullable GpuTextureView view;
    /** The menus' picture over the launch's. */
    private @Nullable GpuTexture menusTexture;
    private @Nullable GpuTextureView menusView;
    private long last = System.nanoTime(), realLast = this.last;
    private static final long LIFE0 = System.nanoTime();

    private float curX = 0.62F, curY = 0.55F, curSX = 0.62F, curSY = 0.55F, curOn, lookX, lookY;
    private double flashAt = -99, nextFlash;
    private long volumeAt;
    /** What the menus said back this frame (menus/mod.rs out). */
    private final float[] said = new float[64];
    private float clicksSeen;
    /** The menus' number for this screen. */
    private final int menusId = cursed.recode.menus.MenuHost.nextId();
    /** Frames drawn since the game could hand over to it (the window before the game, Twin). */
    private int handFrames, steady;
    /** Frames in a row drawn evenly before this window is shown. */
    private static final int STEADY = 10;
    private static boolean collected;
    /** Development: the menu's frame times, counted over a few seconds. */
    private int paceFrames, paceSlow;
    private long paceFrom, paceWorst;
    /** The way out: since when (0 none), and since when the window before the game took it over. */
    private long exitAt, farewellAt;

    public LaunchScreen() {
        super(Component.literal("Cursed Recode"));
        if (!binEmptied) {
            // the worlds deleted last session (kept a while for Undo) go now
            binEmptied = true;
            cursed.recode.menus.WorldsPage.emptyBin();
        }
    }

    private static boolean binEmptied;

    /** Continue: the server joined last, or the server list. */
    private void proceed() {
        LastServer.Entry last = LastServer.get();
        if (last == null) {
            this.minecraft.gui.setScreen(new JoinMultiplayerScreen(this));
            return;
        }
        ConnectScreen.startConnecting(this, this.minecraft, ServerAddress.parseString(last.address()), new ServerData(last.name(), last.address(), ServerData.Type.OTHER), false, null);
    }

    private static float card() {
        return (frameNow() - cardT0) / 1e9F;
    }

    /** The clock that never waits (the fire's, the water's) ahead of the menus' own by this much (the window before the game's). */
    private static float lifeShift;

    /**
     * The menus' clock (s), for a screen drawn by the menus out of this one (cursed.recode.menus.Host): it steps evenly
     * with the screen (paceNow), once a frame — whoever draws the menus calls it.
     */
    public static float menusClock() {
        return (paceNow(System.nanoTime()) - LIFE0) / 1e9F;
    }

    /** The same, as it is this frame (not stepped). */
    public static float menusNow() {
        return (frameNow() - LIFE0) / 1e9F;
    }

    /** The card's second, the forge's, the clock that never waits: the menus' valley goes on from the main menu's. */
    public static float cardNow() {
        return card();
    }

    public static float fxNow() {
        return card() + fxShift;
    }

    public static float lifeNow() {
        return (frameNow() - LIFE0) / 1e9F + lifeShift;
    }

    /**
     * The menu's clock: one refresh of the screen a frame. The game starts its frames unevenly (on a window the
     * compositor draws, in pairs: one at once, the next a refresh and a half later) while the screen shows them one a
     * refresh, evenly; what moves steps by the screen's refresh, not by when its frame happened to start. The refresh
     * is the frames' own average; a pause, or a clock gone too far from real time, starts it again from now.
     */
    private static long paceNow(long real) {
        long gap = real - paceReal;
        paceReal = real;
        if (paceClock == 0 || gap <= 0 || gap > 100_000_000L) {
            paceClock = real;
            // the gaps go with their sum: kept, each next one would take an old one out of a sum that no longer
            // has it, and the refresh would come out near nothing for good (after the first world: every frame
            // stepped by several)
            java.util.Arrays.fill(PACE_GAPS, 0L);
            paceAt = 0;
            paceSum = 0;
            paceCount = 0;
            return real;
        }
        paceSum += gap - PACE_GAPS[paceAt];
        PACE_GAPS[paceAt] = gap;
        paceAt = (paceAt + 1) % PACE_GAPS.length;
        paceCount = Math.min(paceCount + 1, PACE_GAPS.length);
        long period = Math.max(2_000_000L, paceSum / paceCount);
        long steps = Math.max(1L, Math.round((double) (real - paceClock) / period));
        paceClock += steps * period;
        paceSkips += steps > 1 ? 1 : 0;
        if (Math.abs(real - paceClock) > 3 * period) {
            paceClock = real;
        }
        return paceClock;
    }

    private static long frameNow() {
        return paceClock != 0 ? paceClock : System.nanoTime();
    }

    private static long paceClock, paceReal, paceSum;
    private static int paceAt, paceCount, paceSkips;
    private static final long[] PACE_GAPS = new long[24];

    private static float clamp01(float x) {
        return Math.max(0.0F, Math.min(1.0F, x));
    }

    private static float smooth(float a, float b, float x) {
        float k = clamp01((x - a) / (b - a));
        return k * k * (3.0F - 2.0F * k);
    }

    private static float easeOut(float a, float b, float x) {
        float k = clamp01((x - a) / (b - a));
        return 1.0F - (1.0F - k) * (1.0F - k) * (1.0F - k);
    }

    private static float toward(float v, float target, float rate, float dt) {
        return v + (target - v) * Math.min(1.0F, dt * rate);
    }

    @Override
    public boolean shouldCloseOnEsc() {
        return false;
    }

    /** The window was closed elsewhere than in the main menu: the main menu shown next goes out at once. */
    private static boolean exitOnShow;
    /** When the window was asked to close (ns): asked again a while later, the game closes as it would. */
    private static long quitAskedAt;

    /**
     * The window's close button or Alt+F4 (WindowMixin): the menu's own way out, as Quit held. Elsewhere than the main
     * menu (a world, a screen over it) the world is left first, saved as Save and Quit does, and the main menu that
     * comes goes out at once: the same farewell from anywhere. True when it was taken (the game does not close by
     * itself then).
     */
    public static boolean quitRequested() {
        net.minecraft.client.Minecraft mc = net.minecraft.client.Minecraft.getInstance();
        if (mc == null) {
            return false;
        }
        if (mc.level == null && mc.gui.screen() instanceof LaunchScreen s) {
            s.beginExit();
            return true;
        }
        long now = System.nanoTime();
        // asked again after it should have gone: the game closes as it would
        if (quitAskedAt != 0 && now - quitAskedAt > 4_000_000_000L) {
            cursed.recode.CursedRecode.LOGGER.info("[quit] asked again, closing at once");
            return false;
        }
        if (quitAskedAt != 0) {
            return true;
        }
        quitAskedAt = now;
        exitOnShow = true;
        boolean inWorld = mc.level != null;
        // the world is left first (saved as by Save and Quit); the main menu that comes goes out the way it does
        // (the farewell when there is the window before the game, else the menu sinking and dimming)
        cursed.recode.CursedRecode.LOGGER.info("[quit] the window closed in {}: leaving the world, then the way out (farewell {})",
                mc.gui.screen() == null ? "the game" : mc.gui.screen().getClass().getSimpleName(), Twin.place() != null ? "ready" : "without the window before the game");
        if (inWorld) {
            // v12: the world's frame goes grey and crumbles into the line, the window closes onto it (Transition)
            Transition.exitWorld();
            return true;
        } else {
            mc.schedule(LaunchScreen::leave);
        }
        return true;
    }

    /** When the world began to go dark for the way out (ns; 0 none). */
    private static long veilAt;
    private static boolean leaving;
    /** How long the world takes to go dark before it is left. */
    private static final float VEIL = 0.45F;

    private static void leave() {
        net.minecraft.client.Minecraft mc = net.minecraft.client.Minecraft.getInstance();
        if (mc.level != null) {
            mc.disconnectFromWorld(net.minecraft.client.multiplayer.ClientLevel.DEFAULT_QUIT_MESSAGE);
        }
        if (!(mc.gui.screen() instanceof LaunchScreen)) {
            mc.gui.setScreen(new LaunchScreen());
        }
    }

    /**
     * The world's darkness over everything (0..1) while the window closes in a world: the world and its screens go
     * dark, and then it is left — into the same dark the saving and the main menu's way out come up from (before, the
     * world was cut to black at once).
     */
    public static float quitVeil() {
        if (veilAt == 0L) {
            return 0.0F;
        }
        return smooth(0.0F, VEIL, (System.nanoTime() - veilAt) / 1e9F);
    }

    /** Each frame (GuiRendererMixin): once the world is dark, it is left; the veil goes with it. */
    public static void quitFrame() {
        if (veilAt != 0L && !leaving && quitVeil() >= 1.0F) {
            leaving = true;
            net.minecraft.client.Minecraft.getInstance().schedule(() -> {
                leave();
                veilAt = 0L;
                leaving = false;
            });
        }
        twinWatch();
    }

    /** Since when the game, loaded, shows a screen of its own instead of the main menu (ns; 0 not). */
    private static long otherSince;

    /**
     * The window before the game waits for our main menu to take over from it. Should the game, loaded, show a screen of
     * its own first instead (Mojang's notice of a ban before the title), that window must not wait for good: after two
     * seconds of it the game's window is shown, and it goes.
     */
    private static void twinWatch() {
        if (!Twin.active()) {
            otherSince = 0L;
            return;
        }
        net.minecraft.client.Minecraft mc = net.minecraft.client.Minecraft.getInstance();
        var screen = mc.gui.screen();
        if (mc.gui.overlay() != null || mc.level != null || screen == null || screen instanceof LaunchScreen) {
            otherSince = 0L;
            return;
        }
        long now = System.nanoTime();
        if (otherSince == 0L) {
            otherSince = now;
        } else if (now - otherSince > 2_000_000_000L) {
            cursed.recode.CursedRecode.LOGGER.info("[launch] the game shows {} instead of the main menu: the window before the game goes", screen.getClass().getSimpleName());
            Twin.ready();
            showGameWindow(mc);
            Twin.release();
            otherSince = 0L;
        }
    }

    /**
     * The game's window shown under the window before the game, brought up and asked for the focus; minimised, its
     * taskbar button blinking, when that window is.
     */
    private static void showGameWindow(net.minecraft.client.Minecraft mc) {
        long handle = mc.getWindow().handle();
        long hwnd = org.lwjgl.sdl.SDLProperties.SDL_GetPointerProperty(org.lwjgl.sdl.SDLVideo.SDL_GetWindowProperties(handle), org.lwjgl.sdl.SDLVideo.SDL_PROP_WINDOW_WIN32_HWND_POINTER, 0L);
        if (hwnd != 0L && Twin.hidden()) {
            cursed.recode.render.Native.windowLook(hwnd, true);
            org.lwjgl.sdl.SDLVideo.SDL_MinimizeWindow(handle);
            cursed.recode.render.Native.windowMinimised(hwnd);
            return;
        }
        org.lwjgl.sdl.SDLVideo.SDL_ShowWindow(handle);
        if (hwnd != 0L) {
            cursed.recode.render.Native.windowFront(hwnd);
            cursed.recode.render.Native.windowLook(hwnd, true);
        }
    }

    /** Whether the frames go at the menus' own pace now (native vk/pace.rs). */
    private static boolean configuredPaced;

    /**
     * The main menu or another of the client's menus is up with no world: frames at the monitor's own pace
     * (CursedSurface, FramerateLimitTrackerMixin). All of them alike: going from one to another the swap chain stays
     * as it is (made again on each, every change of screens held the frame up some 60–90 ms and the animations jumped).
     */
    public static boolean paced() {
        net.minecraft.client.Minecraft mc = net.minecraft.client.Minecraft.getInstance();
        if (mc == null || mc.level != null || mc.gui.overlay() != null) {
            return false;
        }
        var s = mc.gui.screen();
        return s instanceof LaunchScreen || s != null && cursed.recode.menus.Menus.host(s) != null;
    }

    /**
     * The pace changed (into a world, out of one, a loading screen gone): the frame thread waits for the monitor's
     * refresh after each frame, or no longer does. The swap chain is not made again for it (that cost 60–90 ms a time,
     * and a swap chain made again in another mode came back slow).
     */
    public static void checkPace() {
        boolean p = paced();
        if (p != configuredPaced && cursed.recode.render.NativeTerrain.active()) {
            configuredPaced = p;
            cursed.recode.render.Native.surfacePaced(p);
        }
    }

    @Override
    protected void init() {
        super.init();
        // the account last played (Alt Manager) before the nickname is shown
        cursed.recode.alts.Alts.load(this.minecraft);
        com.google.gson.JsonObject data = new com.google.gson.JsonObject();
        data.addProperty("nick", this.minecraft.getUser().getName());
        LastServer.Entry last = LastServer.get();
        if (last != null) {
            com.google.gson.JsonObject l = new com.google.gson.JsonObject();
            l.addProperty("name", last.name());
            l.addProperty("address", last.address());
            data.add("last", l);
        }
        cursed.recode.menus.MenuHost.open(this.menusId, "main", data);
        if (exitOnShow) {
            // the window was closed elsewhere: this menu only carries the way out, settled as it is after a world (a
            // game started straight into a world has not played its card yet: not its first seconds now)
            exitOnShow = false;
            if (!started) {
                started = true;
                cardT0 = System.nanoTime() - (long) (SETTLE * 1e9);
                fxShift = 0.0F;
            }
            this.beginExit();
            // coming from a world, the menu rises from its lines first (as it does after a world), then goes out as
            // from the main menu
            this.exitAt += (long) (RISE_BEFORE_EXIT * 1e9);
        }
    }

    /** Coming to the main menu only for the way out: how long the menu is up before it sinks (s). */
    private static final float RISE_BEFORE_EXIT = 0.75F;

    /** The way out begins (Quit held, the close button): the menu sinks into its lines, the music goes. */
    private void beginExit() {
        if (this.exitAt != 0) {
            return;
        }
        this.exitAt = System.nanoTime();
        cursed.recode.render.Native.launchMusic(2);
    }

    @Override
    public boolean isPauseScreen() {
        return false;
    }

    private boolean live() {
        return card() >= LIVE && this.exitAt == 0;
    }

    @Override
    public boolean keyPressed(final KeyEvent event) {
        cursed.recode.menus.MenuHost.key(event);
        return true;
    }

    @Override
    public boolean charTyped(final net.minecraft.client.input.CharacterEvent event) {
        cursed.recode.render.Native.menusChar(event.codepoint());
        return true;
    }

    @Override
    public boolean mouseClicked(final MouseButtonEvent event, final boolean doubleClick) {
        float[] p = this.pointer();
        cursed.recode.render.Native.menusMouse(p[0], p[1], cursed.recode.menus.MenuHost.button(event.button()), 1);
        return true;
    }

    @Override
    public boolean mouseReleased(final MouseButtonEvent event) {
        float[] p = this.pointer();
        cursed.recode.render.Native.menusMouse(p[0], p[1], cursed.recode.menus.MenuHost.button(event.button()), 2);
        return true;
    }

    @Override
    public boolean mouseScrolled(final double x, final double y, final double scrollX, final double scrollY) {
        cursed.recode.render.Native.menusScroll((float) scrollY);
        return true;
    }

    /** The pointer in window px (the game's own place for it: an inactive window gets no moves). */
    private float[] pointer() {
        if (DEV_MOUSE != null) {
            int w = this.minecraft.getWindow().getWidth(), h = this.minecraft.getWindow().getHeight();
            return new float[] {DEV_MOUSE[0] * w, DEV_MOUSE[1] * h};
        }
        return new float[] {(float) this.minecraft.mouseHandler.xpos(), (float) this.minecraft.mouseHandler.ypos()};
    }

    /** What the menu asked for. */
    private void act(String a) {
        if (this.exitAt != 0) {
            // on the way out (the menu up only for it after a world, before it sinks): nothing more is taken
            return;
        }
        switch (a) {
            case "continue" -> this.proceed();
            case "open single" -> this.minecraft.gui.setScreen(new SelectWorldScreen(this));
            case "open multi" -> this.minecraft.gui.setScreen(new JoinMultiplayerScreen(this));
            case "open settings" -> this.minecraft.gui.setScreen(new OptionsScreen(this, this.minecraft.options));
            case "music 0" -> cursed.recode.render.Native.launchMusic(0);
            case "music 1" -> cursed.recode.render.Native.launchMusic(1);
            case "quit" -> this.beginExit();
            default -> cursed.recode.CursedRecode.LOGGER.warn("[menus] the main menu asked for {}", a);
        }
    }

    @Override
    public void extractBackground(final GuiGraphicsExtractor graphics, final int mouseX, final int mouseY, final float a) {
    }

    @Override
    public void extractRenderState(final GuiGraphicsExtractor graphics, final int mouseX, final int mouseY, final float a) {
        if (!LaunchFrame.active()) {
            super.extractRenderState(graphics, mouseX, mouseY, a);
            return;
        }
        int w = this.minecraft.getWindow().getWidth(), h = this.minecraft.getWindow().getHeight();
        if (w <= 0 || h <= 0) {
            return;
        }
        if (this.texture == null || this.texture.getWidth(0) != w || this.texture.getHeight(0) != h) {
            this.release();
            this.texture = RenderSystem.getDevice().createTexture("Cursed launch", GpuTexture.USAGE_COPY_DST | GpuTexture.USAGE_TEXTURE_BINDING, GpuFormat.RGBA8_UNORM, w, h, 1, 1);
            this.view = RenderSystem.getDevice().createTextureView(this.texture);
        }
        long real = System.nanoTime();
        long now = paceNow(real);
        // the menu opens under the game's loading screen, when it is not yet the menu's own pace
        checkPace();
        float[] tw = Twin.times();
        if (tw != null && Twin.active() && ((int) tw[3] & 1) != 0) {
            // The window before the game plays the card over this hidden window: its clocks here, to the frame. Once
            // the game draws the menu and the card's wait is over, this window is shown under it (no animation: it
            // was styled so at its making) and draws a few frames there; then the other goes, on the same picture.
            if (tw[1] >= 0.0F) {
                // (its card's second was read at real time: the menu's clock goes on from it)
                cardT0 = real - (long) (tw[1] * 1e9);
                fxShift = tw[2] - tw[1];
                started = true;
            }
            if (this.minecraft.gui.overlay() == null) {
                if (!collected) {
                    // the garbage of the whole load, collected now while this window is hidden and the other plays on
                    // (else the first collections in the menu hold a frame for 40 ms and more)
                    collected = true;
                    System.gc();
                    real = System.nanoTime();
                    now = paceNow(real);
                    this.last = now;
                    this.realLast = real;
                }
                Twin.ready();
                // shown only once it draws evenly: so many frames in a row each under a frame and a half of 60 Hz
                this.steady = real - this.realLast < 25_000_000L ? this.steady + 1 : 0;
                if (((int) tw[3] & 2) != 0 && tw[1] >= HAND_FROM && (this.handFrames > 0 || this.steady >= STEADY)) {
                    this.handFrames++;
                    long minimised = this.handFrames == SHOW_AT && Twin.hidden() ? this.hwnd() : 0L;
                    if (minimised != 0L) {
                        // the window before the game was minimised: the game's comes up minimised, its taskbar button
                        // blinking, and that window goes at once (nothing of it is on the screen to fade)
                        cursed.recode.CursedRecode.LOGGER.info("[launch] the window before the game is minimised: the game's comes up minimised");
                        cursed.recode.render.Native.windowLook(minimised, true);
                        // (SDL told first: a hidden window shown from outside it is otherwise restored at once — and
                        // brought to the front — as its pending state, none, says)
                        org.lwjgl.sdl.SDLVideo.SDL_MinimizeWindow(this.minecraft.getWindow().handle());
                        cursed.recode.render.Native.windowMinimised(minimised);
                        this.handFrames = SHOW_AT + DRAWN;
                        Twin.release();
                    } else if (this.handFrames == SHOW_AT) {
                        cursed.recode.CursedRecode.LOGGER.info("[launch] the game's window shown under the window before the game");
                        org.lwjgl.sdl.SDLVideo.SDL_ShowWindow(this.minecraft.getWindow().handle());
                        // shown from a process in the background Windows puts it under the window in use: brought up
                        // (still under the window before the game, the only one over it), and asked for the focus
                        long hwnd = this.hwnd();
                        if (hwnd != 0L) {
                            cursed.recode.render.Native.windowFront(hwnd);
                        }
                    } else if (this.handFrames == SHOW_AT + DRAWN) {
                        Twin.release();
                        // from now on Windows animates it as any window (minimising, restoring)
                        long hwnd = this.hwnd();
                        if (hwnd != 0L) {
                            cursed.recode.render.Native.windowLook(hwnd, true);
                        }
                    }
                }
            }
        } else if (!started || this.minecraft.gui.overlay() != null && card() < SETTLE) {
            // without it the card starts with the first frame seen: not under the game's loading screen
            started = this.minecraft.gui.overlay() == null;
            cardT0 = now - (long) (START * 1e9);
            fxShift = 0.0F;
        }
        float dt = Math.min(0.1F, (now - this.last) / 1e9F);
        if (TIMES && real - this.realLast > 40_000_000L) {
            cursed.recode.CursedRecode.LOGGER.info("[launch] frame gap {} ms at t {}, overlay {}", (real - this.realLast) / 1_000_000, card(), this.minecraft.gui.overlay() != null);
        }
        if (TIMES && Twin.place() != null && !Twin.active()) {
            // shown: how evenly the game starts its frames (real time; the menu's own clock steps evenly)
            long gap = real - this.realLast;
            if (this.paceFrom == 0) {
                this.paceFrom = real;
            } else {
                this.paceFrames++;
                this.paceSlow += gap > 10_000_000L ? 1 : 0;
                this.paceWorst = Math.max(this.paceWorst, gap);
                this.paceHist[(int) Math.min(this.paceHist.length - 1, gap / 1_000_000L)]++;
                if (real - this.paceFrom > 5_000_000_000L) {
                    cursed.recode.CursedRecode.LOGGER.info("[launch] menu: {} frames in {} s, {} over 10 ms, worst {} ms; by ms {}; the clock stepped {} times by more than a refresh", this.paceFrames,
                            String.format(java.util.Locale.ROOT, "%.1f", (real - this.paceFrom) / 1e9), this.paceSlow, this.paceWorst / 1_000_000, java.util.Arrays.toString(this.paceHist), paceSkips);
                    paceSkips = 0;
                    java.util.Arrays.fill(this.paceHist, 0);
                    this.paceFrom = real;
                    this.paceFrames = this.paceSlow = 0;
                    this.paceWorst = 0;
                }
            }
        }
        this.last = now;
        this.realLast = real;
        // the clock that never waits: the window before the game's for the whole session (the farewell goes on from it)
        float life = tw != null ? tw[0] + (now - real) / 1e9F : (now - LIFE0) / 1e9F;
        lifeShift = life - (now - LIFE0) / 1e9F;
        float t = card();
        float fx = t + fxShift;
        float[] mp = this.pointer();
        float mx = mp[0], my = mp[1];
        this.cursor(w, h, dt, mx, my);
        float ui = Math.max(1.0F, h / 1300.0F);
        if (AUTO_QUIT > 0 && this.exitAt == 0 && t >= SETTLE + AUTO_QUIT) {
            this.beginExit();
        }
        // the menu has settled, the game's resources are in: the world's mesher tables are made now, aside (not when the
        // first world opens: its way in held still for them)
        if (t >= SETTLE + 1.0F && this.exitAt == 0 && this.minecraft.gui.overlay() == null) {
            cursed.recode.world.NativeMesher.warm();
        }
        // the game's volumes for the launch's sound, now and then
        if (now - this.volumeAt > 500_000_000L) {
            this.volumeAt = now;
            var o = this.minecraft.options;
            cursed.recode.render.Native.launchVolume(Twin.mute() ? 0.0F : o.getSoundSourceVolume(net.minecraft.sounds.SoundSource.MASTER),
                    o.getSoundSourceVolume(net.minecraft.sounds.SoundSource.MUSIC));
        }
        // the menu (the menus' page): laid out and drawn for this frame, what it asked for done
        cursed.recode.render.Native.menusMouse(mx, my, 0, 0);
        float exitE = this.exitAt == 0 ? -1.0F : (now - this.exitAt) / 1e9F;
        float[] inp = cursed.recode.menus.MenuHost.inp(new float[] {t, exitE, cursed.recode.render.Native.launchMusicState(0), cursed.recode.render.Native.launchMusicState(1),
                cursed.recode.render.Native.launchMusicState(2)});
        int menus = cursed.recode.render.Native.menusFrame(w, h, (now - LIFE0) / 1e9F, inp, false, this.said);
        cursed.recode.menus.Toasts.framedByMenu();
        // what it asked for is done once this frame is drawn (it may close this screen)
        java.util.List<String> asked = new java.util.ArrayList<>();
        cursed.recode.menus.MenuHost.poll(asked::add);
        if (this.said[19] != this.clicksSeen) {
            this.clicksSeen = this.said[19];
            this.flashAt = System.nanoTime() / 1e9 - 0.05;
        }
        float[] hover = new float[10];
        System.arraycopy(this.said, 1, hover, 0, 10);
        this.left = this.said[13];
        this.footTop = this.said[14];
        // a field of the menu's has the keys (Alt Manager's new nickname): the game takes text while it does
        this.typing(this.said[49] > 0.5F);
        // (v12: the way out is a scene of its own — the world unloads in chunks into the line — nothing dims)
        float fade = cursed.recode.menus.MenuWorld.shown();
        this.flash = this.flashC(t, now / 1e9);
        if (exitE >= 0.0F) {
            LaunchFrame.scene(LaunchFrame.EXIT_MENU, exitE, 1.0F, -1.0F, null);
        }
        LaunchFrame.send(this.texture, t, fx, life, dt, LaunchFrame.LAUNCHED | LaunchFrame.MENUS_FIRE, 1.0F, new float[] {this.curSX, this.curSY, this.curOn}, new float[] {this.lookX, this.lookY},
                this.said[11], this.flash, fade, ui, this.left, this.footTop, hover, 1.0F, NO_LINES, 0, NO_ICONS, NO_GLYPHS, 0, java.util.Arrays.copyOfRange(this.said, 20, 49));
        this.wayOut(now, t, fx, this.left);
        graphics.blit(this.view, RenderSystem.getSamplerCache().getClampToEdge(FilterMode.NEAREST), 0, 0, this.width, this.height, 0.0F, 1.0F, 0.0F, 1.0F);
        if (menus != 0) {
            if (this.menusTexture == null || this.menusTexture.getWidth(0) != w || this.menusTexture.getHeight(0) != h) {
                this.releaseMenus();
                this.menusTexture = RenderSystem.getDevice().createTexture("Cursed menus", GpuTexture.USAGE_COPY_DST | GpuTexture.USAGE_TEXTURE_BINDING, GpuFormat.RGBA8_UNORM, w, h, 1, 1);
                this.menusView = RenderSystem.getDevice().createTextureView(this.menusTexture);
            }
            cursed.recode.render.GuiFrame.sendMenus(this.menusTexture, menus);
            graphics.blit(this.menusView, RenderSystem.getSamplerCache().getClampToEdge(FilterMode.NEAREST), 0, 0, this.width, this.height, 0.0F, 1.0F, 0.0F, 1.0F);
        }
        // the account's head (the game draws it over the picture)
        float headA = this.said[18];
        if (headA > 0.01F && this.said[17] > 0) {
            double s = this.minecraft.getWindow().getGuiScale();
            PlayerFaceExtractor.extractRenderState(graphics, cursed.recode.alts.Alts.skinNow(this.minecraft), (int) Math.round(this.said[15] / s), (int) Math.round(this.said[16] / s),
                    Math.max(1, (int) Math.round(this.said[17] / s)), ARGB.white(headA));
        }
        // Alt Manager's card: the accounts' faces
        cursed.recode.menus.MenuHost.altFaces(graphics);
        // done by the game's loop before its next frame (joining a world in the middle of this one is not safe)
        for (String line : asked) {
            this.minecraft.schedule(() -> {
                if (this.minecraft.gui.screen() == this) {
                    this.act(line);
                }
            });
        }
    }

    private static final float[] NO_LINES = new float[0], NO_GLYPHS = new float[0];
    private static final float[] NO_ICONS = {0, 0, 0, -1, 0, 0, 0, -1, 0, 0, 0, -1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0};
    private float left;

    /** The cursor: a small fire in the world, the camera leans toward it. */
    private void cursor(int w, int h, float dt, float mx, float my) {
        this.curX = mx / w;
        this.curY = my / h;
        this.lookX = toward(this.lookX, (clamp01(this.curX) - 0.5F) * 0.05F, 1.8F, dt);
        this.lookY = toward(this.lookY, -(clamp01(this.curY) - 0.5F) * 0.028F, 1.8F, dt);
        this.curSX = toward(this.curSX, this.curX, 5.0F, dt);
        this.curSY = toward(this.curSY, this.curY, 5.0F, dt);
        this.curOn = toward(this.curOn, 1.0F, 2.0F, dt);
    }

    private float footTop, flash;
    private final int[] paceHist = new int[30];

    private long hwnd() {
        long handle = this.minecraft.getWindow().handle();
        return org.lwjgl.sdl.SDLProperties.SDL_GetPointerProperty(org.lwjgl.sdl.SDLVideo.SDL_GetWindowProperties(handle),
                org.lwjgl.sdl.SDLVideo.SDL_PROP_WINDOW_WIN32_HWND_POINTER, 0L);
    }

    /**
     * The way out, once Quit is held (or the window closed): the menu sinks into its lines, the world unloads in
     * chunks from the camera, its glow goes into the horizon's line (this window); before the window closes the
     * window before the game comes back over it on the same picture (the scene's clock, the look, the cursor's light
     * carried over), closes the window onto the line, the line into the spark (native/src/twin), and the game ends
     * behind it.
     */
    private void wayOut(long now, float t, float fx, float left) {
        if (this.exitAt == 0) {
            return;
        }
        float e = (now - this.exitAt) / 1e9F;
        if (this.farewellAt == 0 && e >= EXIT_HAND) {
            long hwnd = this.hwnd();
            float[] menu = {t, fx, left, this.footTop, e, 0.0F};
            float[] view = {this.lookX, this.lookY, this.curSX, this.curSY, this.curOn, this.said[11], this.flash};
            this.farewellAt = hwnd != 0L && Twin.farewell(hwnd, menu, view) ? now : -1;
        } else if (this.farewellAt == -1 && e >= EXIT_ALONE) {
            // no window before the game: the scene went dark here, the game goes
            this.farewellAt = -2;
            this.minecraft.stop();
        } else if (this.farewellAt > 0 && Twin.handedOver(this.farewellAt, now)) {
            // its first picture is up over this one: this one hides, the game shuts down behind it
            org.lwjgl.sdl.SDLVideo.SDL_HideWindow(this.minecraft.getWindow().handle());
            this.farewellAt = -3;
            this.minecraft.stop();
        }
    }

    /** Sheet lightning: once the menu has settled the fire behind the ridges now and then flares in the clouds. */
    private float flashC(float t, double now) {
        if (t < SETTLE) {
            return 0.0F;
        }
        if (this.nextFlash == 0) {
            this.nextFlash = now + 4 + Math.random() * 5;
        }
        if (now >= this.nextFlash) {
            this.flashAt = now;
            this.nextFlash = now + 7 + Math.random() * 9;
        }
        double x = now - this.flashAt;
        return (float) ((pulseAt(x) + 0.55 * pulseAt(x - 0.2) + 0.3 * pulseAt(x - 0.45)) * 0.9);
    }

    private static double pulseAt(double y) {
        return y < 0 ? 0 : Math.exp(-y * 6.5) * (1 - Math.exp(-y * 45));
    }

    @Override
    public void removed() {
        this.typing(false);
        this.release();
        this.releaseMenus();
        cursed.recode.menus.MenuHost.closed(this.menusId);
    }

    private boolean typing;

    /** The game's text input (SDL's) on while a field of the menu's has the keys: letters come only then. */
    private void typing(boolean on) {
        if (on != this.typing) {
            this.typing = on;
            if (on) {
                this.minecraft.textInputManager().startTextInput(this);
            } else {
                this.minecraft.textInputManager().stopTextInput(this);
            }
        }
    }

    private void releaseMenus() {
        if (this.menusView != null) {
            this.menusView.close();
            this.menusView = null;
        }
        if (this.menusTexture != null) {
            this.menusTexture.close();
            this.menusTexture = null;
        }
    }

    private void release() {
        if (this.view != null) {
            this.view.close();
            this.view = null;
        }
        if (this.texture != null) {
            this.texture.close();
            this.texture = null;
        }
    }
}
