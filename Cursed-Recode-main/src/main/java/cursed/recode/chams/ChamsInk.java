package cursed.recode.chams;

import com.mojang.blaze3d.pipeline.RenderTarget;
import com.mojang.renderpearl.api.textures.GpuTexture;
import cursed.recode.render.EngineFrame;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import net.minecraft.client.Minecraft;
import net.minecraft.client.renderer.entity.state.AvatarRenderState;
import net.minecraft.client.renderer.entity.state.LivingEntityRenderState;
import net.minecraft.client.renderer.state.level.CameraRenderState;
import net.minecraft.util.Mth;
import net.minecraft.world.phys.Vec3;
import org.joml.Vector4f;
import org.jspecify.annotations.Nullable;

/**
 * Chams' ink in the air (native/src/engine/ink.rs): the bodies in the look "Ink" let ink into the air as into still
 * water, simulated. Each body this frame: its feet and height, its velocity (from where it was the frame before), its
 * hands (where its arms swing them, how fast); after the level they go to the engine, which steps the water, draws the
 * ink and lays it over the level's picture, under the hand and the GUI. The water goes on half a minute after the last
 * body (its ink fades out). Of the bodies in the look (Chams.wants: players, mobs…) one's own goes first (the water's
 * grid goes round it), then the nearest others within the grid's reach of it, as many as the player lets have smoke —
 * not the first eight drawn (in a crowd one's own was left out, and its ink stopped). A mob has no hands to let ink
 * go from, only its trunk, as wide as it is.
 */
public final class ChamsInk {
    /** The most bodies that let ink go at once (engine/ink.rs MOST_BODIES). */
    private static final int MOST = 8;
    /**
     * Each body: feet (from the camera), height, velocity, how much, its two hands (from the camera, their speed); where
     * its feet and hands were the frame before (from the camera now: the ink is let go all along the way between); how
     * much its hands let go (1 a player's, 0 a mob's), how wide it is against a player.
     */
    private static final int EACH = 27;
    /** Where in each body a place starts (its feet, its hands, where its feet and hands were the frame before). */
    private static final int[] POINTS = {0, 8, 12, 16, 19, 22};
    private static final float[] BODIES = new float[MOST * EACH];
    /** This frame's bodies before the choice (how far each is from the camera, whether it is one's own). */
    private static final int CANDIDATES = 64;
    private static final float[] CAND = new float[CANDIDATES * EACH];
    private static final double[] FAR = new double[CANDIDATES];
    private static final boolean[] OWN = new boolean[CANDIDATES];
    private static final Integer[] ORDER = new Integer[CANDIDATES];
    /** How far from the grid's body another counts (blocks, across and up: the grid is 8 x 4 x 8 round it). */
    private static final double REACH = 6.0;
    private static final double REACH_UP = 4.0;
    private static int count;
    private static long lastBody;
    /**
     * Where a body was the frame before: a player's by its id; a mob's (its picture's state has no id) the nearest of its
     * kind last frame, within a stride.
     */
    private static final Map<Integer, Was> WAS = new HashMap<>();
    private static List<Was> mobs = new ArrayList<>();
    private static List<Was> mobsNow = new ArrayList<>();

    private ChamsInk() {
    }

    /** Where a body was the frame before, and its hands. */
    private static final class Was {
        double x;
        double y;
        double z;
        double[] hands = new double[6];
        long at;
        @Nullable Object kind;
    }

    private static @Nullable Was was(LivingEntityRenderState s) {
        if (s instanceof AvatarRenderState a) {
            return WAS.get(a.id);
        }
        Was best = null;
        double nearest = 0.36;
        for (Was w : mobs) {
            double d = (w.x - s.x) * (w.x - s.x) + (w.y - s.y) * (w.y - s.y) + (w.z - s.z) * (w.z - s.z);
            if (w.kind == s.entityType && d < nearest) {
                nearest = d;
                best = w;
            }
        }
        return best;
    }

    /** A body in the look this frame (ChamsHooks.bodyDone). */
    static void body(LivingEntityRenderState s, CameraRenderState camera) {
        Minecraft mc = Minecraft.getInstance();
        boolean own = mc.player != null && s instanceof AvatarRenderState a && a.id == mc.player.getId();
        if (count >= CANDIDATES && !own) {
            return;
        }
        long now = System.nanoTime();
        Vec3 c = camera.pos;
        float h = Math.max(0.6F, s.boundingBoxHeight);
        // its hands: from the shoulders, swung as its legs walk (the game's own walk animation), on its facing
        float yaw = s.bodyRot * (float) (Math.PI / 180.0);
        double fx = -Mth.sin(yaw);
        double fz = Mth.cos(yaw);
        double sx = -fz;
        double sz = fx;
        float scale = h / 1.8F;
        double shoulder = h * 0.72;
        double arm = 0.62 * scale;
        double side = 0.33 * scale;
        double[] hands = new double[6];
        for (int k = 0; k < 2; k++) {
            float swing = Mth.cos(s.walkAnimationPos * 0.6662F + (k == 0 ? (float) Math.PI : 0.0F)) * s.walkAnimationSpeed;
            double sign = k == 0 ? 1.0 : -1.0;
            hands[k * 3] = s.x + sx * side * sign + fx * Math.sin(swing) * arm;
            hands[k * 3 + 1] = s.y + shoulder - Math.cos(swing) * arm;
            hands[k * 3 + 2] = s.z + sz * side * sign + fz * Math.sin(swing) * arm;
        }
        Was w = was(s);
        float dt = w == null ? 0.0F : Math.min(0.2F, (now - w.at) / 1.0E9F);
        float vx = 0.0F;
        float vy = 0.0F;
        float vz = 0.0F;
        float[] handSpeed = new float[2];
        if (w != null && dt > 1.0E-4F) {
            vx = (float) ((s.x - w.x) / dt);
            vy = (float) ((s.y - w.y) / dt);
            vz = (float) ((s.z - w.z) / dt);
            for (int k = 0; k < 2; k++) {
                double dx = hands[k * 3] - w.hands[k * 3];
                double dy = hands[k * 3 + 1] - w.hands[k * 3 + 1];
                double dz = hands[k * 3 + 2] - w.hands[k * 3 + 2];
                handSpeed[k] = (float) (Math.sqrt(dx * dx + dy * dy + dz * dz) / dt);
            }
        }
        // a jump (a teleport, the pearl) stirs nothing, and nothing is let go along it
        float speed = (float) Math.sqrt(vx * vx + vy * vy + vz * vz);
        float keep = speed > 30.0F ? 0.0F : 1.0F;
        double[] before = new double[9];
        boolean was = w != null && keep > 0.0F;
        before[0] = was ? w.x : s.x;
        before[1] = was ? w.y : s.y;
        before[2] = was ? w.z : s.z;
        for (int i = 0; i < 6; i++) {
            before[3 + i] = was ? w.hands[i] : hands[i];
        }
        if (s instanceof AvatarRenderState a) {
            if (w == null) {
                w = new Was();
                WAS.put(a.id, w);
            }
        } else {
            w = new Was();
            w.kind = s.entityType;
            mobsNow.add(w);
        }
        w.x = s.x;
        w.y = s.y;
        w.z = s.z;
        w.hands = hands;
        w.at = now;
        // the candidates full: one's own takes the last place
        int slot = Math.min(count, CANDIDATES - 1);
        int o = slot * EACH;
        CAND[o] = (float) (s.x - c.x);
        CAND[o + 1] = (float) (s.y - c.y);
        CAND[o + 2] = (float) (s.z - c.z);
        CAND[o + 3] = h;
        CAND[o + 4] = vx * keep;
        CAND[o + 5] = vy * keep;
        CAND[o + 6] = vz * keep;
        CAND[o + 7] = 1.0F;
        for (int k = 0; k < 2; k++) {
            CAND[o + 8 + k * 4] = (float) (hands[k * 3] - c.x);
            CAND[o + 9 + k * 4] = (float) (hands[k * 3 + 1] - c.y);
            CAND[o + 10 + k * 4] = (float) (hands[k * 3 + 2] - c.z);
            CAND[o + 11 + k * 4] = Math.min(handSpeed[k] * keep, 20.0F);
        }
        for (int i = 0; i < 9; i++) {
            CAND[o + 16 + i] = (float) (before[i] - (i % 3 == 0 ? c.x : i % 3 == 1 ? c.y : c.z));
        }
        CAND[o + 25] = s instanceof AvatarRenderState ? 1.0F : 0.0F;
        CAND[o + 26] = Math.max(0.5F, s.boundingBoxWidth / 0.6F);
        FAR[slot] = CAND[o] * CAND[o] + CAND[o + 1] * CAND[o + 1] + CAND[o + 2] * CAND[o + 2];
        OWN[slot] = own;
        count = Math.min(count + 1, CANDIDATES);
        lastBody = now;
    }

    /**
     * One's own body when it is not drawn (the first person): its feet only, for the traces it leaves on the ground — it
     * lets go of no ink (the camera would be inside its smoke) and has no hands.
     */
    private static void unseenOwn(CameraRenderState camera) {
        Chams.Settings set = Chams.settings;
        Minecraft mc = Minecraft.getInstance();
        if (!set.on || !set.traces || !set.self || mc.player == null || !mc.options.getCameraType().isFirstPerson()) {
            return;
        }
        for (int i = 0; i < count; i++) {
            if (OWN[i]) {
                return;
            }
        }
        Vec3 c = camera.pos;
        Vec3 at = mc.player.getPosition(mc.getDeltaTracker().getGameTimeDeltaPartialTick(true));
        int slot = Math.min(count, CANDIDATES - 1);
        int o = slot * EACH;
        java.util.Arrays.fill(CAND, o, o + EACH, 0.0F);
        CAND[o] = (float) (at.x - c.x);
        CAND[o + 1] = (float) (at.y - c.y);
        CAND[o + 2] = (float) (at.z - c.z);
        CAND[o + 3] = mc.player.getBbHeight();
        Vec3 v = mc.player.getDeltaMovement();
        CAND[o + 4] = (float) (v.x * 20.0);
        CAND[o + 5] = (float) (v.y * 20.0);
        CAND[o + 6] = (float) (v.z * 20.0);
        // no ink let go
        CAND[o + 7] = 0.0F;
        for (int k = 0; k < 2; k++) {
            CAND[o + 8 + k * 4] = CAND[o];
            CAND[o + 9 + k * 4] = CAND[o + 1];
            CAND[o + 10 + k * 4] = CAND[o + 2];
        }
        for (int i = 0; i < 9; i++) {
            CAND[o + 16 + i] = CAND[o + i % 3];
        }
        CAND[o + 26] = 1.0F;
        FAR[slot] = 0.0;
        OWN[slot] = true;
        count = Math.min(count + 1, CANDIDATES);
        lastBody = System.nanoTime();
    }

    /**
     * The bodies the engine gets, into BODIES: one's own first, else the nearest to the camera (the grid goes round it),
     * then the nearest others within the grid's reach of it, at most MOST. Returns how many.
     */
    private static int choose() {
        int n = count;
        if (n == 0) {
            return 0;
        }
        Chams.Settings set = Chams.settings;
        int most = Math.clamp(set.bodies, 1, MOST);
        for (int i = 0; i < n; i++) {
            ORDER[i] = i;
        }
        Arrays.sort(ORDER, 0, n, (a, b) -> OWN[a] != OWN[b] ? (OWN[a] ? -1 : 1) : Double.compare(FAR[a], FAR[b]));
        int first = ORDER[0] * EACH;
        int chosen = 0;
        for (int k = 0; k < n && chosen < most; k++) {
            int o = ORDER[k] * EACH;
            // smoke from one's own body alone
            if (set.onlyMe && !OWN[ORDER[k]]) {
                break;
            }
            double dx = CAND[o] - CAND[first];
            double dy = CAND[o + 1] - CAND[first + 1];
            double dz = CAND[o + 2] - CAND[first + 2];
            if (k > 0 && (Math.abs(dx) > REACH || Math.abs(dz) > REACH || Math.abs(dy) > REACH_UP)) {
                continue;
            }
            System.arraycopy(CAND, o, BODIES, chosen * EACH, EACH);
            chosen++;
        }
        return chosen;
    }

    /**
     * After the level, before the hand (GameRendererMixin): this frame's bodies to the engine, over `main`'s picture
     * (`depth` the level's, when it was drawn into a smaller target), for half a minute after the last of them.
     */
    public static void flush(RenderTarget main, @Nullable GpuTexture depth, CameraRenderState camera) {
        long now = System.nanoTime();
        unseenOwn(camera);
        int n = choose();
        count = 0;
        List<Was> last = mobs;
        mobs = mobsNow;
        mobsNow = last;
        mobsNow.clear();
        if (n == 0 && now - lastBody > 32_000_000_000L) {
            return;
        }
        WAS.values().removeIf(w -> now - w.at > 2_000_000_000L);
        // from the eye: the engine casts the rays from there (NativeTerrain.eye); the bodies and the models' boxes were
        // given from the camera's place
        Vec3 eye = cursed.recode.render.NativeTerrain.eye(camera.pos);
        float ex = (float) (eye.x - camera.pos.x);
        float ey = (float) (eye.y - camera.pos.y);
        float ez = (float) (eye.z - camera.pos.z);
        for (int i = 0; i < n; i++) {
            int o = i * EACH;
            // its feet, its hands, where they were the frame before
            for (int at : POINTS) {
                BODIES[o + at] -= ex;
                BODIES[o + at + 1] -= ey;
                BODIES[o + at + 2] -= ez;
            }
        }
        ChamsLiquid.moved(ex, ey, ez);
        Vector4f fog = camera.fogData.color;
        EngineFrame.ink(main.getColorTexture(), depth != null ? depth : main.getDepthTexture(), new float[] {fog.x(), fog.y(), fog.z(), fog.w()}, BODIES, n,
                ChamsLiquid.BOXES, ChamsLiquid.count);
        ChamsLiquid.sent();
    }
}
