package cursed.recode.fx;

import java.util.ArrayDeque;
import java.util.HashMap;
import java.util.Iterator;
import java.util.Map;
import java.util.Random;
import net.minecraft.client.Minecraft;
import net.minecraft.client.multiplayer.ClientLevel;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.entity.projectile.arrow.AbstractArrow;
import net.minecraft.world.entity.projectile.throwableitemprojectile.ThrowableItemProjectile;
import net.minecraft.world.phys.Vec3;

/**
 * Trails (the menu's module "trails"): what is left behind on the way — a ribbon of light standing on it or lying along
 * it (brighter toward its edges, a flow running back along it, narrowing to its tail), a comet's tail with sparks shed
 * off its head, smoke curling up and thinning, or footprints of light going out one by one. Yours, the others', the
 * paths of pearls and arrows in flight, the tips of elytra's wings.
 */
final class Trails {
    private static final int MOST_POINTS = 240;
    private static Map<String, String> set = Map.of();
    private static long polled;
    private static final Map<Long, Track> TRACKS = new HashMap<>();
    private static final Bits BITS = new Bits(512);
    private static final Random RANDOM = new Random();
    private static double time;

    /** A trail: its points (x, y, z, when, how far along it), its footprints (x, y, z, way x, way z, when, left), how far
     *  walked since the last, what it is, when last seen. */
    private static final class Track {
        final ArrayDeque<double[]> points = new ArrayDeque<>();
        final ArrayDeque<double[]> steps = new ArrayDeque<>();
        double along;
        double walked;
        boolean left;
        /** Living Motion's feet on the ground last frame (1 right, 2 left). */
        int planted;
        double emit;
        double[] head = new double[3];
        int kind;
        double seen;
    }

    private static final int BODY = 0;
    private static final int THROWN = 1;
    private static final int WING = 2;

    private Trails() {
    }

    private static Map<String, String> settings() {
        long now = System.nanoTime();
        if (now - polled > 250_000_000L) {
            polled = now;
            set = Fx.settings("trails");
        }
        return set;
    }

    static void gone() {
        TRACKS.clear();
        BITS.clear();
    }

    static void draw(Minecraft mc, float partial, float dt) {
        Map<String, String> v = settings();
        ClientLevel level = mc.level;
        time += dt;
        if (level == null || mc.player == null || !Fx.on(v, "on", false)) {
            gone();
            return;
        }
        String style = Fx.choice(v, "Style", "Ribbon");
        float length = Math.max(0.2F, Fx.number(v, "Length", 1.8F));
        String height = Fx.choice(v, "Height", "Waist");
        boolean firstPerson = mc.options.getCameraType().isFirstPerson();
        // the bodies' trails
        for (Player p : level.players()) {
            boolean me = p == mc.player;
            if (me ? !Fx.on(v, "Yourself", true) || firstPerson && !Fx.on(v, "In first person", true) : !Fx.on(v, "Other players", false)) {
                continue;
            }
            if (p.isInvisible() || p.isSpectator() || p.distanceToSqr(mc.player) > 96.0 * 96.0) {
                continue;
            }
            Vec3 at = p.getPosition(partial);
            double up = switch (height) {
                case "Feet" -> 0.12;
                case "Head" -> p.getBbHeight() - 0.15;
                case "Whole body" -> p.getBbHeight() * 0.5;
                default -> p.getBbHeight() * 0.52;
            };
            Track t = track(p.getId(), BODY);
            if (style.equals("Footsteps")) {
                steps(t, p, at);
            } else {
                follow(t, at.x, at.y + up, at.z);
            }
            // the tips of elytra's wings
            if (Fx.on(v, "Elytra wingtips", true) && p.isFallFlying()) {
                double yaw = Math.toRadians(p.getViewYRot(partial));
                double rx = Math.cos(yaw);
                double rz = Math.sin(yaw);
                follow(track(p.getId() | 1L << 40, WING), at.x + rx * 0.95, at.y + 0.35, at.z + rz * 0.95);
                follow(track(p.getId() | 2L << 40, WING), at.x - rx * 0.95, at.y + 0.35, at.z - rz * 0.95);
            }
        }
        // the paths of what is thrown and shot, each kind as its switch says
        boolean pearls = Fx.on(v, "Pearls", true), arrows = Fx.on(v, "Arrows and tridents", true);
        boolean snow = Fx.on(v, "Snowballs and eggs", false), potions = Fx.on(v, "Potions and bottles", false);
        if (pearls || arrows || snow || potions) {
            for (Entity e : level.entitiesForRendering()) {
                boolean kind = e instanceof net.minecraft.world.entity.projectile.throwableitemprojectile.ThrownEnderpearl ? pearls
                        : e instanceof AbstractArrow ? arrows
                        : e instanceof net.minecraft.world.entity.projectile.throwableitemprojectile.Snowball
                                || e instanceof net.minecraft.world.entity.projectile.throwableitemprojectile.ThrownEgg ? snow
                        : e instanceof ThrowableItemProjectile && potions;
                if (kind && e.getDeltaMovement().lengthSqr() > 0.01 && e.distanceToSqr(mc.player) < 128.0 * 128.0) {
                    Vec3 at = e.getPosition(partial);
                    follow(track(e.getId() | 3L << 40, THROWN), at.x, at.y + e.getBbHeight() * 0.5, at.z);
                }
            }
        }
        // what is old goes; what is no longer seen goes once its trail has
        for (Iterator<Track> it = TRACKS.values().iterator(); it.hasNext(); ) {
            Track t = it.next();
            double life = t.kind == BODY ? length : t.kind == THROWN ? 1.4 : 1.1;
            while (!t.points.isEmpty() && time - t.points.peekFirst()[3] > life) {
                t.points.pollFirst();
            }
            while (!t.steps.isEmpty() && time - t.steps.peekFirst()[5] > life * 1.6) {
                t.steps.pollFirst();
            }
            if (t.points.isEmpty() && t.steps.isEmpty() && time - t.seen > 0.2) {
                it.remove();
                continue;
            }
            boolean live = time - t.seen < 0.05;
            if (t.kind == BODY) {
                body(t, v, style, height, (float) life, live, dt);
            } else {
                line(t, v, (float) life, live);
            }
        }
        BITS.step(level, dt, Fx.number(v, "Opacity", 100.0F) / 100.0F);
    }

    private static Track track(long id, int kind) {
        Track t = TRACKS.computeIfAbsent(id, k -> new Track());
        t.kind = kind;
        t.seen = time;
        return t;
    }

    /** A trail's point where it is now, when it went far enough from the last (its head follows it every frame). */
    private static void follow(Track t, double x, double y, double z) {
        t.head = new double[] {x, y, z};
        double[] last = t.points.peekLast();
        if (last != null) {
            double dx = x - last[0];
            double dy = y - last[1];
            double dz = z - last[2];
            double d = Math.sqrt(dx * dx + dy * dy + dz * dz);
            if (d < 0.06) {
                return;
            }
            if (d > 6.0) {
                // a jump across the world: a new trail
                t.points.clear();
            } else {
                t.along += d;
            }
        }
        t.points.addLast(new double[] {x, y, z, time, t.along});
        if (t.points.size() > MOST_POINTS) {
            t.points.pollFirst();
        }
    }

    /** Footprints: one each stride walked on the ground, left and right by turns, beside the way gone. */
    private static void steps(Track t, Player p, Vec3 at) {
        float[] m = cursed.recode.motion.LivingMotion.now(p.getId());
        if (m != null && m.length >= 28) {
            // Living Motion's own feet: a print where each is set down, as far ahead and aside as it is then
            double[] was = t.head;
            t.head = new double[] {at.x, at.y, at.z};
            double yaw = Math.toRadians(cursed.recode.motion.LivingMotion.bodyRot(p.getId()));
            double fx = -Math.sin(yaw);
            double fz = Math.cos(yaw);
            double gx = at.x - was[0];
            double gz = at.z - was[2];
            double gone = Math.sqrt(gx * gx + gz * gz);
            double wx = gone > 1e-4 && gone < 3.0 ? gx / gone : fx;
            double wz = gone > 1e-4 && gone < 3.0 ? gz / gone : fz;
            for (int k = 0; k < 2; k++) {
                boolean down = m[22 + k] > 0.5F;
                if (down && (t.planted & (1 << k)) == 0) {
                    double ahead = m[24 + k * 2];
                    double aside = m[25 + k * 2] + (k == 0 ? 0.12 : -0.12);
                    // the right of the way it faces: (-fz, fx)
                    t.steps.addLast(new double[] {at.x + fx * ahead - fz * aside, at.y, at.z + fz * ahead + fx * aside, wx, wz, time, k});
                    if (t.steps.size() > 64) {
                        t.steps.pollFirst();
                    }
                }
                t.planted = down ? t.planted | 1 << k : t.planted & ~(1 << k);
            }
            return;
        }
        double[] h = t.head;
        double dx = at.x - h[0];
        double dz = at.z - h[2];
        t.head = new double[] {at.x, at.y, at.z};
        double d = Math.sqrt(dx * dx + dz * dz);
        if (!p.onGround() || d > 3.0 || d < 1e-4) {
            return;
        }
        t.walked += d;
        if (t.walked < 0.62) {
            return;
        }
        t.walked = 0.0;
        double fx = dx / d;
        double fz = dz / d;
        t.left = !t.left;
        double side = t.left ? -0.13 : 0.13;
        // the right of the way gone: (-fz, fx)
        t.steps.addLast(new double[] {at.x - fz * side, at.y, at.z + fx * side, fx, fz, time, t.left ? 1 : 0});
        if (t.steps.size() > 64) {
            t.steps.pollFirst();
        }
    }

    private static int colourAt(Map<String, String> v, double q, double along) {
        int a = Fx.colour(v, "Colour", 0xffffff);
        return switch (Fx.choice(v, "Colouring", "One colour")) {
            case "Gradient" -> Bits.mix(a, Fx.colour(v, "Second colour", 0x6a6a6a), q);
            case "Shimmer" -> Bits.mix(a, Fx.colour(v, "Second colour", 0x6a6a6a), 0.5 + 0.5 * Math.sin(along * 2.0 - time * 3.0));
            default -> a;
        };
    }

    /** A body's trail drawn in its style. */
    private static void body(Track t, Map<String, String> v, String style, String height, float life, boolean live, float dt) {
        float width = Fx.number(v, "Trail width", 1.0F);
        float glow = Fx.number(v, "Glow", 1.0F);
        float opacity = Fx.number(v, "Opacity", 100.0F) / 100.0F;
        switch (style) {
            case "Footsteps" -> {
                for (double[] s : t.steps) {
                    float q = (float) ((time - s[5]) / (life * 1.6));
                    float k = opacity * (float) Math.pow(1.0F - q, 1.2) * Math.min(1.0F, (float) (time - s[5]) * 12.0F);
                    Fx.stamp(s[0], s[1], s[2], 0.2F * width, s[3], s[4], 0.5F, s[6] > 0.5, glow, colourAt(v, q, 0.0), k);
                }
            }
            case "Smoke" -> {
                // smoke from where they are while they move, curling up, swelling, thinning
                double[] last = t.points.peekLast();
                if (live && last != null && time - last[3] < 0.1) {
                    t.emit += dt * 24.0 * Fx.number(v, "Amount", 100.0F) / 100.0F;
                    while (t.emit >= 1.0) {
                        t.emit -= 1.0;
                        double[] h = t.head;
                        BITS.add(h[0] + RANDOM.nextGaussian() * 0.08, h[1] - 0.1 + RANDOM.nextGaussian() * 0.06, h[2] + RANDOM.nextGaussian() * 0.08,
                                RANDOM.nextGaussian() * 0.15, 0.25 + RANDOM.nextDouble() * 0.35, RANDOM.nextGaussian() * 0.15, life * (0.6F + RANDOM.nextFloat() * 0.4F),
                                (0.16F + RANDOM.nextFloat() * 0.12F) * width, colourAt(v, RANDOM.nextDouble(), RANDOM.nextDouble() * 6.0), Bits.PUFF, -0.15F, 1.4F);
                    }
                }
            }
            case "Comet" -> {
                double[] prev = null;
                for (double[] p : withHead(t, live)) {
                    if (prev != null) {
                        float q = (float) ((time - p[3]) / life);
                        float w = 0.03F * width * (float) Math.pow(1.0F - Math.min(q, 1.0F), 0.8);
                        Fx.seg(prev[0], prev[1], prev[2], p[0], p[1], p[2], w * 0.85F, 1.15F, glow, colourAt(v, q, p[4]), opacity * (1.0F - q) * 1.2F, Fx.THIN);
                    }
                    prev = p;
                }
                if (live && !t.points.isEmpty()) {
                    double[] h = t.head;
                    Fx.dot(h[0], h[1], h[2], 0.05F * width, 2.2F * glow, colourAt(v, 0.0, 0.0), opacity * 1.3F, 0);
                    // sparks shed off its head while it moves
                    double[] last = t.points.peekLast();
                    if (time - last[3] < 0.1) {
                        t.emit += dt * 30.0 * Fx.number(v, "Amount", 100.0F) / 100.0F;
                        while (t.emit >= 1.0) {
                            t.emit -= 1.0;
                            BITS.add(h[0], h[1], h[2], RANDOM.nextGaussian() * 0.6, RANDOM.nextGaussian() * 0.5 + 0.4, RANDOM.nextGaussian() * 0.6,
                                    0.35F + RANDOM.nextFloat() * 0.5F, (0.022F + RANDOM.nextFloat() * 0.02F) * width, colourAt(v, 0.2, 0.0), Bits.EMBER, 3.5F, 1.5F);
                        }
                    }
                }
            }
            default -> {
                // the ribbon: standing on the way gone, or lying along it
                boolean lying = Fx.choice(v, "Ribbon", "Standing").equals("Lying");
                float half = switch (height) {
                    case "Whole body" -> 0.85F;
                    case "Feet" -> 0.13F;
                    default -> 0.22F;
                } * width;
                boolean flow = Fx.on(v, "Flow", true);
                double[] prev = null;
                for (double[] p : withHead(t, live)) {
                    if (prev != null) {
                        float qa = (float) Math.min(1.0, (time - prev[3]) / life);
                        float qb = (float) Math.min(1.0, (time - p[3]) / life);
                        double dx = 0.0;
                        double dy = 1.0;
                        double dz = 0.0;
                        if (lying) {
                            double ax = p[0] - prev[0];
                            double az = p[2] - prev[2];
                            double l = Math.sqrt(ax * ax + az * az);
                            if (l < 1e-6) {
                                prev = p;
                                continue;
                            }
                            dx = -az / l;
                            dy = 0.0;
                            dz = ax / l;
                        }
                        float wa = half * (0.2F + 0.8F * (1.0F - qa));
                        float wb = half * (0.2F + 0.8F * (1.0F - qb));
                        float q = (qa + qb) * 0.5F;
                        Fx.ribbon(prev[0], prev[1], prev[2], wa, p[0], p[1], p[2], wb, dx, dy, dz, (float) prev[4], (float) p[4], flow, colourAt(v, q, p[4]),
                                opacity * (float) Math.pow(1.0F - q, 1.2) * 0.9F, 0);
                    }
                    prev = p;
                }
            }
        }
    }

    /** A thrown thing's path, or a wing's tip's: a thin line of light thinning to its tail. */
    private static void line(Track t, Map<String, String> v, float life, boolean live) {
        float glow = Fx.number(v, "Glow", 1.0F);
        float opacity = Fx.number(v, "Opacity", 100.0F) / 100.0F;
        double[] prev = null;
        for (double[] p : withHead(t, live)) {
            if (prev != null) {
                float q = (float) Math.min(1.0, (time - p[3]) / life);
                Fx.seg(prev[0], prev[1], prev[2], p[0], p[1], p[2], 0.012F * (1.0F - 0.6F * q), 1.0F, glow, colourAt(v, q, p[4]), opacity * (1.0F - q), Fx.THIN);
            }
            prev = p;
        }
    }

    /** A trail's points, and its head where it is now (when it is still there). */
    private static Iterable<double[]> withHead(Track t, boolean live) {
        if (!live || t.points.isEmpty()) {
            return t.points;
        }
        ArrayDeque<double[]> all = new ArrayDeque<>(t.points);
        double[] last = t.points.peekLast();
        double[] h = t.head;
        double dx = h[0] - last[0];
        double dy = h[1] - last[1];
        double dz = h[2] - last[2];
        all.addLast(new double[] {h[0], h[1], h[2], time, last[4] + Math.sqrt(dx * dx + dy * dy + dz * dz)});
        return all;
    }
}
