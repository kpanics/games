package cursed.recode.motion;

import com.mojang.blaze3d.vertex.PoseStack;
import com.mojang.math.Axis;
import cursed.recode.render.Native;
import java.util.HashMap;
import java.util.Map;
import net.minecraft.client.Minecraft;
import net.minecraft.client.model.HumanoidModel;
import net.minecraft.client.renderer.entity.state.AvatarRenderState;
import net.minecraft.client.renderer.entity.state.HumanoidRenderState;
import net.minecraft.util.Mth;
import net.minecraft.world.entity.Avatar;
import net.minecraft.world.entity.HumanoidArm;
import net.minecraft.world.entity.Pose;

/**
 * Living Motion (the menu's "living-motion", Visual · Players): the players' walk as native motion.rs works it out from
 * how each one moves — the pose taken as the game gathers a player's state for the frame (extract), put on the model's
 * limbs where the game puts its own walk (the held items', the blows', the crouch's after it: they stay the game's), the
 * shoulders' twist and the head's keeping level after the game is done (body), and the whole moved under it: down as
 * the hips sit, leaning, rolling, shifted (root). A screen's picture of a player (the inventory's, the HUD's: the
 * game draws them at the tick's end) walks on a gait of its own, so its turning to face the mouse is not the world's.
 */
public final class LivingMotion {
    private static final String MODULE = "living-motion";
    private static final String[] STYLES = {"Natural", "Nimble", "Heavy"};
    private static final float[] IN = new float[13];
    private static Map<String, String> set = Map.of();
    private static long setAt;
    /** CURSED_DEV_MOTION: a player's numbers once a second, in the log. */
    private static final boolean LOG = cursed.recode.dev.Dev.on("CURSED_DEV_MOTION");
    private static long logAt;

    private LivingMotion() {
    }

    /** As the game gathers a player's state: its pose this frame, or none (off, not for it, or the game's own pose). */
    public static void extract(Avatar entity, AvatarRenderState state, float partialTicks) {
        MotionPose holder = (MotionPose) state;
        holder.cursed$motion(null);
        poll();
        if (!"1".equals(set.get("on"))) {
            return;
        }
        Minecraft mc = Minecraft.getInstance();
        if ("Just me".equals(set.get("Players")) && entity != mc.player) {
            return;
        }
        int flags = 0;
        if (entity.onGround()) {
            flags |= 1;
        }
        if (state.isCrouching) {
            flags |= 2;
        }
        if (entity.isSprinting()) {
            flags |= 4;
        }
        if (state.swimAmount > 0.0F || state.isFallFlying || state.isVisuallySwimming || state.isPassenger || state.isAutoSpinAttack || state.deathTime > 0.0F
                || state.hasPose(Pose.SLEEPING) || entity.isSpectator()) {
            flags |= 8;
        }
        boolean picture = partialTicks == 1.0F;
        // the key (its id twice, +1 for a picture) in halves: a float holds whole numbers only to 16.7 million
        long key = entity.getId() * 2L + (picture ? 1 : 0);
        IN[0] = key & 0xFFFF;
        IN[12] = key >>> 16;
        IN[1] = (float) (entity.getX() - entity.xo);
        IN[2] = (float) (entity.getZ() - entity.zo);
        IN[3] = (float) (entity.getY() - entity.yo);
        IN[4] = state.bodyRot;
        IN[5] = flags;
        IN[6] = Math.max(0, java.util.Arrays.asList(STYLES).indexOf(set.getOrDefault("Style", "Natural")));
        IN[7] = number("Bounce", 100.0F) / 100.0F;
        IN[8] = number("Lean", 100.0F) / 100.0F;
        IN[9] = number("Arm swing", 100.0F) / 100.0F;
        IN[10] = number("Stride", 100.0F) / 100.0F;
        IN[11] = "0".equals(set.get("Breathing")) ? 0.0F : 1.0F;
        float[] out = new float[28];
        Native.motionPose(IN, out);
        if (LOG && entity != mc.player && System.nanoTime() - logAt > 500_000_000L) {
            logAt = System.nanoTime();
            cursed.recode.CursedRecode.LOGGER.info("[living motion] {} #{}: body {} (head {}), moved x {} z {} y {}, flags {}, down {} lean {} roll {}",
                    entity.getType().toShortString(), entity.getId(), state.bodyRot, state.yRot, IN[1], IN[2], IN[3], flags, out[14], out[15], out[16]);
        }
        if (out[0] > 0.5F) {
            holder.cursed$motion(out);
            if (!picture) {
                LAST.put(entity.getId(), new Last(out, state.bodyRot, System.nanoTime()));
            }
        }
    }

    /** Each player's pose this frame (the world's, not a screen's picture), for what goes on the body: China Hat, the
     *  footprints. */
    private record Last(float[] pose, float bodyRot, long at) {
    }

    private static final Map<Integer, Last> LAST = new HashMap<>();

    /** A player's pose now, or null (Living Motion off for it, or not drawn lately). */
    public static float @org.jspecify.annotations.Nullable [] now(int id) {
        Last l = LAST.get(id);
        if (l == null || System.nanoTime() - l.at > 250_000_000L) {
            if (l != null) {
                LAST.remove(id);
            }
            return null;
        }
        return l.pose;
    }

    /** Where a point on the body `h` blocks over the feet has gone by the pose (down, shifted, leaning, rolled), in the
     *  world (blocks to add), or null (not posed). */
    public static double @org.jspecify.annotations.Nullable [] moved(int id, double h) {
        Last l = LAST.get(id);
        float[] p = now(id);
        if (p == null) {
            return null;
        }
        double lean = p[15];
        double roll = p[16];
        // under the model at its feet (its right +x, forward -z, up +y): shifted and down, leaning, rolled
        double x = p[17] + h * Math.sin(roll);
        double y = p[14] + h * (Math.cos(roll) * Math.cos(lean) - 1.0);
        double z = -h * Math.cos(roll) * Math.sin(lean);
        double a = Math.toRadians(180.0 - l.bodyRot);
        return new double[] {x * Math.cos(a) + z * Math.sin(a), y, -x * Math.sin(a) + z * Math.cos(a)};
    }

    /** The body's yaw the pose was taken at. */
    public static float bodyRot(int id) {
        Last l = LAST.get(id);
        return l == null ? 0.0F : l.bodyRot;
    }

    /** Where the game puts its walk on the limbs: ours instead, the knees and the elbows bent (the model cut at them the
     *  first time, Joints). */
    public static void limbs(HumanoidModel<?> m, float[] p) {
        m.rightLeg.xRot = p[1];
        m.rightLeg.zRot = p[2];
        m.leftLeg.xRot = p[3];
        m.leftLeg.zRot = p[4];
        m.rightArm.xRot = p[5];
        m.rightArm.zRot = p[6];
        m.leftArm.xRot = p[7];
        m.leftArm.zRot = p[8];
        Joints j = Joints.of(m);
        j.rightKnee.xRot = p[18];
        j.leftKnee.xRot = p[19];
        j.rightElbow.xRot = -p[20];
        j.leftElbow.xRot = -p[21];
    }

    /** After the game is done: the shoulders' twist (the arms go with it, as the game's swing turns them), the chest,
     *  the head kept as it was. */
    public static void body(HumanoidModel<?> m, HumanoidRenderState state, float[] p) {
        // an arm the game poses itself (a bow drawn, a shield up, a blow, a spyglass…) straight at the elbow, as the game
        // has it
        Joints j = Joints.ifCut(m);
        if (j != null) {
            boolean swinging = state.swingAnimation > 0.0F;
            if (!free(state.rightArmPose) || swinging && state.mainArm == HumanoidArm.RIGHT) {
                j.rightElbow.xRot = 0.0F;
            }
            if (!free(state.leftArmPose) || swinging && state.mainArm == HumanoidArm.LEFT) {
                j.leftElbow.xRot = 0.0F;
            }
        }
        float twist = p[9];
        if (twist != 0.0F) {
            float total = m.body.yRot + twist;
            m.body.yRot = total;
            m.rightArm.z = Mth.sin(total) * 5.0F;
            m.rightArm.x = -Mth.cos(total) * 5.0F;
            m.leftArm.z = -Mth.sin(total) * 5.0F;
            m.leftArm.x = Mth.cos(total) * 5.0F;
            m.rightArm.yRot += twist;
            m.leftArm.yRot += twist;
        }
        m.body.xRot += p[10];
        m.head.xRot += p[11];
        m.head.yRot += p[12];
        m.head.zRot += p[13];
    }

    private static boolean free(HumanoidModel.ArmPose pose) {
        return pose == HumanoidModel.ArmPose.EMPTY || pose == HumanoidModel.ArmPose.ITEM;
    }

    /** Under the model, at its feet (its right +x, forward -z, up +y): down as the hips sit, shifted, leaning, rolled. */
    public static void root(float[] p, PoseStack pose) {
        pose.translate(p[17], p[14], 0.0F);
        pose.rotate(Axis.XP, -p[15]);
        pose.rotate(Axis.ZP, -p[16]);
    }

    private static float number(String label, float fallback) {
        try {
            return Float.parseFloat(set.getOrDefault(label, String.valueOf(fallback)));
        } catch (NumberFormatException e) {
            return fallback;
        }
    }

    private static void poll() {
        long now = System.nanoTime();
        if (now - setAt < 250_000_000L) {
            return;
        }
        setAt = now;
        Map<String, String> v = new HashMap<>();
        String s = Native.guiCall("set\t" + MODULE);
        if (s != null && !s.isEmpty()) {
            String[] f = s.split("\t");
            v.put("on", f[0]);
            for (int i = 1; i < f.length; i++) {
                int eq = f[i].indexOf('=');
                if (eq > 0) {
                    v.put(f[i].substring(0, eq), f[i].substring(eq + 1));
                }
            }
        }
        set = v;
    }
}
