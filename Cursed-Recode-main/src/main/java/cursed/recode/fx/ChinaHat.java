package cursed.recode.fx;

import java.util.HashMap;
import java.util.Map;
import net.minecraft.client.Minecraft;
import net.minecraft.client.multiplayer.ClientLevel;
import net.minecraft.world.entity.Pose;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.phys.Vec3;

/**
 * China Hat (the menu's module "china-hat"): a hat over the head — sitting on it as the head is posed (it nods and
 * turns with it), swaying after it on a spring as a real one would when the head moves fast, turning slowly. Glass the
 * world bends through with a light going round it, a paper lantern lit from inside with tassels swinging off its brim,
 * cut crystal throwing glints off its faces, woven straw, or a halo floating over the head.
 */
final class ChinaHat {
    private static final int TASSELS = 8;
    private static Map<String, String> set = Map.of();
    private static long polled;
    private static final Map<Integer, Hat> HATS = new HashMap<>();
    private static double time;

    /** A hat: which way it stands now and how fast that turns (its sway), the head's last place and speed, how far it
     *  has turned, its tassels' ends (two links each: x y z of each joint, last frame's too), when last seen. */
    private static final class Hat {
        final double[] up = {0.0, 1.0, 0.0};
        final double[] spin = new double[3];
        double[] pivot;
        double[] speed = new double[3];
        double turn;
        double[] joints;
        double[] before;
        double seen;
    }

    private ChinaHat() {
    }

    private static Map<String, String> settings() {
        long now = System.nanoTime();
        if (now - polled > 250_000_000L) {
            polled = now;
            set = Fx.settings("china-hat");
        }
        return set;
    }

    static void gone() {
        HATS.clear();
    }

    static void draw(Minecraft mc, float partial, float dt) {
        Map<String, String> v = settings();
        ClientLevel level = mc.level;
        time += dt;
        if (level == null || mc.player == null || !Fx.on(v, "on", false) || dt <= 0.0F) {
            if (!Fx.on(v, "on", false)) {
                HATS.clear();
            }
            return;
        }
        boolean firstPerson = mc.options.getCameraType().isFirstPerson();
        for (Player p : level.players()) {
            boolean me = p == mc.player;
            if (me ? !Fx.on(v, "Yourself", true) || firstPerson && !Fx.on(v, "In first person", false) : !Fx.on(v, "Other players", false)) {
                continue;
            }
            if (p.isInvisible() || p.isSpectator() || p.distanceToSqr(mc.player) > 64.0 * 64.0) {
                continue;
            }
            Pose pose = p.getPose();
            if (pose == Pose.SWIMMING || pose == Pose.FALL_FLYING || pose == Pose.SLEEPING || pose == Pose.SPIN_ATTACK || pose == Pose.DYING) {
                continue;
            }
            Hat h = HATS.computeIfAbsent(p.getId(), k -> new Hat());
            wear(h, p, v, partial, dt);
        }
        HATS.values().removeIf(h -> time - h.seen > 1.0);
    }

    /** One hat this frame: the head's pose, the sway, the hat drawn in its style. */
    private static void wear(Hat h, Player p, Map<String, String> v, float partial, float dt) {
        h.seen = time;
        Vec3 at = p.getPosition(partial);
        // the head: its pivot at the neck, the way it looks
        double yaw = Math.toRadians(p.getViewYRot(partial));
        double pitch = Math.toRadians(p.getViewXRot(partial));
        double[] pivot = {at.x, at.y + p.getEyeHeight() - 0.12, at.z};
        // Living Motion moves the body under the head (down as the hips sit, leaning, rolling): the neck goes with it
        double[] moved = cursed.recode.motion.LivingMotion.moved(p.getId(), p.getEyeHeight() - 0.12);
        if (moved != null) {
            pivot[0] += moved[0];
            pivot[1] += moved[1];
            pivot[2] += moved[2];
        }
        double[] f = {-Math.sin(yaw) * Math.cos(pitch), -Math.sin(pitch), Math.cos(yaw) * Math.cos(pitch)};
        double[] right = norm(cross(new double[] {-Math.sin(yaw), 0.0, Math.cos(yaw)}, new double[] {0.0, 1.0, 0.0}));
        double[] headUp = norm(cross(right, f));
        // the sway: the hat's way up on a spring after the head's, pushed back as the head speeds up
        double[] speed = new double[3];
        double[] push = new double[3];
        if (h.pivot != null) {
            for (int i = 0; i < 3; i++) {
                speed[i] = (pivot[i] - h.pivot[i]) / dt;
                push[i] = Math.clamp((speed[i] - h.speed[i]) / dt, -60.0, 60.0);
            }
        }
        h.pivot = pivot;
        h.speed = speed;
        boolean sways = Fx.on(v, "Sways", true);
        if (!sways) {
            System.arraycopy(headUp, 0, h.up, 0, 3);
        } else {
            double stiff = 140.0;
            double damp = 11.0;
            for (int i = 0; i < 3; i++) {
                double target = headUp[i] - push[i] * 0.0035;
                h.spin[i] += ((target - h.up[i]) * stiff - h.spin[i] * damp) * dt;
                h.up[i] += h.spin[i] * dt;
            }
            double[] u = norm(h.up);
            // never far from the head's own way
            if (dot(u, headUp) < 0.8) {
                u = norm(new double[] {u[0] * 0.5 + headUp[0] * 0.5, u[1] * 0.5 + headUp[1] * 0.5, u[2] * 0.5 + headUp[2] * 0.5});
            }
            System.arraycopy(u, 0, h.up, 0, 3);
        }
        h.turn += dt * Fx.number(v, "Spin", 0.4F);
        double[] up = h.up;
        String style = Fx.choice(v, "Style", "Glass");
        float width = Fx.number(v, "Brim width", 1.0F);
        float height = Fx.number(v, "Height", 1.0F);
        float glow = Fx.number(v, "Glow", 1.0F);
        int colour = Fx.colour(v, "Colour", 0xffffff);
        float k = Fx.number(v, "Opacity", 100.0F) / 100.0F;
        // the head's top (its 8 pixels, 15/16 of a block's 16) and the brim just over it
        double top = 0.47;
        double[] base = {pivot[0] + up[0] * (top - 0.04), pivot[1] + up[1] * (top - 0.04), pivot[2] + up[2] * (top - 0.04)};
        float hgt = 0.3F * height;
        float rad = 0.62F * width;
        double[] tip = {base[0] + up[0] * hgt, base[1] + up[1] * hgt, base[2] + up[2] * hgt};
        switch (style) {
            case "Halo" -> {
                // a ring of light floating over the head, bobbing
                double lift = 0.17 + 0.025 * Math.sin(time * 2.2);
                double[] c = {pivot[0] + up[0] * (top + lift), pivot[1] + up[1] * (top + lift), pivot[2] + up[2] * (top + lift)};
                double[] e1 = norm(cross(up, Math.abs(up[1]) < 0.9 ? new double[] {0, 1, 0} : new double[] {1, 0, 0}));
                double[] e2 = cross(up, e1);
                double r = 0.3 * width;
                int n = 36;
                for (int i = 0; i < n; i++) {
                    double a0 = h.turn + i * Math.PI * 2.0 / n;
                    double a1 = h.turn + (i + 1) * Math.PI * 2.0 / n;
                    // a little brighter one way round, as a light going round it
                    float run = 0.75F + 0.35F * (float) Math.cos(a0 - time * 1.6);
                    Fx.seg(c[0] + (e1[0] * Math.cos(a0) + e2[0] * Math.sin(a0)) * r, c[1] + (e1[1] * Math.cos(a0) + e2[1] * Math.sin(a0)) * r,
                            c[2] + (e1[2] * Math.cos(a0) + e2[2] * Math.sin(a0)) * r, c[0] + (e1[0] * Math.cos(a1) + e2[0] * Math.sin(a1)) * r,
                            c[1] + (e1[1] * Math.cos(a1) + e2[1] * Math.sin(a1)) * r, c[2] + (e1[2] * Math.cos(a1) + e2[2] * Math.sin(a1)) * r, 0.009F * width, 1.0F,
                            glow * 0.8F, colour, k * run, Fx.THIN);
                }
            }
            case "Lantern" -> {
                Fx.cone(tip[0], tip[1], tip[2], -up[0], -up[1], -up[2], hgt * 1.25F, rad * 0.8F, 0.6F, 0.4F, 5.0F, 0.0F, (float) h.turn, glow, colour, k,
                        Fx.INNER);
                tassels(h, up, base, rad * 0.8F, glow, colour, k, dt);
            }
            case "Crystal" -> Fx.cone(tip[0], tip[1], tip[2], -up[0], -up[1], -up[2], hgt, rad, 0.14F, 1.7F, 0.0F, 8.0F, (float) h.turn, glow, colour, k, Fx.SWEEP);
            case "Straw" -> Fx.cone(tip[0], tip[1], tip[2], -up[0], -up[1], -up[2], hgt, rad, 1.0F, 0.15F, 11.0F, 0.0F, (float) h.turn, glow * 0.4F, colour, k, 0);
            default -> Fx.cone(tip[0], tip[1], tip[2], -up[0], -up[1], -up[2], hgt, rad, 0.2F, 1.0F, 0.0F, 0.0F, (float) h.turn, glow, colour, k, Fx.SWEEP);
        }
    }

    /** A lantern's tassels: two links each off its brim, swinging under their weight as the hat moves. */
    private static void tassels(Hat h, double[] up, double[] base, float rad, float glow, int colour, float k, float dt) {
        double[] e1 = norm(cross(up, Math.abs(up[1]) < 0.9 ? new double[] {0, 1, 0} : new double[] {1, 0, 0}));
        double[] e2 = cross(up, e1);
        double link = 0.11;
        if (h.joints == null) {
            h.joints = new double[TASSELS * 6];
            h.before = new double[TASSELS * 6];
            for (int i = 0; i < TASSELS; i++) {
                double[] a = anchor(i, h.turn, base, e1, e2, rad);
                for (int j = 0; j < 2; j++) {
                    h.joints[i * 6 + j * 3] = a[0];
                    h.joints[i * 6 + j * 3 + 1] = a[1] - link * (j + 1);
                    h.joints[i * 6 + j * 3 + 2] = a[2];
                }
            }
            System.arraycopy(h.joints, 0, h.before, 0, h.joints.length);
        }
        double g = 9.0 * dt * dt;
        for (int i = 0; i < TASSELS; i++) {
            double[] a = anchor(i, h.turn, base, e1, e2, rad);
            // each joint goes on as it went, falls, then is held a link from the one above
            for (int j = 0; j < 2; j++) {
                int o = i * 6 + j * 3;
                for (int c = 0; c < 3; c++) {
                    double now = h.joints[o + c];
                    double next = now + (now - h.before[o + c]) * 0.96 - (c == 1 ? g : 0.0);
                    h.before[o + c] = now;
                    h.joints[o + c] = next;
                }
            }
            double[] prev = a;
            for (int j = 0; j < 2; j++) {
                int o = i * 6 + j * 3;
                double dx = h.joints[o] - prev[0];
                double dy = h.joints[o + 1] - prev[1];
                double dz = h.joints[o + 2] - prev[2];
                double d = Math.max(Math.sqrt(dx * dx + dy * dy + dz * dz), 1e-6);
                h.joints[o] = prev[0] + dx / d * link;
                h.joints[o + 1] = prev[1] + dy / d * link;
                h.joints[o + 2] = prev[2] + dz / d * link;
                Fx.seg(prev[0], prev[1], prev[2], h.joints[o], h.joints[o + 1], h.joints[o + 2], 0.006F, 1.0F, glow * 0.6F, colour, k * 0.9F, Fx.THIN);
                prev = new double[] {h.joints[o], h.joints[o + 1], h.joints[o + 2]};
            }
            Fx.dot(prev[0], prev[1], prev[2], 0.014F, glow * 1.6F, colour, k, 0);
        }
    }

    private static double[] anchor(int i, double turn, double[] base, double[] e1, double[] e2, float rad) {
        double a = turn + i * Math.PI * 2.0 / TASSELS;
        return new double[] {base[0] + (e1[0] * Math.cos(a) + e2[0] * Math.sin(a)) * rad, base[1] + (e1[1] * Math.cos(a) + e2[1] * Math.sin(a)) * rad,
                base[2] + (e1[2] * Math.cos(a) + e2[2] * Math.sin(a)) * rad};
    }

    private static double[] cross(double[] a, double[] b) {
        return new double[] {a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]};
    }

    private static double dot(double[] a, double[] b) {
        return a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    }

    private static double[] norm(double[] a) {
        double l = Math.max(Math.sqrt(dot(a, a)), 1e-9);
        return new double[] {a[0] / l, a[1] / l, a[2] / l};
    }
}
