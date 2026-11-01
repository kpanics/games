package cursed.recode.engine;

import com.mojang.blaze3d.pipeline.RenderTarget;
import com.mojang.blaze3d.pipeline.TextureTarget;
import com.mojang.renderpearl.api.GpuFormat;
import cursed.recode.dev.Dev;
import cursed.recode.render.EngineFrame;
import org.joml.Matrix4f;
import org.jspecify.annotations.Nullable;

/**
 * The world drawn smaller and made whole again over the frames (native: engine/upscale.rs). While the level is drawn
 * the game's main target is a smaller one of ours (GameRendererMixin answers mainRenderTarget() with it) and the
 * projection moves by a fraction of a pixel each frame; before the hand and the GUI, the frames are put together into
 * the real main target at the window's size. The hand and the GUI are drawn after, sharp.
 *
 * CURSED_DEV_SCALE=0.5..1 the world's size (1 by default: the same size, only smoothed over the frames);
 * CURSED_DEV_NOTAA leaves the game's own picture.
 */
public final class Upscale {
    private static final boolean OFF = Dev.on("CURSED_DEV_NOTAA");
    /** CURSED_DEV_SCALE overrides the world's size chosen in Video Settings (EngineSettings). */
    private static final float DEV_SCALE = Dev.on("CURSED_DEV_SCALE") ? Float.parseFloat(System.getenv("CURSED_DEV_SCALE")) : -1.0F;
    /** Halton (2, 3): 8 positions inside a pixel. */
    private static final float[][] JITTER = new float[8][];

    static {
        for (int i = 0; i < 8; i++) {
            JITTER[i] = new float[]{halton(i + 1, 2) - 0.5F, halton(i + 1, 3) - 0.5F};
        }
    }

    private static @Nullable TextureTarget scaled;
    private static boolean active;
    /** The level was drawn into the smaller target this frame (its depth is the level's: Chams' ink reads it). */
    private static boolean wasActive;
    private static int frame;
    /** This frame's move, in clip space. */
    private static float jx;
    private static float jy;
    private static int frameWidth;

    private Upscale() {
    }

    private static float halton(int i, int base) {
        float f = 1.0F;
        float r = 0.0F;
        while (i > 0) {
            f /= base;
            r += f * (i % base);
            i /= base;
        }
        return r;
    }

    /** The level begins: from now on the game draws it into the smaller target. */
    public static void begin(RenderTarget main) {
        active = false;
        wasActive = false;
        if (OFF || !EngineFrame.active() || EngineSettings.quality() == EngineSettings.Quality.OFF) {
            return;
        }
        float size = Math.clamp(DEV_SCALE > 0.0F ? DEV_SCALE : EngineSettings.scale(), 0.25F, 1.0F);
        int w = Math.max(1, Math.round(main.width * size));
        int h = Math.max(1, Math.round(main.height * size));
        if (scaled == null) {
            scaled = new TextureTarget("Cursed world", w, h, GpuFormat.RGBA8_UNORM, GpuFormat.D32_FLOAT);
        } else if (scaled.width != w || scaled.height != h) {
            scaled.resize(w, h);
        }
        frame++;
        frameWidth = main.width;
        float[] j = JITTER[frame & 7];
        jx = j[0] * 2.0F / w;
        jy = j[1] * 2.0F / h;
        active = true;
    }

    /** How big the world is drawn against the window this frame (1 when it is not made smaller). */
    public static float scale() {
        return active && scaled != null ? (float) scaled.width / Math.max(1, frameWidth) : 1.0F;
    }

    /** What the game gets for its main target while the level is drawn. */
    public static @Nullable RenderTarget target() {
        return active ? scaled : null;
    }

    /** The level's projection, moved by this frame's fraction of a pixel. */
    public static Matrix4f jitter(Matrix4f projection) {
        if (active) {
            projection.translateLocal(jx, jy, 0.0F);
        }
        return projection;
    }

    /** The level is drawn: the frames are put together into the main target, before the hand. */
    public static void end(RenderTarget main) {
        if (!active || scaled == null) {
            return;
        }
        active = false;
        wasActive = true;
        EngineFrame.upscale(scaled.getColorTexture(), scaled.getDepthTexture(), main.getColorTexture(), jx, jy);
    }

    /** The level's depth this frame: the smaller target's when the level was drawn there, else null (the main one). */
    public static com.mojang.renderpearl.api.textures.@Nullable GpuTexture levelDepth() {
        return wasActive && scaled != null ? scaled.getDepthTexture() : null;
    }
}
