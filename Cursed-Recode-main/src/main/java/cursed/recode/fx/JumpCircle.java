package cursed.recode.fx;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.Random;
import net.minecraft.client.Minecraft;
import net.minecraft.client.multiplayer.ClientLevel;
import net.minecraft.core.BlockPos;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.phys.Vec3;
import net.minecraft.world.phys.shapes.VoxelShape;

/**
 * Jump Circle (the menu's module "jump-circle"): where you jump, a wave goes out over the ground — laid on the blocks
 * as they are, climbing a little up what it meets and stopped by walls and drops (looked for each of 16 ways when it
 * starts); a shock bending the air round its edge, ripples as on water, dust thrown up, or embers; dust of the block
 * you stood on, in its colour, flying, falling and bouncing on the blocks. Landing from a height makes one too, bigger
 * the higher; and (if set) the others' jumps.
 */
final class JumpCircle {
    private static final int MOST = 24;
    private static final int MOST_BITS = 384;
    /** How far a wave may reach (blocks: the reach looked for and its packing's most). */
    private static final float FURTHEST = 9.0F;
    private static Map<String, String> set = Map.of();
    private static long polled;
    private static final List<Circle> CIRCLES = new ArrayList<>();
    private static final Map<Integer, Feet> FEET = new HashMap<>();
    private static final Random RANDOM = new Random();
    /** The bits flying: dust and embers. */
    private static final Bits BITS = new Bits(MOST_BITS);
    /** CURSED_DEV_JUMP: the waves and the bits, in the log a few times a second. */
    private static final boolean LOG = cursed.recode.dev.Dev.on("CURSED_DEV_JUMP");
    private static long logged;

    /** A wave: where (its middle on the ground), how strong, its seed, how far it reaches each way, the ground's colour. */
    private record Circle(double x, double y, double z, float strength, float seed, float[] reach, int ground, float[] age) {
    }

    /** Someone's feet: on the ground last frame, where they last stood, how high they went since. */
    private static final class Feet {
        boolean on;
        double gx;
        double gy;
        double gz;
        double peak;
        long seen;
        /** Last frame's height and when (the speed up of one whose speed the server does not send). */
        double lastY = Double.NaN;
        long lastAt;
        double vy;
    }

    private JumpCircle() {
    }

    private static Map<String, String> settings() {
        long now = System.nanoTime();
        if (now - polled > 250_000_000L) {
            polled = now;
            set = Fx.settings("jump-circle");
        }
        return set;
    }

    static void gone() {
        CIRCLES.clear();
        FEET.clear();
        BITS.clear();
    }

    static void draw(Minecraft mc, float partial, float dt) {
        Map<String, String> v = settings();
        ClientLevel level = mc.level;
        if (level == null) {
            return;
        }
        boolean on = Fx.on(v, "on", true);
        if (on) {
            long now = System.nanoTime();
            if (mc.player != null) {
                feet(mc, level, v, mc.player, partial, now);
            }
            if (Fx.on(v, "Others' jumps", false)) {
                for (Player p : level.players()) {
                    if (p != mc.player && p.distanceToSqr(mc.player) < 48.0 * 48.0) {
                        feet(mc, level, v, p, partial, now);
                    }
                }
            }
            FEET.values().removeIf(f -> now - f.seen > 2_000_000_000L);
        }
        String style = Fx.choice(v, "Style", "Shockwave");
        float size = Fx.number(v, "Size", 1.0F);
        float life = Math.max(0.2F, Fx.number(v, "Duration", 1.0F));
        float lw = Fx.number(v, "Line width", 1.0F);
        float glow = Fx.number(v, "Glow", 1.0F);
        int colour = Fx.colour(v, "Colour", 0xffffff);
        boolean bend = Fx.on(v, "Bend the air", true);
        boolean tint = Fx.on(v, "Ring in the ground's colour", false);
        for (int i = CIRCLES.size() - 1; i >= 0; i--) {
            Circle c = CIRCLES.get(i);
            c.age[0] += dt;
            float q = c.age[0] / life;
            if (q >= 1.0F || !on) {
                CIRCLES.remove(i);
                continue;
            }
            float st = c.strength;
            float out = 1.0F - (1.0F - q) * (1.0F - q) * (1.0F - q);
            int rgb = tint ? c.ground : colour;
            float band = 0.9F + 0.4F * size;
            switch (style) {
                case "Ripples" -> {
                    float r = 2.2F * size * (float) Math.sqrt(st) * (float) Math.pow(q, 0.8);
                    Fx.wave(c.x, c.y, c.z, r, 0.028F * lw, band, 0.1F * (1.0F - q), glow, 1, c.seed, c.reach, bend, rgb, (float) Math.pow(1.0F - q, 1.2) * Math.min(st, 1.4F));
                }
                case "Dust" -> {
                    float r = 1.6F * size * (float) Math.sqrt(st) * out;
                    Fx.wave(c.x, c.y, c.z, r, 0.02F * lw, band, 0.15F * (1.0F - q), glow * 0.6F, 0, c.seed, c.reach, false, rgb, 0.45F * (1.0F - q) * (1.0F - q));
                }
                case "Embers" -> {
                    float r = 1.4F * size * (float) Math.sqrt(st) * out;
                    Fx.wave(c.x, c.y, c.z, r, 0.035F * lw, band, 0.22F * (1.0F - q), glow, 0, c.seed, c.reach, false, rgb, (float) Math.pow(1.0F - q, 1.3) * Math.min(st, 1.4F));
                }
                default -> {
                    float r = 1.8F * size * (float) Math.sqrt(st) * (0.08F + 0.92F * out);
                    Fx.wave(c.x, c.y, c.z, r, 0.05F * lw * (1.0F - 0.5F * q), band, 0.3F * (1.0F - q), glow, 0, c.seed, c.reach, bend, rgb,
                            (float) Math.pow(1.0F - q, 1.5) * 1.2F * Math.min(st, 1.4F));
                    // the glint where it struck
                    if (q < 0.25F) {
                        Fx.dot(c.x, c.y + 0.05, c.z, 0.03F * size, 2.0F * glow, rgb, (1.0F - q / 0.25F) * Math.min(st, 1.4F), 0);
                    }
                }
            }
        }
        if (LOG && System.nanoTime() - logged > 150_000_000L && (BITS.count() > 0 || !CIRCLES.isEmpty())) {
            logged = System.nanoTime();
            org.slf4j.LoggerFactory.getLogger("JumpCircle").info("[jump] circles {} bits {}", CIRCLES.size(), BITS.count());
        }
        // the bits: flying, falling, bouncing on the blocks, fading
        BITS.step(level, dt, 1.0F);
    }

    /** Someone's feet this frame: a jump (off the ground going up) or a landing (back on it from high enough) makes a
     *  wave. */
    private static void feet(Minecraft mc, ClientLevel level, Map<String, String> v, Player p, float partial, long now) {
        Vec3 at = p.getPosition(partial);
        boolean on = p.onGround();
        boolean free = !(p.isInWater() || p.isInLava() || p.getAbilities().flying || p.isFallFlying() || p.isPassenger() || p.onClimbable() || p.isSpectator());
        Feet f = FEET.get(p.getId());
        if (f == null) {
            f = new Feet();
            f.on = on;
            f.gx = at.x;
            f.gy = at.y;
            f.gz = at.z;
            f.peak = at.y;
            FEET.put(p.getId(), f);
        }
        // the speed up (blocks a tick): yours as the game has it; another's from how its place moved
        if (p == mc.player) {
            f.vy = p.getDeltaMovement().y;
        } else if (!Double.isNaN(f.lastY) && now > f.lastAt) {
            double sec = (now - f.lastAt) / 1e9;
            f.vy = f.vy * 0.5 + (at.y - f.lastY) / sec / 20.0 * 0.5;
        }
        f.lastY = at.y;
        f.lastAt = now;
        f.seen = now;
        if (free) {
            if (f.on && !on && f.vy > (p == mc.player ? 0.2 : 0.12) && Fx.on(v, "On jumping", true)) {
                float st = (float) Math.clamp(f.vy / 0.42, 0.6, 2.0);
                start(mc, level, v, f.gx, f.gy, f.gz, st);
            }
            if (!f.on && on && Fx.on(v, "On landing", true)) {
                double fell = f.peak - at.y;
                float from = Fx.number(v, "Landing from", 2.5F);
                if (fell >= from) {
                    float st = Fx.on(v, "Bigger from higher", true) ? (float) Math.clamp(0.5 + (fell - from) / 8.0, 0.5, 2.0) : 1.0F;
                    start(mc, level, v, at.x, at.y, at.z, st);
                }
            }
        }
        if (on) {
            f.gx = at.x;
            f.gy = at.y;
            f.gz = at.z;
            f.peak = at.y;
        } else {
            f.peak = Math.max(f.peak, at.y);
        }
        f.on = on;
    }

    /** A wave from (x, y, z): how far it may go each way, the ground's colour, its dust. */
    private static void start(Minecraft mc, ClientLevel level, Map<String, String> v, double x, double y, double z, float st) {
        float size = Fx.number(v, "Size", 1.0F);
        float reachMost = Math.min(FURTHEST, 2.4F * size * (float) Math.sqrt(st) + 0.5F);
        float[] reach = Fx.on(v, "Stops at walls and drops", true) ? reach(level, x, y, z, reachMost) : null;
        BlockPos under = BlockPos.containing(x, y - 0.2, z);
        BlockState ground = level.getBlockState(under);
        int gc = ground.isAir() ? 0 : ground.getMapColor(level, under).col;
        if (gc == 0) {
            gc = 0xbdbdbd;
        }
        if (CIRCLES.size() >= MOST) {
            CIRCLES.remove(0);
        }
        CIRCLES.add(new Circle(x, y, z, st, RANDOM.nextFloat() * 100.0F, reach, gc, new float[1]));
        String style = Fx.choice(v, "Style", "Shockwave");
        // dust of the ground thrown up; embers for their style
        if (Fx.on(v, "Dust", true)) {
            int n = Math.round((style.equals("Dust") ? 60 : style.equals("Ripples") ? 9 : 24) * Fx.number(v, "Dust amount", 100.0F) / 100.0F * (float) Math.sqrt(st));
            int dc = Fx.on(v, "Dust in the ground's colour", true) ? gc : Fx.colour(v, "Colour", 0xffffff);
            // a puff is the ground's dust in the air: paler than the ground
            int pc = Bits.mix(dc, 0xffffff, 0.6);
            int grain = Bits.mix(dc, 0xffffff, 0.25);
            for (int i = 0; i < n; i++) {
                double a = RANDOM.nextDouble() * Math.PI * 2.0;
                double r0 = 0.15 + RANDOM.nextDouble() * 0.35;
                if (i % 3 == 2) {
                    // a puff: rolling out low along the ground, swelling, thinning
                    double sp = (1.0 + RANDOM.nextDouble() * 1.8) * Math.sqrt(st) * size;
                    BITS.add(x + Math.cos(a) * r0, y + 0.12, z + Math.sin(a) * r0, Math.cos(a) * sp, 0.3 + RANDOM.nextDouble() * 0.6, Math.sin(a) * sp,
                            0.7F + RANDOM.nextFloat() * 0.6F, (0.14F + RANDOM.nextFloat() * 0.12F) * size, pc, Bits.PUFF, 0.4F, 3.2F);
                    continue;
                }
                double sp = (1.5 + RANDOM.nextDouble() * 2.6) * Math.sqrt(st) * size;
                BITS.add(x + Math.cos(a) * r0, y + 0.04, z + Math.sin(a) * r0, Math.cos(a) * sp, (1.6 + RANDOM.nextDouble() * 3.0) * Math.sqrt(st), Math.sin(a) * sp,
                        0.6F + RANDOM.nextFloat() * 0.9F, (0.04F + RANDOM.nextFloat() * 0.04F) * size, grain, Bits.GRAIN, 9.0F, 1.8F);
            }
        }
        if (style.equals("Embers")) {
            int ec = Fx.colour(v, "Colour", 0xffffff);
            for (int i = 0; i < 26; i++) {
                double a = RANDOM.nextDouble() * Math.PI * 2.0;
                double sp = (0.8 + RANDOM.nextDouble() * 2.2) * size;
                BITS.add(x + Math.cos(a) * 0.3, y + 0.05, z + Math.sin(a) * 0.3, Math.cos(a) * sp, (2.0 + RANDOM.nextDouble() * 2.6) * Math.sqrt(st), Math.sin(a) * sp,
                        0.8F + RANDOM.nextFloat() * 0.9F, (0.03F + RANDOM.nextFloat() * 0.025F) * size, ec, Bits.EMBER, 5.5F, 1.2F);
            }
        }
    }

    /** How far a wave from (x, y, z) goes each of 16 ways (round from +x toward +z, from -180°) before a wall (a block
     *  over half a block above its ground) or a drop (no floor three blocks down) stops it. */
    private static float[] reach(ClientLevel level, double x, double y, double z, float most) {
        float[] r = new float[16];
        BlockPos.MutableBlockPos m = new BlockPos.MutableBlockPos();
        for (int i = 0; i < 16; i++) {
            double a = -Math.PI + i * Math.PI * 2.0 / 16.0;
            double dx = Math.cos(a);
            double dz = Math.sin(a);
            float got = most;
            for (float d = 0.3F; d <= most; d += 0.25F) {
                double px = x + dx * d;
                double pz = z + dz * d;
                // a wall: what is at the feet's height standing more than half a block over them, or the block over it
                m.set(px, y + 0.05, pz);
                VoxelShape s = level.getBlockState(m).getCollisionShape(level, m);
                boolean wall = !s.isEmpty() && m.getY() + s.max(net.minecraft.core.Direction.Axis.Y) > y + 0.55;
                m.set(px, y + 1.05, pz);
                wall |= !level.getBlockState(m).getCollisionShape(level, m).isEmpty();
                if (wall) {
                    got = d + 0.08F;
                    break;
                }
                // a drop: no floor three blocks down
                boolean floor = false;
                for (int k = 0; k < 3 && !floor; k++) {
                    m.set(px, y - 0.05 - k, pz);
                    floor = !level.getBlockState(m).getCollisionShape(level, m).isEmpty();
                }
                if (!floor) {
                    got = d + 0.25F;
                    break;
                }
            }
            r[i] = got;
        }
        return r;
    }
}
