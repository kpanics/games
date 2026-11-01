package cursed.recode.fx;

import com.mojang.blaze3d.pipeline.RenderTarget;
import com.mojang.renderpearl.api.textures.GpuTexture;
import cursed.recode.render.EngineFrame;
import java.util.HashMap;
import java.util.Map;
import net.minecraft.client.Minecraft;
import net.minecraft.client.renderer.state.level.CameraRenderState;
import org.jspecify.annotations.Nullable;

/**
 * The world's effects (native/src/engine/fx.rs, shaders/engine/fx.comp): each frame the modules that draw in the world
 * (Chunk Grid, …) put their shapes here where they are in the world; after the hitboxes they go to the engine from the
 * camera, drawn over the level's picture against its depth in one pass. Colours are the menu's (sRGB), made linear
 * here.
 */
public final class Fx {
    static final int SEG = 0;
    static final int DOT = 1;
    static final int RING = 2;
    static final int GRID = 3;
    static final int WALL = 4;
    static final int CONE = 5;
    static final int BOX = 6;
    static final int SPRITE = 7;
    static final int RIBBON = 8;
    static final int STAMP = 9;
    static final int FOG = 10;
    static final int BLUR = 11;
    static final int TEXT = 12;
    /** A letter from the second atlas (the system fonts' letters). */
    static final int PAGE1 = 16384;
    /** Shown faintly through what is in front. */
    static final int XRAY = 1;
    /** A segment's ends fade out. */
    static final int FADE_ENDS = 2;
    /** A wall not faded toward its ends. */
    static final int NO_FADE = 4;
    /** An edge's light running round its middle; its colour going to the second round it or along it; its hidden part
     *  dashed. */
    static final int RUN = 8;
    static final int SHIMMER = 16;
    static final int GRADIENT = 32;
    static final int DASH = 64;
    /** A segment no thicker on the frame close by than a few pixels. */
    static final int THIN = 128;
    /** A ring stopped where a wall or a drop met it; a ring bending the picture round its edge. */
    static final int CLIPPED = 256;
    static final int BEND = 512;
    /** A sprite of matter (dust): laid over the picture, not light added to it. */
    static final int SOLID = 1024;
    /** A cone: a light going round it; lit from inside. */
    static final int SWEEP = 2048;
    static final int INNER = 4096;
    private static final int MOST = 4096;
    private static final int EACH = 20;
    private static final float[] F = new float[MOST * EACH];
    private static int n;
    /** This frame's camera: the shapes go from it. */
    private static double cx;
    private static double cy;
    private static double cz;
    private static boolean broken;
    /** Shapes went last frame (or a world was drawn): one empty frame more clears them. */
    private static boolean sent;
    private static long last;

    private Fx() {
    }

    /**
     * After the hitboxes (GameRendererMixin): this frame's shapes to the engine, over `main`'s picture (`depth` the
     * level's, when it was drawn into a smaller target).
     */
    public static void flush(RenderTarget main, @Nullable GpuTexture depth, CameraRenderState camera) {
        if (broken || !EngineFrame.active()) {
            return;
        }
        try {
            n = 0;
            long now = System.nanoTime();
            float dt = last == 0 ? 0.0F : Math.min((now - last) / 1e9F, 0.1F);
            last = now;
            Minecraft mc = Minecraft.getInstance();
            boolean world = mc.level != null && mc.player != null && camera.initialized;
            if (world) {
                // from the eye: the engine casts the rays from there (NativeTerrain.eye)
                net.minecraft.world.phys.Vec3 eye = cursed.recode.render.NativeTerrain.eye(camera.pos);
                cx = eye.x;
                cy = eye.y;
                cz = eye.z;
                float partial = mc.getDeltaTracker().getGameTimeDeltaPartialTick(false);
                MotionBlur.draw(dt);
                ChunkGrid.draw(mc, partial, dt);
                BlockOutline.draw(mc, partial, dt);
                JumpCircle.draw(mc, partial, dt);
                Trails.draw(mc, partial, dt);
                ChinaHat.draw(mc, partial, dt);
                ItemPhysic.draw(mc, partial, dt);
                WorldMusic.draw(mc, partial, dt, camera.pos);
                Tracers.draw();
            } else {
                ChunkGrid.gone();
                BlockOutline.gone();
                JumpCircle.gone();
                Trails.gone();
                ChinaHat.gone();
                ItemPhysic.gone();
                WorldMusic.gone();
            }
            // in a world each frame, shapes or none: the native's own (World Effects' weather) are drawn in it
            if (n == 0 && !sent && !world) {
                return;
            }
            sent = n > 0 || world;
            EngineFrame.fx(main.getColorTexture(), depth != null ? depth : main.getDepthTexture(), F, n);
        } catch (Throwable t) {
            broken = true;
            System.err.println("[cursed] The world's effects are off for this launch: " + t);
        }
    }

    /** Whether the effects draw (our renderer, and they have not failed this launch): the game's own outline and F3+G
     *  lines give way only then. */
    public static boolean working() {
        return !broken && EngineFrame.active();
    }

    /** A module's settings from the menu's "set" line ("1\tLabel=value…"): its switch under "on". */
    static Map<String, String> settings(String module) {
        Map<String, String> v = new HashMap<>();
        String s = cursed.recode.render.Native.guiCall("set\t" + module);
        if (s == null || s.isEmpty()) {
            return v;
        }
        String[] f = s.split("\t");
        v.put("on", f[0]);
        for (int i = 1; i < f.length; i++) {
            int eq = f[i].indexOf('=');
            if (eq > 0) {
                v.put(f[i].substring(0, eq), f[i].substring(eq + 1));
            }
        }
        return v;
    }

    static float number(Map<String, String> v, String key, float or) {
        try {
            return Float.parseFloat(v.getOrDefault(key, Float.toString(or)));
        } catch (NumberFormatException e) {
            return or;
        }
    }

    static boolean on(Map<String, String> v, String key, boolean or) {
        String x = v.get(key);
        return x == null ? or : x.equals("1");
    }

    static String choice(Map<String, String> v, String key, String or) {
        return v.getOrDefault(key, or);
    }

    static int colour(Map<String, String> v, String key, int or) {
        String x = v.get(key);
        if (x == null || x.length() < 6) {
            return or;
        }
        try {
            return Integer.parseInt(x.substring(0, 6), 16);
        } catch (NumberFormatException e) {
            return or;
        }
    }

    private static float linear(int c) {
        return (float) Math.pow((c & 255) / 255.0, 2.2);
    }

    /** A new shape's place: its kind, flags, glow and own number set, its colour (sRGB) and strength; null when full. */
    private static int shape(int kind, int flags, float glow, float own, int rgb, float k) {
        if (n == MOST || k <= 0.0F) {
            return -1;
        }
        int o = n++ * EACH;
        for (int i = 0; i < 12; i++) {
            F[o + i] = 0.0F;
        }
        F[o + 12] = kind;
        F[o + 13] = flags;
        F[o + 14] = glow;
        F[o + 15] = own;
        F[o + 16] = linear(rgb >> 16);
        F[o + 17] = linear(rgb >> 8);
        F[o + 18] = linear(rgb);
        F[o + 19] = k;
        return o;
    }

    private static void at(int o, double x, double y, double z, float w) {
        F[o] = (float) (x - cx);
        F[o + 1] = (float) (y - cy);
        F[o + 2] = (float) (z - cz);
        F[o + 3] = w;
    }

    private static void way(int o, double x, double y, double z, float w) {
        F[o] = (float) x;
        F[o + 1] = (float) y;
        F[o + 2] = (float) z;
        F[o + 3] = w;
    }

    /** A segment of light from a to b, `width` blocks thick at a (its half), `end` times that at b. */
    static void seg(double ax, double ay, double az, double bx, double by, double bz, float width, float end, float glow, int rgb, float k, int flags) {
        int o = shape(SEG, flags, glow, 0.0F, rgb, k);
        if (o >= 0) {
            at(o, ax, ay, az, width);
            at(o + 4, bx, by, bz, end);
        }
    }

    /** A lamp: a core `radius` blocks round and its glow. */
    static void dot(double x, double y, double z, float radius, float glow, int rgb, float k, int flags) {
        int o = shape(DOT, flags, glow, 0.0F, rgb, k);
        if (o >= 0) {
            at(o, x, y, z, radius);
        }
    }

    /** A ring laid on what is there round (x, y, z): `radius` round, its line `width` thick, within `band` up and down;
     *  a light inside it rising toward it (`fill`). */
    static void ring(double x, double y, double z, float radius, float width, float band, float fill, float glow, int rgb, float k) {
        int o = shape(RING, 0, glow, 0.0F, rgb, k);
        if (o >= 0) {
            at(o, x, y, z, radius);
            way(o + 4, width, band, fill, 0.0F);
        }
    }

    /** A jump's wave laid on what is there: as a ring, of a style (0 one ring, 1 ripples following it in), its edge
     *  rough by its seed; stopped as far as `reach` says each of 16 ways (round from +x toward +z, from -180°; none: no
     *  end); bending the picture round its edge. */
    static void wave(double x, double y, double z, float radius, float width, float band, float fill, float glow, int style, float seed, float @Nullable [] reach,
                     boolean bend, int rgb, float k) {
        int o = shape(RING, (reach != null ? CLIPPED : 0) | (bend ? BEND : 0), glow, seed, rgb, k);
        if (o >= 0) {
            at(o, x, y, z, radius);
            way(o + 4, width, band, fill, style);
            if (reach != null) {
                int[] packed = new int[4];
                for (int i = 0; i < 16; i++) {
                    int q = Math.clamp(Math.round(reach[i] / 0.3F), 0, 31);
                    packed[i >> 2] |= q << ((i & 3) * 5);
                }
                way(o + 8, packed[0], packed[1], packed[2], packed[3]);
            }
        }
    }

    /** A grid laid on the surface round (x, y, z) as far as `reach`: its lines every `period` blocks, `width` thick; the
     *  middle's cell's own lines in `mine` (when not -1). */
    static void grid(double x, double y, double z, float reach, float period, float width, int mine, int rgb, float k) {
        int o = shape(GRID, 0, 0.0F, 0.0F, rgb, k);
        if (o >= 0) {
            at(o, x, y, z, reach);
            way(o + 4, period, width, mine != -1 ? 1.0F : 0.0F, 0.0F);
            if (mine != -1) {
                way(o + 8, linear(mine >> 16), linear(mine >> 8), linear(mine), 0.0F);
            }
        }
    }

    /** A wall of glass from (ox, oy, oz) along u and v (v its height: it fades toward its ends), its grid every `cell`
     *  blocks with every `major`th line brighter, its lines `width` thick, the glass `sheet` bright. */
    static void wall(double ox, double oy, double oz, double ux, double uy, double uz, double vx, double vy, double vz, float cell, float width, float sheet, float major,
                     float glow, int rgb, float k, int flags) {
        int o = shape(WALL, flags, glow, major, rgb, k);
        if (o >= 0) {
            at(o, ox, oy, oz, cell);
            way(o + 4, ux, uy, uz, width);
            way(o + 8, vx, vy, vz, sheet);
        }
    }

    /** A cone of glass: its tip, which way its base is and how far, its base's radius; how tinted (toward 1 a solid
     *  thing), how it shines, rings down it, flat faces round it (0: round), how far it has turned; its rim's light (or,
     *  INNER, the light inside it). */
    static void cone(double x, double y, double z, double ax, double ay, double az, float height, float radius, float opacity, float shine, float rings, float facets,
                     float turn, float glow, int rgb, float k, int flags) {
        int o = shape(CONE, flags, glow, turn, rgb, k);
        if (o >= 0) {
            at(o, x, y, z, height);
            way(o + 4, ax, ay, az, radius);
            way(o + 8, opacity, shine, rings, facets);
        }
    }

    /** A box's faces, tinted `strength`. */
    static void box(double x0, double y0, double z0, double x1, double y1, double z1, float strength, int rgb, float k, int flags) {
        box(x0, y0, z0, x1, y1, z1, strength, 0.0F, 0.0F, 0.0F, 0.0F, 1.0F, rgb, k, flags);
    }

    /** A box's faces, tinted `strength`; a fill risen to `level` (0..1: up its sides, then over its top from the edges
     *  in) `fill` bright with its edge `band`; cracks of light over it as far as `cracks` (0..1), `cells` a block. */
    static void box(double x0, double y0, double z0, double x1, double y1, double z1, float strength, float fill, float level, float band, float cracks, float cells,
                    int rgb, float k, int flags) {
        int o = shape(BOX, flags, 0.0F, 0.0F, rgb, k);
        if (o >= 0) {
            at(o, x0, y0, z0, strength);
            at(o + 4, x1, y1, z1, fill);
            way(o + 8, level, band, cracks, cells);
        }
    }

    /** An edge of an outline from a to b: a segment `width` thick; its light running round (cx, cz) at `speed` (RUN);
     *  its colour going to `second` along it (GRADIENT) or round (SHIMMER); its hidden part dashed every `dash` blocks
     *  (DASH). */
    static void edge(double ax, double ay, double az, double bx, double by, double bz, float width, float glow, int rgb, int second, double mx, double mz, float speed,
                     float dash, float k, int flags) {
        int o = shape(SEG, flags, glow, (second & 0xffffff) + 1, rgb, k);
        if (o >= 0) {
            at(o, ax, ay, az, width);
            at(o + 4, bx, by, bz, 1.0F);
            way(o + 8, mx - cx, mz - cz, speed, dash);
        }
    }

    /** A piece of a ribbon of light from a to b, `wa` and `wb` blocks wide each side of them across `d` (a unit); how far
     *  along the whole ribbon its ends are (its flow runs along it when `flow`). */
    static void ribbon(double ax, double ay, double az, float wa, double bx, double by, double bz, float wb, double dx, double dy, double dz, float alongA, float alongB,
                       boolean flow, int rgb, float k, int flags) {
        int o = shape(RIBBON, flags, flow ? 1.0F : 0.0F, alongB, rgb, k);
        if (o >= 0) {
            at(o, ax, ay, az, wa);
            at(o + 4, bx, by, bz, wb);
            way(o + 8, dx, dy, dz, alongA);
        }
    }

    /** A footprint laid on what is there at (x, y, z), `size` blocks, going the way (fx, fz), within `band` up and down;
     *  a left foot's or a right one's. */
    static void stamp(double x, double y, double z, float size, double fx, double fz, float band, boolean left, float glow, int rgb, float k) {
        int o = shape(STAMP, 0, glow, 0.0F, rgb, k);
        if (o >= 0) {
            at(o, x, y, z, size);
            way(o + 4, fx, 0.0, fz, band);
            way(o + 8, left ? 1.0F : 0.0F, 0.0F, 0.0F, 0.0F);
        }
    }

    /** A letter standing in the world: its quad from its top left corner across `u` and down `d` (blocks, whole), its
     *  rectangle in the menu's atlas (px) and which atlas. */
    static void letter(double x, double y, double z, double ux, double uy, double uz, double dx, double dy, double dz, float ax, float ay, float aw, float ah, int page,
                       float glow, int rgb, float k, int flags) {
        int o = shape(TEXT, flags | (page == 1 ? PAGE1 : 0), glow, ah, rgb, k);
        if (o >= 0) {
            at(o, x, y, z, ax);
            way(o + 4, ux, uy, uz, ay);
            way(o + 8, dx, dy, dz, aw);
        }
    }

    /** The motion blur: the shutter (of a 60th of a second), how many samples along each smear, only the turn, the longest
     *  smear (of the frame's width). */
    static void blur(float shutter, int samples, boolean onlyTurning, float longest, float k) {
        int o = shape(BLUR, 0, 0.0F, 0.0F, 0xffffff, k);
        if (o >= 0) {
            way(o, shutter, samples, onlyTurning ? 1.0F : 0.0F, longest);
        }
    }

    /** A layer of fog from `bottom` to `top` (the world's heights), `density` thick a block, in wisps `wisps` blocks
     *  across the wind has drifted (dx, dz) blocks. */
    static void fog(double bottom, double top, float density, double dx, double dz, float wisps, int rgb, float k) {
        int o = shape(FOG, 0, 0.0F, 0.0F, rgb, k);
        if (o >= 0) {
            way(o, bottom - cy, top - cy, density, 0.0F);
            way(o + 4, dx, wisps, dz, 0.0F);
        }
    }

    /** A small shape turned to the camera, `size` blocks: 0 round, 1 a leaf, 2 a flake, 3 an ember; turned `turn`. */
    static void sprite(double x, double y, double z, float size, int form, float turn, int rgb, float k, int flags) {
        sprite(x, y, z, size, form, turn, 0.0F, rgb, k, flags);
    }

    /** The same narrowed across to `narrow` of its width (0..1: a leaf turned edge on, a thing lying flat seen from the
     *  side, a streak; 0: not narrowed). */
    static void sprite(double x, double y, double z, float size, int form, float turn, float narrow, int rgb, float k, int flags) {
        int o = shape(SPRITE, flags, 0.0F, 0.0F, rgb, k);
        if (o >= 0) {
            at(o, x, y, z, size);
            way(o + 4, form, turn, narrow, 0.0F);
        }
    }
}
