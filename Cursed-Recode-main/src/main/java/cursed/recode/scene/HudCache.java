package cursed.recode.scene;

import com.mojang.blaze3d.pipeline.RenderTarget;
import com.mojang.blaze3d.pipeline.TextureTarget;
import com.mojang.blaze3d.platform.Monitor;
import com.mojang.blaze3d.systems.RenderSystem;
import com.mojang.renderpearl.api.GpuFormat;
import com.mojang.renderpearl.api.textures.FilterMode;
import cursed.recode.dev.Dev;
import java.util.Arrays;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.render.TextureSetup;
import net.minecraft.client.renderer.RenderPipelines;
import net.minecraft.client.renderer.state.gui.BlitRenderState;
import net.minecraft.client.renderer.state.gui.GuiRenderState;
import org.joml.Matrix3x2f;
import org.joml.Vector4f;
import org.jspecify.annotations.Nullable;

/**
 * The HUD above the crosshair drawn at the screen's rate, not the game's: the hotbar, the bars, the effects, the chat,
 * the scoreboard, the titles, the toasts and F3 are extracted and drawn at most hz times a second (the monitor's rate,
 * never under 240); the frames in between show that drawing from a texture. The camera's overlays and the crosshair
 * (the HUD's first stratum, before Hud's nextStratum) stay live every frame.
 *
 * A frame that draws the HUD draws it as the game does, and once more into the texture over nothing (the GUI's blends
 * over a transparent target give it premultiplied). A frame that keeps it extracts nothing above the crosshair: in the
 * upper stratum go the texture's pieces where the HUD has something (its elements' rectangles, in tiles), as
 * premultiplied blits (the game's own pipeline for pictures in the GUI) — the same picture, no pass of its own.
 *
 * Only in the game with no screen or overlay open and the HUD shown; while the frames come about as slowly as the HUD's
 * rate, the HUD is drawn as the game draws it.
 *
 * The frame's course: Gui.extractRenderState decides (begin); Hud at its nextStratum splits the strata or puts the
 * texture's pieces and stops (HudMixin; ToastManagerMixin, the saving indicator and F3 likewise); GuiRenderer.render
 * notes where the HUD has something, GuiRenderer.draw draws it into the texture too (GuiRendererMixin).
 *
 * CURSED_DEV_NOHUDCACHE: the game's own HUD every frame; CURSED_DEV_HUDHZ=n another rate.
 */
public final class HudCache {
    private static final boolean OFF = Dev.on("CURSED_DEV_NOHUDCACHE");
    private static final int DEV_HZ = Dev.on("CURSED_DEV_HUDHZ") ? Integer.parseInt(System.getenv("CURSED_DEV_HUDHZ")) : 0;
    private static final Vector4f CLEAR = new Vector4f(0.0F, 0.0F, 0.0F, 0.0F);
    /** The monitor's rate is looked up again this often (a window can move to another monitor). */
    private static final long MONITOR_EVERY = 2_000_000_000L;
    /** The texture's pieces go in tiles of this many GUI pixels... */
    private static final int TILE = 8;
    /** ...around each element's rectangle grown by this much (shadows and bold offsets reach a pixel past the bounds). */
    private static final int MARGIN = 2;
    private static final Matrix3x2f NO_POSE = new Matrix3x2f();

    private static @Nullable TextureTarget target;
    /** This frame: the HUD's upper strata come from the texture (kept) or are drawn into it (refresh). */
    private static boolean active;
    private static boolean refresh;
    /** This frame: Hud split its strata (the crosshair's stratum below, the rest above). */
    private static boolean split;
    /** The texture holds the HUD of a recent frame of this size. */
    private static boolean valid;
    /** The frames come quickly enough for the texture to be worth it (with a margin both ways). */
    private static boolean fast;
    private static long lastFrame;
    private static long lastRefresh;
    /** The frame's length, smoothed (ns). */
    private static long frameNs = 16_000_000L;
    private static long period = 1_000_000_000L / 240;
    private static long monitorChecked = Long.MIN_VALUE;
    private static int width;
    private static int height;
    private static int scale;

    /** The tiles the HUD covers (a frame that draws it), then the pieces made from them: x0, y0, x1, y1 in GUI pixels. */
    private static boolean[] tiles = new boolean[0];
    private static int columns;
    private static int rows;
    private static int[] pieces = new int[64];
    private static int pieceCount;
    /** The pieces open in the row above and in this one, while they are made. */
    private static int[] openAbove = new int[0];
    private static int[] openHere = new int[0];

    private HudCache() {
    }

    /** Gui.extractRenderState begins: whether this frame's HUD comes from the texture, and whether the texture is drawn again. */
    public static void begin(Minecraft mc, boolean shouldRenderLevel, boolean resourcesLoaded) {
        long now = System.nanoTime();
        long dt = now - lastFrame;
        lastFrame = now;
        if (dt > 0 && dt < 1_000_000_000L) {
            frameNs += (dt - frameNs) / 8;
        }
        split = false;
        refresh = false;
        active = false;
        if (OFF || !shouldRenderLevel || !resourcesLoaded || mc.gui.screen() != null || mc.gui.overlay() != null || mc.gui.hud.isHidden()) {
            valid = false;
            return;
        }
        if (now - monitorChecked > MONITOR_EVERY) {
            monitorChecked = now;
            period = 1_000_000_000L / rate(mc);
        }
        fast = fast ? frameNs < period * 85 / 100 : frameNs < period * 70 / 100;
        if (!fast) {
            valid = false;
            return;
        }
        var window = mc.getWindow();
        if (window.getWidth() != width || window.getHeight() != height || window.getGuiScale() != scale) {
            width = window.getWidth();
            height = window.getHeight();
            scale = window.getGuiScale();
            valid = false;
        }
        active = true;
        // drawn again when the next frame would come after the period is up
        refresh = !valid || now - lastRefresh + frameNs / 2 >= period;
        if (refresh) {
            lastRefresh = now;
        }
    }

    private static int rate(Minecraft mc) {
        if (DEV_HZ > 0) {
            return DEV_HZ;
        }
        int hz = 0;
        try {
            Monitor monitor = mc.getWindow().findBestMonitor();
            if (monitor != null) {
                hz = Math.round(monitor.currentMode().getRefreshRate());
            }
        } catch (RuntimeException ignored) {
        }
        return Math.clamp(hz, 240, 1000);
    }

    private static long hudRefresh;

    /**
     * The client's own HUD (HudLayer, its native picture) drawn again this frame: at the screen's rate as the game's HUD
     * above, every frame while the frames come about as slowly (or CURSED_DEV_NOHUDCACHE). Called after begin.
     */
    public static boolean hudRedraw(Minecraft mc) {
        if (OFF) {
            return true;
        }
        long now = System.nanoTime();
        if (now - monitorChecked > MONITOR_EVERY) {
            monitorChecked = now;
            period = 1_000_000_000L / rate(mc);
        }
        if (frameNs >= period * 70 / 100 || now - hudRefresh + frameNs / 2 >= period || now < hudRefresh) {
            hudRefresh = now;
            return true;
        }
        return false;
    }

    /** The HUD's upper strata come from the texture this frame (drawn into it, or kept). */
    public static boolean active() {
        return active;
    }

    /** This frame keeps the texture: nothing above the crosshair is extracted. */
    public static boolean kept() {
        return active && !refresh;
    }

    /** This frame draws the HUD into the texture (Hud split its strata). */
    public static boolean drawing() {
        return active && refresh && split;
    }

    /** Hud is past the crosshair on a frame that draws it: the strata from here on are the texture's. */
    public static void split(GuiRenderState state) {
        state.blurBeforeThisStratum();
        split = true;
    }

    /** Hud is past the crosshair on a frame that keeps the texture: its pieces go where the HUD has something. */
    public static void place(GuiRenderState state) {
        if (target == null || target.getColorTextureView() == null) {
            return;
        }
        TextureSetup texture = TextureSetup.singleTexture(target.getColorTextureView(), RenderSystem.getSamplerCache().getClampToEdge(FilterMode.NEAREST));
        float gw = (float) width / scale;
        float gh = (float) height / scale;
        for (int i = 0; i < pieceCount; i++) {
            int x0 = pieces[i * 4];
            int y0 = pieces[i * 4 + 1];
            int x1 = pieces[i * 4 + 2];
            int y1 = pieces[i * 4 + 3];
            state.addBlitToCurrentLayer(new BlitRenderState(RenderPipelines.GUI_TEXTURED_PREMULTIPLIED_ALPHA, texture, NO_POSE, x0, y0, x1, y1,
                    x0 / gw, x1 / gw, 1.0F - y0 / gh, 1.0F - y1 / gh, -1, null));
        }
    }

    /** A frame that draws the HUD: where it has something, from the upper strata's rectangles (GuiStrata). */
    public static void cover(GuiStrata strata) {
        if (!drawing()) {
            return;
        }
        int gw = (width + scale - 1) / scale;
        int gh = (height + scale - 1) / scale;
        columns = (gw + TILE - 1) / TILE;
        rows = (gh + TILE - 1) / TILE;
        if (tiles.length < columns * rows) {
            tiles = new boolean[columns * rows];
        } else {
            Arrays.fill(tiles, 0, columns * rows, false);
        }
        strata.cursed$coverAfterBlur(HudCache::mark);
        // runs of tiles in each row; a run the same as one in the row above grows that piece down
        pieceCount = 0;
        if (openAbove.length < columns) {
            openAbove = new int[columns];
            openHere = new int[columns];
        }
        int above = 0;
        for (int row = 0; row < rows; row++) {
            int here = 0;
            for (int c = 0; c < columns; ) {
                if (!tiles[row * columns + c]) {
                    c++;
                    continue;
                }
                int end = c;
                while (end < columns && tiles[row * columns + end]) {
                    end++;
                }
                int x0 = c * TILE;
                int x1 = Math.min(end * TILE, gw);
                int y1 = Math.min((row + 1) * TILE, gh);
                int piece = -1;
                for (int k = 0; k < above; k++) {
                    int p = openAbove[k];
                    if (pieces[p * 4] == x0 && pieces[p * 4 + 2] == x1) {
                        piece = p;
                        break;
                    }
                }
                if (piece >= 0) {
                    pieces[piece * 4 + 3] = y1;
                } else {
                    if ((pieceCount + 1) * 4 > pieces.length) {
                        pieces = Arrays.copyOf(pieces, pieces.length * 2);
                    }
                    piece = pieceCount++;
                    pieces[piece * 4] = x0;
                    pieces[piece * 4 + 1] = row * TILE;
                    pieces[piece * 4 + 2] = x1;
                    pieces[piece * 4 + 3] = y1;
                }
                openHere[here++] = piece;
                c = end;
            }
            int[] t = openAbove;
            openAbove = openHere;
            openHere = t;
            above = here;
        }
    }

    private static void mark(int left, int top, int right, int bottom) {
        int c0 = Math.max(0, Math.floorDiv(left - MARGIN, TILE));
        int c1 = Math.min(columns, Math.floorDiv(right + MARGIN + TILE - 1, TILE));
        int r0 = Math.max(0, Math.floorDiv(top - MARGIN, TILE));
        int r1 = Math.min(rows, Math.floorDiv(bottom + MARGIN + TILE - 1, TILE));
        for (int r = r0; r < r1; r++) {
            Arrays.fill(tiles, r * columns + c0, Math.max(r * columns + c0, r * columns + c1), true);
        }
    }

    /** The texture for a frame that draws the HUD, the main target's size. */
    public static RenderTarget target(RenderTarget main) {
        if (target == null) {
            target = new TextureTarget("Cursed HUD", main.width, main.height, GpuFormat.RGBA8_UNORM, null);
        } else if (target.width != main.width || target.height != main.height) {
            target.resize(main.width, main.height);
        }
        RenderSystem.getDevice().createCommandEncoder().clearColorTexture(target.getColorTexture(), CLEAR);
        return target;
    }

    /** The upper strata are in the texture. */
    public static void drawn() {
        valid = true;
    }

    /** The upper strata's rectangles (GuiRenderStateMixin). */
    public interface GuiStrata {
        void cursed$coverAfterBlur(Rects rects);
    }

    public interface Rects {
        void accept(int left, int top, int right, int bottom);
    }
}
