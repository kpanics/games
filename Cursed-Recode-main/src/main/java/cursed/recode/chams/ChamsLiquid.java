package cursed.recode.chams;

import com.mojang.blaze3d.vertex.PoseStack;
import net.minecraft.client.model.Model;
import net.minecraft.client.model.geom.ModelPart;
import org.joml.Matrix4f;
import org.joml.Vector3f;

/**
 * A body's boxes for the engine's looks (each cube of each part that shows, where the model's pose puts it this frame,
 * from the camera, and the light on the body). Chams' polymorphic liquid (its Look "Polymorph"): a body in it is not
 * drawn as its polygons — the engine draws the boxes and the drops round them as one liquid (native/src/engine/
 * liquid.rs: drops flowing out of the body and back, the surfaces between them joining as a liquid's do). The curse
 * (its Look "Cursed"): the body drawn as the game draws it, its boxes where the engine's strands grow out of it and its
 * ash comes off it (native/src/engine/curse.rs). Every model is boxes, so a mob's too.
 */
public final class ChamsLiquid {
    /** The most boxes a frame (engine/liquid.rs MOST_BOXES), and the numbers of each: its middle (xyz, w the body's
     *  number), its three half-sides as vectors (w of the first the block light on the body, of the second the sky's,
     *  0..1). */
    public static final int MOST = 128;
    public static final int EACH = 16;
    static final float[] BOXES = new float[MOST * EACH];
    static int count;
    /** The body being submitted now (a number for its boxes; below 0 one's own). */
    private static int body;
    private static int bodies;
    private static final Vector3f TMP = new Vector3f();

    private ChamsLiquid() {
    }

    /** A new body's boxes follow (`own`: one's own body). */
    static void bodyStart(boolean own) {
        bodies++;
        body = own ? -bodies : bodies;
    }

    /**
     * A model of a body in the look, as the game is about to submit it: posed for its state (the game poses the shared
     * model again before drawing), its boxes taken where the pose stack puts them, with its light (the game's packed
     * block and sky light). Whether they were: all of them, or none when they would not all fit in this frame's (the
     * body is then drawn as polygons, not left out).
     */
    @SuppressWarnings({"unchecked", "rawtypes"})
    public static boolean model(Model model, Object state, PoseStack poseStack, int light) {
        try {
            ((Model) model).setupAnim(state);
        } catch (ClassCastException e) {
            return false;
        }
        ModelPart root = model.root();
        int[] cubes = {0};
        root.visit(poseStack, (pose, path, index, cube) -> {
            if (shows(root, path)) {
                cubes[0]++;
            }
        });
        if (count + cubes[0] > MOST) {
            return false;
        }
        root.visit(poseStack, (pose, path, index, cube) -> {
            if (shows(root, path)) {
                add(pose.pose(), cube, ((light >>> 4) & 15) / 15.0F, ((light >>> 20) & 15) / 15.0F);
            }
        });
        return true;
    }

    /** Whether the part at `path` ("/head/hat") shows: it and every part above it visible and drawn. */
    private static boolean shows(ModelPart root, String path) {
        if (!root.visible || root.skipDraw) {
            return false;
        }
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

    private static void add(Matrix4f m, ModelPart.Cube c, float block, float sky) {
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
        int o = count * EACH;
        m.transformPosition((x0 + x1) / 32.0F, (y0 + y1) / 32.0F, (z0 + z1) / 32.0F, TMP);
        BOXES[o] = TMP.x;
        BOXES[o + 1] = TMP.y;
        BOXES[o + 2] = TMP.z;
        BOXES[o + 3] = body;
        float[] half = {(x1 - x0) / 32.0F, (y1 - y0) / 32.0F, (z1 - z0) / 32.0F};
        for (int a = 0; a < 3; a++) {
            m.transformDirection(a == 0 ? half[0] : 0.0F, a == 1 ? half[1] : 0.0F, a == 2 ? half[2] : 0.0F, TMP);
            BOXES[o + 4 + a * 4] = TMP.x;
            BOXES[o + 5 + a * 4] = TMP.y;
            BOXES[o + 6 + a * 4] = TMP.z;
            BOXES[o + 7 + a * 4] = a == 0 ? block : a == 1 ? sky : 0.0F;
        }
        count++;
    }

    /** This frame's boxes' middles moved by `-e` (from the camera's place to the eye's, ChamsInk.flush). */
    static void moved(float ex, float ey, float ez) {
        for (int i = 0; i < count; i++) {
            BOXES[i * EACH] -= ex;
            BOXES[i * EACH + 1] -= ey;
            BOXES[i * EACH + 2] -= ez;
        }
    }

    /** This frame's boxes have gone to the engine: the next frame's begin. */
    static void sent() {
        count = 0;
        body = 0;
        bodies = 0;
    }
}
