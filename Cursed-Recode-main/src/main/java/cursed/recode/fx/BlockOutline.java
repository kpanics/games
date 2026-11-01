package cursed.recode.fx;

import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import net.minecraft.client.Minecraft;
import net.minecraft.client.multiplayer.ClientLevel;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.item.BlockItem;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.BlockHitResult;
import net.minecraft.world.phys.HitResult;
import net.minecraft.world.phys.shapes.CollisionContext;
import net.minecraft.world.phys.shapes.VoxelShape;
import org.jspecify.annotations.Nullable;

/**
 * Block Outline (the menu's module "block-outline"; the game's own outline is not drawn while it is on:
 * BlockOutlineMixin): the block you look at outlined your way — its exact shape's edges or its box, whole or only their
 * corners; lines of light, a light running round them, one colour or two going into each other along them or round;
 * its faces filled faintly, the face you aim at lit; what of it is hidden shown faintly, dashed or not at all. It glides
 * from block to block and grows in; as it breaks a fill rises up its sides and over its top and cracks of light spread
 * over it; broken, it flashes outward and throws sparks; a block placed pops into its place.
 */
public final class BlockOutline {
    /** Lines' half-thickness at "Line width" 1 (blocks); a dash's length. */
    private static final float WIDTH = 0.008F;
    private static final float DASH_LENGTH = 0.09F;
    /** How long a flash on breaking or placing runs, how long sparks live at most (s). */
    private static final float FLASH = 0.45F;
    private static final float SPARK = 0.9F;
    private static final int MOST_SPARKS = 96;
    private static Map<String, String> set = Map.of();
    private static long polled;
    /** The outlined block, its shape's edges and boxes (block-local) and their bounds; the face aimed at. */
    private static @Nullable BlockPos at;
    private static @Nullable BlockState state;
    private static List<double[]> edges = new ArrayList<>();
    private static List<AABB> boxes = new ArrayList<>();
    private static AABB bounds = new AABB(0, 0, 0, 1, 1, 1);
    private static Direction face = Direction.UP;
    /** The aimed face's current quads and the old ones crossfading out; a quad's last value is its alpha weight. */
    private static final float FACE_FADE = 0.14F;
    private static List<double[]> oldFace = new ArrayList<>();
    private static List<double[]> currentFace = new ArrayList<>();
    private static float faceBlend = 1.0F;
    private static float faceAlpha;
    /** Where the outline is drawn (the world's, gliding to its block's), how far in it is, how far the breaking is. */
    private static final double[] LO = new double[3];
    private static final double[] HI = new double[3];
    private static boolean placed;
    private static float alpha;
    private static float level;
    private static final List<Flash> FLASHES = new ArrayList<>();
    private static final double[] SP = new double[MOST_SPARKS * 3];
    private static final double[] SV = new double[MOST_SPARKS * 3];
    private static final float[] SAGE = new float[MOST_SPARKS];
    private static final float[] SLIFE = new float[MOST_SPARKS];
    private static int sparks;
    /** A block placed: where it may have gone (the face's side, or the clicked one when it was replaced), what. */
    private static @Nullable BlockPos placeA;
    private static @Nullable BlockPos placeB;
    private static @Nullable Block placeWhat;
    private static int placeFrames;
    private static float time;
    /** CURSED_DEV_OUTLINE: the outlined block and the breaking, in the log twice a second. */
    private static final boolean LOG = cursed.recode.dev.Dev.on("CURSED_DEV_OUTLINE");
    private static long logged;

    /** A flash: a block broken (going out) or placed (coming in), its edges and bounds where it was. */
    private record Flash(BlockPos pos, List<double[]> edges, AABB bounds, boolean pop, float[] age) {
    }

    private BlockOutline() {
    }

    private static Map<String, String> settings() {
        long now = System.nanoTime();
        if (now - polled > 250_000_000L) {
            polled = now;
            set = Fx.settings("block-outline");
        }
        return set;
    }

    /** Whether the client draws the outline (the game's own is not drawn then). */
    public static boolean drawing() {
        return Fx.working() && Fx.on(settings(), "on", true);
    }

    /** A block broken by you (MultiPlayerGameModeMixin: what it was, once it is gone): its flash and sparks. */
    public static void broken(BlockPos pos, BlockState s) {
        Minecraft mc = Minecraft.getInstance();
        ClientLevel level = mc.level;
        if (level == null || s.isAir() || !drawing()) {
            return;
        }
        Map<String, String> v = settings();
        VoxelShape shape = s.getShape(level, pos, CollisionContext.of(mc.getCameraEntity() != null ? mc.getCameraEntity() : mc.player));
        if (shape.isEmpty()) {
            return;
        }
        List<double[]> e = edgesOf(shape, Fx.choice(v, "Shape", "Exact").equals("Box"));
        AABB b = shape.bounds();
        if (Fx.on(v, "Flash when broken", true)) {
            flash(new Flash(pos.immutable(), e, b, false, new float[1]));
        }
        if (Fx.on(v, "Sparks when broken", true)) {
            java.util.Random r = new java.util.Random();
            double mx = pos.getX() + (b.minX + b.maxX) * 0.5;
            double my = pos.getY() + (b.minY + b.maxY) * 0.5;
            double mz = pos.getZ() + (b.minZ + b.maxZ) * 0.5;
            int n = Math.round(Fx.number(v, "Sparks", 24.0F));
            for (int i = 0; i < n && !e.isEmpty(); i++) {
                double[] q = e.get(r.nextInt(e.size()));
                double t = r.nextDouble();
                double x = pos.getX() + q[0] + (q[3] - q[0]) * t;
                double y = pos.getY() + q[1] + (q[4] - q[1]) * t;
                double z = pos.getZ() + q[2] + (q[5] - q[2]) * t;
                double s0 = 1.2 + r.nextDouble() * 2.6;
                spark(x, y, z, (x - mx) * s0 + r.nextGaussian() * 0.4, 1.5 + r.nextDouble() * 3.0 + (y - my) * s0, (z - mz) * s0 + r.nextGaussian() * 0.4,
                        0.45F + r.nextFloat() * (SPARK - 0.45F));
            }
        }
    }

    /** An item about to be used on a block (MultiPlayerGameModeMixin): if its block goes where it was not, it pops into
     *  its place. */
    public static void using(BlockHitResult hit, ItemStack stack) {
        ClientLevel level = Minecraft.getInstance().level;
        if (level == null || !(stack.getItem() instanceof BlockItem item) || !drawing()) {
            return;
        }
        Block what = item.getBlock();
        BlockPos a = hit.getBlockPos().relative(hit.getDirection());
        BlockPos b = hit.getBlockPos();
        placeA = level.getBlockState(a).is(what) ? null : a;
        placeB = level.getBlockState(b).is(what) ? null : b;
        placeWhat = what;
        placeFrames = 6;
    }

    private static void flash(Flash f) {
        if (FLASHES.size() >= 6) {
            FLASHES.remove(0);
        }
        FLASHES.add(f);
    }

    private static void spark(double x, double y, double z, double vx, double vy, double vz, float life) {
        int i = sparks < MOST_SPARKS ? sparks++ : (int) (Math.random() * MOST_SPARKS);
        SP[i * 3] = x;
        SP[i * 3 + 1] = y;
        SP[i * 3 + 2] = z;
        SV[i * 3] = vx;
        SV[i * 3 + 1] = vy;
        SV[i * 3 + 2] = vz;
        SAGE[i] = 0.0F;
        SLIFE[i] = life;
    }

    /** A shape's edges (block-local: x0 y0 z0 x1 y1 z1), or its bounds' when only its box. */
    private static List<double[]> edgesOf(VoxelShape shape, boolean box) {
        List<double[]> e = new ArrayList<>();
        if (box) {
            AABB b = shape.bounds();
            double[] x = {b.minX, b.maxX};
            double[] y = {b.minY, b.maxY};
            double[] z = {b.minZ, b.maxZ};
            for (int i = 0; i < 2; i++) {
                for (int j = 0; j < 2; j++) {
                    e.add(new double[] {x[0], y[i], z[j], x[1], y[i], z[j]});
                    e.add(new double[] {x[i], y[0], z[j], x[i], y[1], z[j]});
                    e.add(new double[] {x[i], y[j], z[0], x[i], y[j], z[1]});
                }
            }
        } else {
            shape.forAllEdges((x0, y0, z0, x1, y1, z1) -> e.add(new double[] {x0, y0, z0, x1, y1, z1}));
        }
        return e;
    }

    /** No world: nothing kept. */
    static void gone() {
        at = null;
        placed = false;
        alpha = 0.0F;
        oldFace.clear();
        currentFace.clear();
        faceBlend = 1.0F;
        faceAlpha = 0.0F;
        FLASHES.clear();
        sparks = 0;
    }

    private static int mixColour(int a, int b, double t) {
        t = Math.clamp(t, 0.0, 1.0);
        int r = (int) Math.round(((a >> 16) & 255) + (((b >> 16) & 255) - ((a >> 16) & 255)) * t);
        int g = (int) Math.round(((a >> 8) & 255) + (((b >> 8) & 255) - ((a >> 8) & 255)) * t);
        int bl = (int) Math.round((a & 255) + ((b & 255) - (a & 255)) * t);
        return r << 16 | g << 8 | bl;
    }

    static void draw(Minecraft mc, float partial, float dt) {
        Map<String, String> v = settings();
        time += dt;
        boolean on = Fx.on(v, "on", true);
        ClientLevel lv = mc.level;
        Entity cam = mc.getCameraEntity() != null ? mc.getCameraEntity() : mc.player;
        boolean exact = !Fx.choice(v, "Shape", "Exact").equals("Box");
        // this frame's block
        BlockPos pos = null;
        BlockState st = null;
        Direction aimed = face;
        if (on && lv != null && cam != null && ((cursed.recode.mixin.client.GameRendererOutlineAccess) mc.gameRenderer).cursed$shouldRenderBlockOutline() && mc.hitResult instanceof BlockHitResult hit
                && hit.getType() != HitResult.Type.MISS) {
            BlockPos p = hit.getBlockPos();
            BlockState s = lv.getBlockState(p);
            if (!s.isAir() && lv.getWorldBorder().isWithinBounds(p)) {
                pos = p;
                st = s;
                aimed = hit.getDirection();
            }
        }
        boolean changeFace = pos != null && (!pos.equals(at) || st != state || aimed != face);
        if (pos != null && (!pos.equals(at) || st != state)) {
            VoxelShape shape = st.getShape(lv, pos, CollisionContext.of(cam));
            if (shape.isEmpty()) {
                pos = null;
            } else {
                at = pos.immutable();
                state = st;
                edges = edgesOf(shape, !exact);
                boxes = exact ? shape.toAabbs() : List.of(shape.bounds());
                bounds = shape.bounds();
            }
        }
        if (changeFace && pos != null) {
            transitionFace();
            face = aimed;
        }
        boolean here = pos != null;
        // the outline's place: gliding to its block's, growing in from smaller when it was not there
        if (here) {
            double[] tlo = {at.getX() + bounds.minX, at.getY() + bounds.minY, at.getZ() + bounds.minZ};
            double[] thi = {at.getX() + bounds.maxX, at.getY() + bounds.maxY, at.getZ() + bounds.maxZ};
            if (!placed || alpha <= 0.0F) {
                double g = Fx.on(v, "Grow in", true) ? 0.3 : 0.0;
                for (int i = 0; i < 3; i++) {
                    double m = (tlo[i] + thi[i]) * 0.5;
                    LO[i] = tlo[i] + (m - tlo[i]) * g;
                    HI[i] = thi[i] + (m - thi[i]) * g;
                }
                placed = true;
            }
            double k = Fx.on(v, "Glide", true) ? 1.0 - Math.exp(-dt * 22.0 * Fx.number(v, "Glide speed", 100.0F) / 100.0) : 1.0;
            for (int i = 0; i < 3; i++) {
                LO[i] += (tlo[i] - LO[i]) * k;
                HI[i] += (thi[i] - HI[i]) * k;
            }
        }
        alpha = here ? Math.min(1.0F, alpha + dt / 0.12F) : Math.max(0.0F, alpha - dt / 0.15F);
        // how far the breaking is (yours, of this block: the game keeps it until it is broken or let go)
        float breaking = 0.0F;
        if (here && mc.gameMode != null && mc.gameMode.destroyProgress > 0.0F && at.equals(mc.gameMode.destroyBlockPos)) {
            breaking = Math.clamp(mc.gameMode.destroyProgress, 0.0F, 1.0F);
        }
        level = breaking > level ? level + (breaking - level) * (1.0F - (float) Math.exp(-dt * 25.0)) : Math.max(breaking, level - dt / 0.15F);
        if (LOG && System.nanoTime() - logged > 500_000_000L && mc.gameMode != null) {
            logged = System.nanoTime();
            org.slf4j.LoggerFactory.getLogger("BlockOutline").info("[outline] at {} destroying {} {} progress {} level {}", at, mc.gameMode.isDestroying(), mc.gameMode.destroyBlockPos,
                    mc.gameMode.destroyProgress, level);
        }

        int colour = Fx.colour(v, "Colour", 0xffffff);
        String colouring = Fx.choice(v, "Colouring", "One colour");
        int second = colouring.equals("One colour") ? colour : Fx.colour(v, "Second colour", 0x8a8a8a);
        float lw = Fx.number(v, "Line width", 1.0F);
        float glow = Fx.number(v, "Glow", 1.0F);
        String hidden = Fx.choice(v, "Hidden edges", "Faint");
        int hide = hidden.equals("Dashed") ? Fx.DASH : hidden.equals("Faint") ? Fx.XRAY : 0;
        int look = Fx.THIN | hide | (colouring.equals("Gradient") ? Fx.GRADIENT : colouring.equals("Shimmer") ? Fx.SHIMMER : 0)
                | (Fx.on(v, "Running light", false) ? Fx.RUN : 0);
        float speed = Fx.number(v, "Light speed", 1.0F);
        boolean corners = Fx.choice(v, "Edges", "Whole").equals("Corners");
        float cornerLen = Fx.number(v, "Corner length", 25.0F) / 100.0F;

        if (alpha > 0.0F && at != null) {
            float a = alpha * alpha * (3.0F - 2.0F * alpha);
            float k = a * (Fx.on(v, "Pulse", false) ? 0.78F + 0.22F * (float) Math.sin(time * 4.0) : 1.0F) * (1.0F + 0.7F * level);
            outline(edges, bounds, LO, HI, 0.002, lw, glow, colour, second, colouring.equals("Gradient"), look, speed, corners, cornerLen, k);
            // the faces: filled faintly; the one aimed at lit; the fill as it breaks and the cracks
            float fill = Fx.number(v, "Fill", 6.0F) / 100.0F;
            boolean rising = Fx.on(v, "Fill as it breaks", true) && level > 0.001F;
            boolean cracks = Fx.on(v, "Cracks", true) && level > 0.001F;
            if (fill > 0.0F || rising || cracks) {
                int bc = Fx.colour(v, "Breaking colour", 0xffffff);
                for (AABB b : boxes) {
                    double[] lo = map(b.minX, b.minY, b.minZ, -0.003);
                    double[] hi = map(b.maxX, b.maxY, b.maxZ, 0.003);
                    if (fill > 0.0F) {
                        Fx.box(lo[0], lo[1], lo[2], hi[0], hi[1], hi[2], fill, colour, a, 0);
                    }
                    if (rising || cracks) {
                        Fx.box(lo[0], lo[1], lo[2], hi[0], hi[1], hi[2], 0.0F, rising ? 0.2F : 0.0F, rising ? level : 0.0F, 1.1F, cracks ? level : 0.0F, 3.0F, bc, a, 0);
                    }
                }
            }
            boolean showFace = Fx.on(v, "Aimed face", true);
            float faceLight = Fx.number(v, "Face light", 16.0F) / 100.0F;
            if (showFace && faceLight > 0.0F) {
                faceBlend = Math.min(1.0F, faceBlend + dt / FACE_FADE);
                float blend = faceBlend * faceBlend * (3.0F - 2.0F * faceBlend);
                currentFace = faceQuads();
                drawFace(oldFace, faceLight, colour, 1.0F - blend);
                drawFace(currentFace, faceLight, colour, a * blend);
                faceAlpha = a;
            } else {
                oldFace.clear();
                currentFace.clear();
                faceBlend = 1.0F;
                faceAlpha = 0.0F;
            }
            if (faceBlend >= 1.0F) {
                oldFace.clear();
            }
        } else {
            oldFace.clear();
            currentFace.clear();
            faceBlend = 1.0F;
            faceAlpha = 0.0F;
        }
        // the flashes: broken, the outline going out bigger; placed, coming in from bigger
        for (int i = FLASHES.size() - 1; i >= 0; i--) {
            Flash f = FLASHES.get(i);
            f.age[0] += dt;
            float q = f.age[0] / FLASH;
            if (q >= 1.0F) {
                FLASHES.remove(i);
                continue;
            }
            float out = 1.0F - (1.0F - q) * (1.0F - q) * (1.0F - q);
            double grow = f.pop ? 0.22 * (1.0 - out) : 0.2 * out;
            double[] lo = new double[3];
            double[] hi = new double[3];
            double[] blo = {f.pos.getX() + f.bounds.minX, f.pos.getY() + f.bounds.minY, f.pos.getZ() + f.bounds.minZ};
            double[] bhi = {f.pos.getX() + f.bounds.maxX, f.pos.getY() + f.bounds.maxY, f.pos.getZ() + f.bounds.maxZ};
            for (int j = 0; j < 3; j++) {
                double m = (blo[j] + bhi[j]) * 0.5;
                lo[j] = blo[j] - (m - blo[j]) * grow;
                hi[j] = bhi[j] + (bhi[j] - m) * grow;
            }
            float k = (1.0F - q) * (1.0F - q) * (f.pop ? 1.2F : 1.6F);
            outline(f.edges, f.bounds, lo, hi, 0.002, lw * 1.3F, glow * 1.4F, colour, second, colouring.equals("Gradient"), look & ~(Fx.DASH | Fx.XRAY), speed, false, 0.0F, k);
            if (!f.pop) {
                Fx.box(lo[0], lo[1], lo[2], hi[0], hi[1], hi[2], 0.35F * (1.0F - q) * (1.0F - q), colour, 1.0F, 0);
            }
        }
        // a block placed: its pop when it is there
        if (placeFrames > 0 && lv != null) {
            placeFrames--;
            for (BlockPos p : new BlockPos[] {placeA, placeB}) {
                if (p != null && placeWhat != null && lv.getBlockState(p).is(placeWhat)) {
                    VoxelShape shape = lv.getBlockState(p).getShape(lv, p, CollisionContext.of(cam));
                    if (!shape.isEmpty() && Fx.on(v, "Pop when placed", true)) {
                        flash(new Flash(p.immutable(), edgesOf(shape, !exact), shape.bounds(), true, new float[1]));
                    }
                    placeFrames = 0;
                    break;
                }
            }
        }
        // sparks: flying, falling, fading
        for (int i = 0; i < sparks; i++) {
            SAGE[i] += dt;
            if (SAGE[i] >= SLIFE[i]) {
                sparks--;
                System.arraycopy(SP, sparks * 3, SP, i * 3, 3);
                System.arraycopy(SV, sparks * 3, SV, i * 3, 3);
                SAGE[i] = SAGE[sparks];
                SLIFE[i] = SLIFE[sparks];
                i--;
                continue;
            }
            double drag = Math.exp(-dt * 1.6);
            SV[i * 3] *= drag;
            SV[i * 3 + 1] = SV[i * 3 + 1] * drag - 14.0 * dt;
            SV[i * 3 + 2] *= drag;
            for (int j = 0; j < 3; j++) {
                SP[i * 3 + j] += SV[i * 3 + j] * dt;
            }
            float q = SAGE[i] / SLIFE[i];
            double x = SP[i * 3];
            double y = SP[i * 3 + 1];
            double z = SP[i * 3 + 2];
            Fx.seg(x - SV[i * 3] * 0.03, y - SV[i * 3 + 1] * 0.03, z - SV[i * 3 + 2] * 0.03, x, y, z, 0.004F, 2.0F, 1.2F, colour,
                    (float) Math.pow(1.0F - q, 1.5) * 1.4F, 0);
        }
    }

    /** A block-local point of the outlined shape where the outline is drawn now (its bounds mapped onto LO..HI), pushed
     *  out by `out`. */
    private static double[] map(double x, double y, double z, double out) {
        return map(x, y, z, bounds, LO, HI, out);
    }

    private static double[] map(double x, double y, double z, AABB b, double[] lo, double[] hi, double out) {
        double[] p = {x, y, z};
        double[] mn = {b.minX, b.minY, b.minZ};
        double[] sz = {b.getXsize(), b.getYsize(), b.getZsize()};
        double[] r = new double[3];
        for (int i = 0; i < 3; i++) {
            double t = sz[i] > 1e-6 ? (p[i] - mn[i]) / sz[i] : 0.5;
            r[i] = lo[i] + (hi[i] - lo[i]) * t + (t < 0.5 ? -Math.abs(out) : Math.abs(out));
        }
        return r;
    }

    /** The edges (block-local, within `b`) drawn onto lo..hi: whole or their corners. */
    private static void outline(List<double[]> e, AABB b, double[] lo, double[] hi, double out, float lw, float glow, int colour, int second, boolean gradient, int flags,
                                float speed, boolean corners, float cornerLen, float k) {
        double mx = (lo[0] + hi[0]) * 0.5;
        double mz = (lo[2] + hi[2]) * 0.5;
        double h = Math.max(hi[1] - lo[1], 1e-6);
        for (double[] q : e) {
            double[] a = map(q[0], q[1], q[2], b, lo, hi, out);
            double[] c = map(q[3], q[4], q[5], b, lo, hi, out);
            int ca = gradient ? mixColour(colour, second, (a[1] - lo[1]) / h) : colour;
            int cb = gradient ? mixColour(colour, second, (c[1] - lo[1]) / h) : second;
            if (!corners) {
                Fx.edge(a[0], a[1], a[2], c[0], c[1], c[2], WIDTH * lw, glow, ca, cb, mx, mz, speed, DASH_LENGTH, k, flags);
                continue;
            }
            double len = Math.sqrt((c[0] - a[0]) * (c[0] - a[0]) + (c[1] - a[1]) * (c[1] - a[1]) + (c[2] - a[2]) * (c[2] - a[2]));
            double t = Math.min(cornerLen, 0.5);
            if (len < 1e-6) {
                continue;
            }
            double[] p = {a[0] + (c[0] - a[0]) * t, a[1] + (c[1] - a[1]) * t, a[2] + (c[2] - a[2]) * t};
            double[] r = {c[0] + (a[0] - c[0]) * t, c[1] + (a[1] - c[1]) * t, c[2] + (a[2] - c[2]) * t};
            int cp = gradient ? mixColour(colour, second, (p[1] - lo[1]) / h) : second;
            int cr = gradient ? mixColour(colour, second, (r[1] - lo[1]) / h) : second;
            Fx.edge(a[0], a[1], a[2], p[0], p[1], p[2], WIDTH * lw, glow, ca, cp, mx, mz, speed, DASH_LENGTH, k, flags);
            Fx.edge(c[0], c[1], c[2], r[0], r[1], r[2], WIDTH * lw, glow, gradient ? cb : colour, cr, mx, mz, speed, DASH_LENGTH, k, flags);
        }
    }

    /** Keep what of the old and current face is visible now: it will fade out while the new target fades in. */
    private static void transitionFace() {
        float blend = faceBlend * faceBlend * (3.0F - 2.0F * faceBlend);
        List<double[]> faded = new ArrayList<>();
        for (double[] q : oldFace) {
            keepFacePart(faded, q, q[9] * (1.0F - blend));
        }
        for (double[] q : currentFace) {
            keepFacePart(faded, q, q[9] * faceAlpha * blend);
        }
        oldFace = faded;
        faceBlend = 0.0F;
    }

    private static void keepFacePart(List<double[]> faces, double[] q, double weight) {
        if (weight > 0.0001F) {
            double[] copy = q.clone();
            copy[9] = weight;
            faces.add(copy);
        }
    }

    /** The aimed face over every part of the block's actual voxel shape, not over its one bounding box. */
    private static List<double[]> faceQuads() {
        List<double[]> faces = new ArrayList<>();
        for (AABB b : boxes) {
            double[] lo = map(b.minX, b.minY, b.minZ, -0.004);
            double[] hi = map(b.maxX, b.maxY, b.maxZ, 0.004);
            double x0 = lo[0];
            double y0 = lo[1];
            double z0 = lo[2];
            double x1 = hi[0];
            double y1 = hi[1];
            double z1 = hi[2];
            double sx = x1 - x0;
            double sy = y1 - y0;
            double sz = z1 - z0;
            switch (face) {
                case UP -> faces.add(faceQuad(x0, y1, z0, sx, 0, 0, 0, 0, sz));
                case DOWN -> faces.add(faceQuad(x0, y0, z0, sx, 0, 0, 0, 0, sz));
                case NORTH -> faces.add(faceQuad(x0, y0, z0, sx, 0, 0, 0, sy, 0));
                case SOUTH -> faces.add(faceQuad(x0, y0, z1, sx, 0, 0, 0, sy, 0));
                case WEST -> faces.add(faceQuad(x0, y0, z0, 0, 0, sz, 0, sy, 0));
                case EAST -> faces.add(faceQuad(x1, y0, z0, 0, 0, sz, 0, sy, 0));
            }
        }
        return faces;
    }

    private static double[] faceQuad(double ox, double oy, double oz, double ux, double uy, double uz, double vx, double vy, double vz) {
        return new double[] {ox, oy, oz, ux, uy, uz, vx, vy, vz, 1.0};
    }

    private static void drawFace(List<double[]> faces, float light, int colour, float k) {
        for (double[] q : faces) {
            float alpha = k * (float) q[9];
            if (alpha > 0.0001F) {
                Fx.wall(q[0], q[1], q[2], q[3], q[4], q[5], q[6], q[7], q[8], 1.0F, 0.0F, light, 1e5F, 0.0F, colour, alpha, Fx.NO_FADE);
            }
        }
    }
}
