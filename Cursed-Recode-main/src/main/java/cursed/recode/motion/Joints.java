package cursed.recode.motion;

import com.mojang.blaze3d.vertex.PoseStack;
import it.unimi.dsi.fastutil.objects.Object2ObjectArrayMap;
import java.util.ArrayList;
import java.util.EnumSet;
import java.util.List;
import java.util.Map;
import java.util.WeakHashMap;
import net.minecraft.client.model.HumanoidModel;
import net.minecraft.client.model.geom.ModelPart;
import net.minecraft.client.model.geom.PartPose;
import net.minecraft.core.Direction;
import net.minecraft.world.entity.HumanoidArm;
import cursed.recode.scene.PartAccess;
import org.jspecify.annotations.Nullable;

/**
 * Living Motion's knees and elbows (stage 2): a player's model's legs and arms — and what lies over them, the skin's
 * pants and sleeves, the armour's models posed from the same state — cut in two at the knee and the elbow, the lower
 * half a part of its own that turns there, about the limb's middle. The bend is mitred, as a picture's frame at its
 * corner: the faces' corners where the halves meet go to the plane half way round (each half's by half the bend, the
 * native moves them: scene/models.rs), so the halves meet edge to edge — nothing of one sinks into the other, nothing
 * opens, the outside of a knee or an elbow comes to a point. The halves keep the texture as it was (the sides part at
 * their middle row, the sole and the top stay where they were). Cut the first time Living Motion poses a model;
 * straight unless it bends them.
 */
public final class Joints {
    /** The knee: half way down the leg (pixels from the hip); the elbow: half way down the arm from its shoulder's pivot;
     *  both in the limb's middle. */
    private static final float KNEE_Y = 6.0F;
    private static final float KNEE_Z = 0.0F;
    private static final float ELBOW_Y = 4.0F;
    private static final float ELBOW_Z = 0.0F;
    private static final Map<HumanoidModel<?>, Joints> CUT = new WeakHashMap<>();
    /** Each half of a cut limb (and what lies over it): its joint, and which half (1 the upper, -1 the lower) — the
     *  corners on the cut turn by half the joint's bend, the lower half's back. */
    private static final Map<ModelPart, Half> HALVES = new WeakHashMap<>();

    private record Half(ModelPart joint, float side, float y, float z) {
    }

    /** The cut cubes' own boxes (pixels, their part's frame: x0 y0 z0 x1 y1 z1; none left of one all gone below): the
     *  game's numbers on a cube are its whole before the cut — the upper half's would be the whole limb, the lower's
     *  a pixel. */
    private static final Map<ModelPart.Cube, float[]> BOXES = new WeakHashMap<>();
    private static final float[] NONE = new float[0];

    public final ModelPart rightKnee;
    public final ModelPart leftKnee;
    public final ModelPart rightElbow;
    public final ModelPart leftElbow;
    /** The lower halves of what lies over the limbs, each with the part whose upper half it is: shown as it is. */
    private final List<ModelPart[]> overlays = new ArrayList<>();
    /** Those of them on the arms (the sleeves' lower halves). */
    private final List<ModelPart> sleeves = new ArrayList<>();
    /** The parts whose faces were changed (the DLL has their old ones if they were drawn before). */
    private final List<ModelPart> touched = new ArrayList<>();

    private Joints(HumanoidModel<?> m) {
        this.rightKnee = this.cut(m.rightLeg, KNEE_Y, KNEE_Z);
        this.leftKnee = this.cut(m.leftLeg, KNEE_Y, KNEE_Z);
        this.rightElbow = this.cut(m.rightArm, ELBOW_Y, ELBOW_Z);
        this.leftElbow = this.cut(m.leftArm, ELBOW_Y, ELBOW_Z);
        // drawn before (Living Motion turned on in a world): their faces go to the DLL anew, the trees laid out anew
        for (ModelPart p : this.touched) {
            ((PartAccess) (Object) p).cursed$setModelId(-1);
        }
        for (ModelPart p : m.root().getAllParts()) {
            ((PartAccess) (Object) p).cursed$setFlat(null);
        }
    }

    /** The model's joints, cut now if they were not. */
    public static Joints of(HumanoidModel<?> m) {
        return CUT.computeIfAbsent(m, Joints::new);
    }

    /** The model's joints if it was cut. */
    public static @Nullable Joints ifCut(HumanoidModel<?> m) {
        return CUT.get(m);
    }

    /** Straight again (the game's poses reset). */
    public void reset() {
        this.rightKnee.resetPose();
        this.leftKnee.resetPose();
        this.rightElbow.resetPose();
        this.leftElbow.resetPose();
    }

    /** The lower halves of the pants and sleeves shown as their upper halves are (the skin's settings, the game's). */
    public void showAsOver() {
        for (ModelPart[] o : this.overlays) {
            o[1].visible = o[0].visible;
            o[1].skipDraw = o[0].skipDraw;
        }
    }

    /** The first person's arm (the game draws the model's arm alone): straight, its sleeve's lower half shown as the
     *  game shows the sleeve. */
    public void firstPerson(boolean sleeve) {
        this.reset();
        for (ModelPart s : this.sleeves) {
            s.visible = sleeve;
            s.skipDraw = false;
        }
    }

    /** A held item in a bent arm's hand: the hand's frame turned about the elbow (after the arm's own). */
    public void hand(HumanoidArm arm, PoseStack pose) {
        ModelPart e = arm == HumanoidArm.LEFT ? this.leftElbow : this.rightElbow;
        if (e.xRot != 0.0F || e.yRot != 0.0F || e.zRot != 0.0F) {
            e.translateAndRotate(pose);
            pose.translate(-e.x / 16.0F, -e.y / 16.0F, -e.z / 16.0F);
        }
    }

    /** A cube's box as it is (pixels, its part's frame: x0 y0 z0 x1 y1 z1; empty when nothing of it is left): a cut
     *  half's own; null for a cube not cut (the game's numbers on it are right). Chams' bodies and Target ESP take it. */
    public static float @Nullable [] box(ModelPart.Cube c) {
        return BOXES.isEmpty() ? null : BOXES.get(c);
    }

    /** Where a cut half's corners turn (pixels, its own frame: the cut's height, the line's depth); null for a part not
     *  cut (NativeModels hands this over with its faces). */
    public static float @Nullable [] hinge(ModelPart part) {
        Half h = HALVES.isEmpty() ? null : HALVES.get(part);
        return h == null ? null : new float[] {h.y, h.z};
    }

    /** How far a cut half's corners on the cut turn now (radians: half its joint's bend, the lower half's back); 0 for
     *  a part not cut. */
    public static float bend(ModelPart part) {
        Half h = HALVES.isEmpty() ? null : HALVES.get(part);
        return h == null ? 0.0F : h.side * h.joint.xRot * 0.5F;
    }

    /** A limb cut at `y` (its own frame): its cubes' halves above stay on it, those below go to a new part turning at
     *  (0, y, z); so do its children's (the pants on a leg, a sleeve on an arm). */
    private ModelPart cut(ModelPart limb, float y, float z) {
        ModelPart joint = new ModelPart(this.lower(limb, y, z), new Object2ObjectArrayMap<>());
        joint.setInitialPose(PartPose.offset(0.0F, y, z));
        joint.resetPose();
        HALVES.put(limb, new Half(joint, 1.0F, y, z));
        HALVES.put(joint, new Half(joint, -1.0F, 0.0F, 0.0F));
        for (Map.Entry<String, ModelPart> c : List.copyOf(childrenOf(limb).entrySet())) {
            ModelPart over = c.getValue();
            if (over.isEmpty() || over.x != 0.0F || over.y != 0.0F || over.z != 0.0F || over.xRot != 0.0F || over.yRot != 0.0F || over.zRot != 0.0F) {
                continue;
            }
            List<ModelPart.Cube> cubes = this.lower(over, y, z);
            if (!cubes.isEmpty()) {
                ModelPart half = new ModelPart(cubes, new Object2ObjectArrayMap<>());
                childrenOf(joint).put("cursed_" + c.getKey(), half);
                this.overlays.add(new ModelPart[] {over, half});
                HALVES.put(over, new Half(joint, 1.0F, y, z));
                HALVES.put(half, new Half(joint, -1.0F, 0.0F, 0.0F));
                if (y == ELBOW_Y) {
                    this.sleeves.add(half);
                }
            }
        }
        childrenOf(limb).put("cursed_joint", joint);
        return joint;
    }

    /** A part's cubes cut at `y`: their upper halves left on it (their faces changed in place), the lower ones given back
     *  in the joint's frame (at (0, y, z)). A cube all above stays, all below moves whole. */
    private List<ModelPart.Cube> lower(ModelPart part, float y, float z) {
        List<ModelPart.Cube> out = new ArrayList<>();
        for (ModelPart.Cube cube : ((PartAccess) (Object) part).cursed$cubes()) {
            float lo = Float.MAX_VALUE;
            float hi = -Float.MAX_VALUE;
            for (ModelPart.Polygon p : cube.polygons) {
                for (ModelPart.Vertex v : p.vertices()) {
                    lo = Math.min(lo, v.y());
                    hi = Math.max(hi, v.y());
                }
            }
            if (hi <= y + 0.01F) {
                continue;
            }
            // the halves meet at the cut itself (the bend mitres them there: no seam to hide)
            float top = lo >= y - 0.01F ? lo : y;
            ModelPart.Polygon[] below = new ModelPart.Polygon[cube.polygons.length];
            for (int i = 0; i < cube.polygons.length; i++) {
                ModelPart.Polygon p = cube.polygons[i];
                below[i] = shifted(squeezed(p, lo, hi, top, hi), 0.0F, y, z);
                if (lo < y - 0.01F) {
                    cube.polygons[i] = squeezed(p, lo, hi, lo, y);
                }
            }
            if (lo >= y - 0.01F) {
                // all below: it goes whole, nothing of it stays above
                for (int i = 0; i < cube.polygons.length; i++) {
                    cube.polygons[i] = collapsed(cube.polygons[i]);
                }
                BOXES.put(cube, NONE);
            } else {
                BOXES.put(cube, new float[] {cube.minX, cube.minY, cube.minZ, cube.maxX, Math.min(cube.maxY, y), cube.maxZ});
            }
            ModelPart.Cube half = cubeOf(below);
            BOXES.put(half, new float[] {cube.minX, Math.max(cube.minY, y) - y, cube.minZ - z, cube.maxX, cube.maxY - y, cube.maxZ - z});
            out.add(half);
            this.touched.add(part);
        }
        return out;
    }

    /** A face of a cube spanning `lo`..`hi` down, made to span `from`..`to`: its corners moved and its texture's rows
     *  with them (a side's), a top or a bottom moved whole. */
    private static ModelPart.Polygon squeezed(ModelPart.Polygon p, float lo, float hi, float from, float to) {
        ModelPart.Vertex[] vs = p.vertices();
        float vLo = Float.NaN;
        float vHi = Float.NaN;
        boolean side = false;
        for (ModelPart.Vertex v : vs) {
            if (Math.abs(v.y() - lo) < 1e-4F) {
                vLo = v.v();
            } else if (Math.abs(v.y() - hi) < 1e-4F) {
                vHi = v.v();
            }
        }
        side = !Float.isNaN(vLo) && !Float.isNaN(vHi);
        ModelPart.Vertex[] out = new ModelPart.Vertex[vs.length];
        for (int k = 0; k < vs.length; k++) {
            ModelPart.Vertex v = vs[k];
            float ny;
            float nv = v.v();
            if (side) {
                boolean atTop = Math.abs(v.y() - lo) < 1e-4F;
                ny = atTop ? from : to;
                nv = vLo + (vHi - vLo) * (ny - lo) / (hi - lo);
            } else {
                // a top stays a top (at `from`), a bottom a bottom (at `to`)
                ny = Math.abs(v.y() - lo) < 1e-4F ? from : to;
            }
            out[k] = new ModelPart.Vertex(v.x(), ny, v.z(), v.u(), nv);
        }
        return new ModelPart.Polygon(out, p.normal());
    }

    private static ModelPart.Polygon shifted(ModelPart.Polygon p, float x, float y, float z) {
        ModelPart.Vertex[] vs = p.vertices();
        ModelPart.Vertex[] out = new ModelPart.Vertex[vs.length];
        for (int k = 0; k < vs.length; k++) {
            ModelPart.Vertex v = vs[k];
            out[k] = new ModelPart.Vertex(v.x() - x, v.y() - y, v.z() - z, v.u(), v.v());
        }
        return new ModelPart.Polygon(out, p.normal());
    }

    /** A face folded to nothing (its cube all went below). */
    private static ModelPart.Polygon collapsed(ModelPart.Polygon p) {
        ModelPart.Vertex[] vs = p.vertices();
        ModelPart.Vertex[] out = new ModelPart.Vertex[vs.length];
        for (int k = 0; k < vs.length; k++) {
            out[k] = new ModelPart.Vertex(vs[0].x(), vs[0].y(), vs[0].z(), vs[k].u(), vs[k].v());
        }
        return new ModelPart.Polygon(out, p.normal());
    }

    /** A cube with these faces (made as any, its faces then put in). */
    private static ModelPart.Cube cubeOf(ModelPart.Polygon[] faces) {
        EnumSet<Direction> some = EnumSet.noneOf(Direction.class);
        for (int i = 0; i < faces.length; i++) {
            some.add(Direction.values()[i]);
        }
        ModelPart.Cube c = new ModelPart.Cube(0, 0, 0.0F, 0.0F, 0.0F, 1.0F, 1.0F, 1.0F, 0.0F, 0.0F, 0.0F, false, 64.0F, 64.0F, some);
        System.arraycopy(faces, 0, c.polygons, 0, faces.length);
        return c;
    }

    @SuppressWarnings("unchecked")
    private static Map<String, ModelPart> childrenOf(ModelPart p) {
        return ((PartAccess) (Object) p).cursed$children();
    }
}
