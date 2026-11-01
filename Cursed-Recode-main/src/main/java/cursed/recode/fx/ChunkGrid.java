package cursed.recode.fx;

import java.util.Map;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.components.debug.DebugScreenEntries;
import net.minecraft.client.gui.components.debug.DebugScreenEntryStatus;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.phys.Vec3;

/**
 * Chunk Grid (the menu's module "chunk-grid"; the game's F3+G is its switch, the same as the menu's:
 * KeyboardHandlerMixin; the game's own lines not drawn: ChunkBorderRendererMixin): the chunks' borders as lines laid on the ground round you; your chunk's four walls of glass
 * with a grid of blocks on them and its sections' lines; beams of light at its corners and fainter ones at its
 * neighbours'; your section's edges. The walls glide after you into the next chunk, a pulse goes out over the ground
 * as you enter it; all of it comes in and goes softly.
 */
public final class ChunkGrid {
    /** How long the pulse on entering a chunk runs (s). */
    private static final float PULSE = 0.9F;
    private static Map<String, String> set = Map.of();
    private static long polled;
    /** The walls' chunk and section where they are drawn (gliding to yours). */
    private static double gx;
    private static double gy;
    private static double gz;
    private static boolean placed;
    private static int chunkX;
    private static int chunkZ;
    /** How far in it is (0..1), how long since you entered a chunk (s), where you were then. */
    private static float shown;
    private static float pulse = PULSE;
    private static double px;
    private static double py;
    private static double pz;

    private ChunkGrid() {
    }

    private static Map<String, String> settings() {
        long now = System.nanoTime();
        if (now - polled > 250_000_000L) {
            polled = now;
            set = Fx.settings("chunk-grid");
        }
        return set;
    }

    /** F3+G while the effects draw (Fx.working): the module switched, the same as from the menu; whether it is on now. */
    public static boolean flip() {
        Minecraft mc = Minecraft.getInstance();
        boolean on = !(Fx.on(settings(), "on", false) || debugShown(mc));
        turn(mc, on);
        return on;
    }

    /** Whether the game's own F3+G is on (from before, its options' screen): the module is switched on for it. */
    private static boolean debugShown(Minecraft mc) {
        return mc.debugEntries != null && mc.debugEntries.isCurrentlyEnabled(DebugScreenEntries.CHUNK_BORDERS);
    }

    /** The module on or off, the game's own F3+G kept off: one switch shows and hides the grid, the menu's. */
    private static void turn(Minecraft mc, boolean on) {
        if (mc.debugEntries != null && mc.debugEntries.getStatus(DebugScreenEntries.CHUNK_BORDERS) != DebugScreenEntryStatus.NEVER) {
            mc.debugEntries.setStatus(DebugScreenEntries.CHUNK_BORDERS, DebugScreenEntryStatus.NEVER);
        }
        cursed.recode.render.Native.guiCall("switch\tchunk-grid\t" + (on ? 1 : 0));
        // asked anew
        polled = 0L;
    }

    /** No world: nothing kept. */
    static void gone() {
        placed = false;
        shown = 0.0F;
    }

    static void draw(Minecraft mc, float partial, float dt) {
        if (debugShown(mc)) {
            turn(mc, true);
        }
        Map<String, String> v = settings();
        boolean want = Fx.on(v, "on", false);
        float step = Fx.on(v, "Fade in and out", true) ? dt / 0.25F : 1.0F;
        shown = want ? Math.min(1.0F, shown + step) : Math.max(0.0F, shown - step);
        Entity e = mc.getCameraEntity() != null ? mc.getCameraEntity() : mc.player;
        if (shown <= 0.0F || e == null) {
            placed = false;
            return;
        }
        Vec3 p = e.getPosition(partial);
        int cx = (int) Math.floor(p.x) >> 4;
        int cz = (int) Math.floor(p.z) >> 4;
        double tx = cx * 16.0;
        double tz = cz * 16.0;
        double ty = ((int) Math.floor(p.y) >> 4) * 16.0;
        boolean glide = Fx.on(v, "Glide", true);
        if (!placed || !glide || Math.abs(tx - gx) > 64.0 || Math.abs(tz - gz) > 64.0 || Math.abs(ty - gy) > 64.0) {
            gx = tx;
            gy = ty;
            gz = tz;
        } else {
            double k = 1.0 - Math.exp(-dt * 12.0);
            gx += (tx - gx) * k;
            gy += (ty - gy) * k;
            gz += (tz - gz) * k;
        }
        if (placed && (cx != chunkX || cz != chunkZ) && Fx.on(v, "Pulse on entering", true)) {
            pulse = 0.0F;
            px = p.x;
            py = p.y;
            pz = p.z;
        }
        placed = true;
        chunkX = cx;
        chunkZ = cz;
        pulse = Math.min(PULSE, pulse + dt);

        float a = shown * shown * (3.0F - 2.0F * shown);
        float lw = Fx.number(v, "Line width", 1.0F);
        float glow = Fx.number(v, "Glow", 1.0F);
        int colour = Fx.colour(v, "Colour", 0xc9ced8);
        int mine = Fx.colour(v, "Your chunk", 0xffffff);
        int xray = Fx.on(v, "Through blocks", false) ? Fx.XRAY : 0;
        float q = pulse / PULSE;
        float flash = pulse < PULSE ? (1.0F - q) * (1.0F - q) : 0.0F;

        // lines on the ground as far as the radius, your chunk's in its colour
        if (Fx.on(v, "Lines on the ground", true)) {
            float reach = Fx.number(v, "Radius", 3.0F) * 16.0F + 8.0F;
            Fx.grid(p.x, p.y, p.z, reach, 16.0F, 0.045F * lw, mine, colour, 0.85F * a);
        }
        // the walls from as far below your section as the height to as far above it, whole sections
        double h = Math.ceil(Fx.number(v, "Height", 24.0F) / 16.0) * 16.0;
        double lo = gy - h;
        double tall = 16.0 + 2.0 * h;
        if (Fx.on(v, "Walls", true)) {
            boolean blocks = Fx.on(v, "Block grid", true);
            boolean sections = Fx.on(v, "Sections", true);
            float cell = blocks ? Fx.number(v, "Grid step", 2.0F) : 16.0F;
            float major = sections ? 16.0F / cell : 1e5F;
            float width = blocks || sections ? 0.02F * lw : 0.0F;
            float sheet = Fx.number(v, "Wall opacity", 12.0F) / 100.0F * (1.0F + 2.5F * flash);
            float k = 0.8F * a;
            Fx.wall(gx, lo, gz, 16.0, 0.0, 0.0, 0.0, tall, 0.0, cell, width, sheet, major, glow, mine, k, xray);
            Fx.wall(gx, lo, gz + 16.0, 16.0, 0.0, 0.0, 0.0, tall, 0.0, cell, width, sheet, major, glow, mine, k, xray);
            Fx.wall(gx, lo, gz, 0.0, 0.0, 16.0, 0.0, tall, 0.0, cell, width, sheet, major, glow, mine, k, xray);
            Fx.wall(gx + 16.0, lo, gz, 0.0, 0.0, 16.0, 0.0, tall, 0.0, cell, width, sheet, major, glow, mine, k, xray);
        }
        // beams at the corners: your chunk's, then its neighbours' fainter
        boolean beams = Fx.on(v, "Corner beams", true);
        if (beams) {
            boolean neighbours = Fx.on(v, "Neighbours' corners", true);
            for (int i = -16; i <= 32; i += 16) {
                for (int j = -16; j <= 32; j += 16) {
                    boolean own = (i == 0 || i == 16) && (j == 0 || j == 16);
                    if (!own && !neighbours) {
                        continue;
                    }
                    double x = gx + i;
                    double z = gz + j;
                    Fx.seg(x, lo, z, x, lo + tall, z, (own ? 0.03F : 0.022F) * lw, 1.0F, glow, own ? mine : colour, (own ? 0.9F : 0.45F) * a, Fx.FADE_ENDS | xray);
                }
            }
        }
        // your section's edges, seen through what is in front (the beams are its upright ones)
        if (Fx.on(v, "Your section", true)) {
            float w = 0.016F * lw;
            float k = 0.55F * a;
            for (int y = 0; y <= 16; y += 16) {
                double yy = gy + y;
                Fx.seg(gx, yy, gz, gx + 16.0, yy, gz, w, 1.0F, glow, mine, k, Fx.XRAY);
                Fx.seg(gx, yy, gz + 16.0, gx + 16.0, yy, gz + 16.0, w, 1.0F, glow, mine, k, Fx.XRAY);
                Fx.seg(gx, yy, gz, gx, yy, gz + 16.0, w, 1.0F, glow, mine, k, Fx.XRAY);
                Fx.seg(gx + 16.0, yy, gz, gx + 16.0, yy, gz + 16.0, w, 1.0F, glow, mine, k, Fx.XRAY);
            }
            if (!beams) {
                for (int i = 0; i <= 16; i += 16) {
                    for (int j = 0; j <= 16; j += 16) {
                        Fx.seg(gx + i, gy, gz + j, gx + i, gy + 16.0, gz + j, w, 1.0F, glow, mine, k, Fx.XRAY);
                    }
                }
            }
        }
        // the pulse on entering: a ring going out over the ground from where you crossed
        if (pulse < PULSE) {
            float out = 1.0F - (1.0F - q) * (1.0F - q) * (1.0F - q);
            Fx.ring(px, py + 0.05, pz, 0.6F + 22.0F * out, (0.06F + 0.12F * q) * lw, 3.0F, 0.35F * (1.0F - q), glow, mine, a * (float) Math.pow(1.0F - q, 1.5));
        }
    }
}
