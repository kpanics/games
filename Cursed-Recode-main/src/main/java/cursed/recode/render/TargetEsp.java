package cursed.recode.render;

import com.mojang.blaze3d.pipeline.RenderTarget;
import com.mojang.blaze3d.vertex.PoseStack;
import com.mojang.renderpearl.api.textures.GpuTexture;
import java.util.HashMap;
import java.util.Iterator;
import java.util.LinkedHashMap;
import java.util.Map;
import net.minecraft.client.Minecraft;
import net.minecraft.client.model.Model;
import net.minecraft.client.model.geom.ModelPart;
import net.minecraft.client.multiplayer.ClientLevel;
import net.minecraft.client.player.LocalPlayer;
import net.minecraft.client.renderer.entity.state.EntityRenderState;
import net.minecraft.client.renderer.state.level.CameraRenderState;
import net.minecraft.util.Mth;
import net.minecraft.world.entity.Avatar;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.entity.NeutralMob;
import net.minecraft.world.entity.animal.golem.AbstractGolem;
import net.minecraft.world.entity.decoration.ArmorStand;
import net.minecraft.world.entity.monster.Enemy;
import net.minecraft.world.entity.npc.villager.AbstractVillager;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.entity.projectile.ProjectileUtil;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.EntityHitResult;
import net.minecraft.world.phys.HitResult;
import net.minecraft.world.phys.Vec3;
import org.joml.Matrix4f;
import org.joml.Vector3f;
import org.jspecify.annotations.Nullable;

/**
 * Target ESP, the eyes (native/src/engine/tesseract.rs has the marks and draws them): the one the
 * player aims at, found as the game finds what the crosshair is on (the blocks stop the look, the nearest creature on it
 * is the one), only as far as the setting looks, never anything invisible, never an armour stand, only the kinds
 * chosen. It and the ones let go of a moment ago go to the engine every frame with where their feet are, how large they
 * are, which way their body faces, what happened to them (a hit: the game's own hurt flash starting again; a death) and
 * their model's boxes as the game drew them this frame (each cube of each part shown, where its pose put it: they hide
 * what the tesseract draws behind the body).
 *
 * <p>Drawn after the level and the chams, under the hand, against the level's depth: a wall, or the target's own body,
 * hides what is behind it.
 */
public final class TargetEsp {
    // What a target is doing, what happened to it (engine/tesseract.rs AIMED, DYING, HIT).
    private static final int AIMED = 1;
    private static final int DYING = 2;
    private static final int HIT = 4;
    /** The most targets sent (engine/tesseract.rs MOST_TARGETS), the most boxes of each (MOST_BOXES), 16 numbers a box:
     *  its middle (from the camera; w 1 a box of the head), its three half sides as vectors. */
    private static final int MOST = 4;
    private static final int MOST_BOXES = 48;
    private static final int EACH = 16;
    /** How long one let go of is still sent (its mark folds away well before; a death goes on to its end). */
    private static final long KEPT_NANOS = 1_500_000_000L;

    /** The module's settings, asked of the menu four times a second. */
    private static final class Settings {
        boolean on;
        double distance = 24.0;
        boolean players = true;
        boolean hostile = true;
        boolean peaceful = true;
        boolean villagers = true;
    }

    /** What is kept of a target between frames: when it was last seen, its hurt flash, its boxes this frame. */
    private static final class Held {
        long seen;
        int hurt;
        final float[] boxes = new float[MOST_BOXES * EACH];
        int count;
        long boxed;
        /** The frame its boxes were moved to the eye's coordinates in. */
        long moved;
    }

    private static Settings settings = new Settings();
    private static long polled;
    private static final Map<Integer, Held> HELD = new LinkedHashMap<>();
    private static final int[] INTS = new int[MOST * 3];
    private static final float[] FLOATS = new float[MOST * 6];
    private static final float[][] BOXES = new float[MOST][];
    /** When the last target was sent: the engine goes on a while after it (a mark folding away, a death to its end). */
    private static long lastTarget;
    /** The target whose body is being submitted now (its boxes are taken), or none. */
    private static @Nullable Held capturing;
    private static long frame;
    private static final Vector3f TMP = new Vector3f();
    private static boolean broken;

    private TargetEsp() {
    }

    private static Settings settings() {
        long now = System.nanoTime();
        if (now - polled > 250_000_000L) {
            polled = now;
            settings = read(Native.guiCall("set\ttarget-esp"));
        }
        return settings;
    }

    private static Settings read(@Nullable String s) {
        Settings o = new Settings();
        if (s == null || s.isEmpty()) {
            return o;
        }
        String[] f = s.split("\t");
        o.on = f[0].equals("1");
        Map<String, String> v = new HashMap<>();
        for (int i = 1; i < f.length; i++) {
            int eq = f[i].indexOf('=');
            if (eq > 0) {
                v.put(f[i].substring(0, eq), f[i].substring(eq + 1));
            }
        }
        o.players = !"0".equals(v.get("Players"));
        o.hostile = !"0".equals(v.get("Hostile mobs"));
        o.peaceful = !"0".equals(v.get("Peaceful mobs"));
        o.villagers = !"0".equals(v.get("Villagers and golems"));
        try {
            o.distance = Double.parseDouble(v.getOrDefault("Distance", "24"));
        } catch (NumberFormatException e) {
            o.distance = 24.0;
        }
        return o;
    }

    /**
     * A body's submit begins (ChamsDispatcherMixin): when it is one of the targets (its kind, where it is drawn), the
     * models it submits until it is done are its boxes.
     */
    public static void bodyStart(EntityRenderState state) {
        capturing = null;
        if (broken || !settings.on || HELD.isEmpty()) {
            return;
        }
        Minecraft mc = Minecraft.getInstance();
        if (mc.level == null) {
            return;
        }
        float partial = mc.getDeltaTracker().getGameTimeDeltaPartialTick(false);
        for (Map.Entry<Integer, Held> e : HELD.entrySet()) {
            Entity entity = mc.level.getEntity(e.getKey());
            if (entity == null || entity.getType() != state.entityType) {
                continue;
            }
            Vec3 at = entity.getPosition(partial);
            if (Math.abs(at.x - state.x) < 0.05 && Math.abs(at.y - state.y) < 0.05 && Math.abs(at.z - state.z) < 0.05) {
                Held h = e.getValue();
                if (h.boxed != frame) {
                    h.count = 0;
                    h.boxed = frame;
                }
                capturing = h;
                return;
            }
        }
    }

    /** The body's submit is done. */
    public static void bodyDone() {
        capturing = null;
    }

    /** A model of the body being submitted (ChamsSubmitMixin), posed for its state: its boxes, when it is a target's. */
    @SuppressWarnings({"unchecked", "rawtypes"})
    public static void model(Model model, Object state, PoseStack poseStack) {
        Held h = capturing;
        if (h == null) {
            return;
        }
        try {
            ((Model) model).setupAnim(state);
        } catch (ClassCastException e) {
            return;
        }
        ModelPart root = model.root();
        if (!root.visible || root.skipDraw) {
            return;
        }
        root.visit(poseStack, (pose, path, index, cube) -> {
            if (h.count < MOST_BOXES && shows(root, path)) {
                add(h, pose.pose(), cube, path.startsWith("/head") || path.equals("head") || path.startsWith("head/"));
            }
        });
    }

    /** Whether the part at `path` ("/head/hat") shows: it and every part above it visible and drawn. */
    private static boolean shows(ModelPart root, String path) {
        ModelPart p = root;
        for (String name : path.split("/")) {
            if (name.isEmpty()) {
                continue;
            }
            if (!p.hasChild(name)) {
                return true;
            }
            p = p.getChild(name);
            if (!p.visible || p.skipDraw) {
                return false;
            }
        }
        return true;
    }

    private static void add(Held h, Matrix4f m, ModelPart.Cube c, boolean head) {
        // (a limb cut at its knee or elbow, Living Motion's: each half's own box, none of a cube all gone below)
        float[] b = cursed.recode.motion.Joints.box(c);
        if (b != null && b.length == 0) {
            return;
        }
        float x0 = b == null ? c.minX : b[0];
        float y0 = b == null ? c.minY : b[1];
        float z0 = b == null ? c.minZ : b[2];
        float x1 = b == null ? c.maxX : b[3];
        float y1 = b == null ? c.maxY : b[4];
        float z1 = b == null ? c.maxZ : b[5];
        int o = h.count * EACH;
        m.transformPosition((x0 + x1) / 32.0F, (y0 + y1) / 32.0F, (z0 + z1) / 32.0F, TMP);
        h.boxes[o] = TMP.x;
        h.boxes[o + 1] = TMP.y;
        h.boxes[o + 2] = TMP.z;
        h.boxes[o + 3] = head ? 1.0F : 0.0F;
        float[] half = {(x1 - x0) / 32.0F, (y1 - y0) / 32.0F, (z1 - z0) / 32.0F};
        for (int a = 0; a < 3; a++) {
            m.transformDirection(a == 0 ? half[0] : 0.0F, a == 1 ? half[1] : 0.0F, a == 2 ? half[2] : 0.0F, TMP);
            h.boxes[o + 4 + a * 4] = TMP.x;
            h.boxes[o + 5 + a * 4] = TMP.y;
            h.boxes[o + 6 + a * 4] = TMP.z;
            h.boxes[o + 7 + a * 4] = 0.0F;
        }
        h.count++;
    }

    /**
     * After the level and the chams, before the hand (GameRendererMixin): this frame's targets to the engine, over
     * `main`'s picture (`depth` the level's, when it was drawn into a smaller target).
     */
    public static void flush(RenderTarget main, @Nullable GpuTexture depth, CameraRenderState camera) {
        if (broken || !EngineFrame.active()) {
            return;
        }
        Minecraft mc = Minecraft.getInstance();
        ClientLevel level = mc.level;
        LocalPlayer player = mc.player;
        long thisFrame = frame;
        frame++;
        try {
            Settings set = settings();
            if (level == null || player == null || !camera.initialized) {
                HELD.clear();
                send(main, depth, 0);
                return;
            }
            float partial = mc.getDeltaTracker().getGameTimeDeltaPartialTick(false);
            long now = System.nanoTime();
            Entity aimed = set.on ? aimedAt(player, partial, set) : null;
            if (aimed != null) {
                HELD.computeIfAbsent(aimed.getId(), id -> new Held()).seen = now;
            }
            // from the eye: the engine casts the rays from there (NativeTerrain.eye)
            Vec3 c = NativeTerrain.eye(camera.pos);
            float ex = (float) (c.x - camera.pos.x);
            float ey = (float) (c.y - camera.pos.y);
            float ez = (float) (c.z - camera.pos.z);
            int n = 0;
            for (Iterator<Map.Entry<Integer, Held>> it = HELD.entrySet().iterator(); it.hasNext(); ) {
                Map.Entry<Integer, Held> e = it.next();
                Held held = e.getValue();
                Entity entity = level.getEntity(e.getKey());
                if (!(entity instanceof LivingEntity living) || now - held.seen > KEPT_NANOS || !set.on) {
                    it.remove();
                    continue;
                }
                int flags = 0;
                if (entity == aimed) {
                    flags |= AIMED;
                }
                if (living.isDeadOrDying()) {
                    flags |= DYING;
                    // its death goes on to its end, aimed at or not
                    held.seen = now;
                }
                // the game's own hurt flash starts again with every hit (it counts down from its length)
                if (living.hurtTime > held.hurt) {
                    flags |= HIT;
                }
                held.hurt = living.hurtTime;
                if (n == MOST) {
                    continue;
                }
                Vec3 at = living.getPosition(partial);
                INTS[n * 3] = living.getId();
                INTS[n * 3 + 1] = flags;
                // its boxes as drawn this frame (none when it was not drawn: the engine keeps the last)
                INTS[n * 3 + 2] = held.boxed == thisFrame ? held.count : 0;
                BOXES[n] = held.boxes;
                // its boxes came from the camera's place (the game's matrices for the body): from the eye's too
                if (held.boxed == thisFrame && held.moved != thisFrame) {
                    held.moved = thisFrame;
                    for (int k = 0; k < held.count; k++) {
                        held.boxes[k * EACH] -= ex;
                        held.boxes[k * EACH + 1] -= ey;
                        held.boxes[k * EACH + 2] -= ez;
                    }
                }
                FLOATS[n * 6] = (float) (at.x - c.x);
                FLOATS[n * 6 + 1] = (float) (at.y - c.y);
                FLOATS[n * 6 + 2] = (float) (at.z - c.z);
                FLOATS[n * 6 + 3] = living.getBbWidth();
                FLOATS[n * 6 + 4] = living.getBbHeight();
                FLOATS[n * 6 + 5] = Mth.rotLerp(partial, living.yBodyRotO, living.yBodyRot);
                n++;
            }
            send(main, depth, n);
        } catch (Throwable t) {
            broken = true;
            HELD.clear();
            System.err.println("[cursed] Target ESP is off for this launch: " + t);
        }
    }

    /** The targets to the engine, and none for five seconds after the last (its marks fold away, a death goes on). */
    private static void send(RenderTarget main, @Nullable GpuTexture depth, int n) {
        long now = System.nanoTime();
        if (n > 0) {
            lastTarget = now;
        } else if (now - lastTarget > 5_000_000_000L) {
            return;
        }
        EngineFrame.target(main.getColorTexture(), depth != null ? depth : main.getDepthTexture(), INTS, FLOATS, BOXES, n);
    }

    /** The creature the crosshair is on, as far as the setting looks: the game's own way of finding it. */
    private static @Nullable Entity aimedAt(LocalPlayer player, float partial, Settings set) {
        if (player.isSpectator()) {
            return null;
        }
        Vec3 eye = player.getEyePosition(partial);
        Vec3 look = player.getViewVector(partial);
        double far = set.distance;
        HitResult block = player.pick(far, partial, false);
        if (block.getType() != HitResult.Type.MISS) {
            far = block.getLocation().distanceTo(eye);
        }
        Vec3 end = eye.add(look.scale(far));
        AABB box = player.getBoundingBox().expandTowards(look.scale(far)).inflate(1.0);
        EntityHitResult hit = ProjectileUtil.getEntityHitResult(player, eye, end, box, e -> wanted(player, e, set), far * far);
        return hit != null ? hit.getEntity() : null;
    }

    private static boolean wanted(LocalPlayer player, Entity e, Settings set) {
        return e != player && e.isPickable() && !e.isSpectator() && !e.isInvisible() && e instanceof LivingEntity living && living.isAlive() && chosen(e, set);
    }

    /** Whether a creature is of a kind chosen (an armour stand never; a mannequin, a player's body, as a player). */
    private static boolean chosen(Entity e, Settings set) {
        if (e instanceof ArmorStand) {
            return false;
        }
        if (e instanceof Player || e instanceof Avatar) {
            return set.players;
        }
        if (e instanceof AbstractVillager || e instanceof AbstractGolem) {
            return set.villagers;
        }
        if (e instanceof Enemy) {
            return set.hostile;
        }
        if (e instanceof NeutralMob) {
            return set.peaceful || set.hostile;
        }
        return set.peaceful;
    }
}
