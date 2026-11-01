package cursed.recode.scene;

import com.mojang.blaze3d.vertex.BufferBuilder;
import com.mojang.blaze3d.vertex.ByteBufferBuilder;
import com.mojang.blaze3d.vertex.DefaultVertexFormat;
import com.mojang.blaze3d.vertex.MeshData;
import com.mojang.blaze3d.vertex.PoseStack;
import com.mojang.blaze3d.vertex.VertexConsumer;
import com.mojang.logging.LogUtils;
import com.mojang.renderpearl.api.pipeline.PrimitiveTopology;
import cursed.recode.dev.Dev;
import cursed.recode.render.Native;
import cursed.recode.render.NativeTerrain;
import cursed.recode.world.VertexAppend;
import java.nio.ByteBuffer;
import java.util.List;
import java.util.Map;
import java.util.concurrent.atomic.LongAdder;
import net.minecraft.client.model.geom.ModelPart;
import org.joml.Matrix3f;
import org.joml.Matrix4f;
import org.joml.Vector3fc;
import org.lwjgl.system.MemoryUtil;
import org.slf4j.Logger;

/**
 * Entity models' vertices made in the DLL (native/src/scene/models.rs). ModelPart.render into a BufferBuilder of the
 * entity format comes here: the part tree is walked as the game walks it (the pose stack, visibility, skipDraw), each
 * drawn part leaves a record (its pose), and one native call writes the whole model's vertices into the builder — the
 * same bytes the game's Cube.compile writes (CURSED_DEV_MODELCHECK renders every model both ways and compares).
 * Into a builder of the frame's staged vertex buffer the GPU makes them instead (ModelJobs, native/src/vk/models.rs):
 * the vertices are reserved, each drawn part leaves a job, and a model with an untrusted pose (its normals need a
 * normalize) goes the CPU way. Any other consumer (outlines, decals, glint wrappers) keeps the game's way.
 */
public final class NativeModels {
    private static final Logger LOGGER = LogUtils.getLogger();
    private static final boolean CHECK = Dev.on("CURSED_DEV_MODELCHECK");
    private static final boolean OFF = Dev.on("CURSED_DEV_NOMODELS");
    /** CURSED_DEV_JAVAWALK: the part tree walked in Java for the GPU's jobs (as before the native walk). */
    private static final boolean JAVA_WALK = Dev.on("CURSED_DEV_JAVAWALK");
    /** CURSED_DEV_NOCPUWALK: the Java walk for models with the CPU's vertices. */
    private static final boolean NO_CPU_WALK = Dev.on("CURSED_DEV_NOCPUWALK");
    /** CURSED_DEV_WALKCHECK: the native walk's CPU vertices next to the Java walk's. */
    private static final boolean WALKCHECK = Dev.on("CURSED_DEV_WALKCHECK");
    static final int RECORD = 116;
    private static final ThreadLocal<Batch> BATCH = ThreadLocal.withInitial(Batch::new);
    private static final LongAdder checked = new LongAdder();
    private static final LongAdder mismatched = new LongAdder();

    private NativeModels() {
    }

    private static final class Batch {
        long mem = MemoryUtil.nmemAlloc(RECORD * 64L);
        int capacity = 64;
        int count;
        int vertices;
        boolean busy;
        boolean game;
        boolean untrusted;
        ByteBufferBuilder scratch;
        /** The native walk's per-frame numbers of the parts, the root's pose and its result. */
        long dynamic = MemoryUtil.nmemAlloc(64L * 40);
        int dynamicCapacity = 64;
        final long root = MemoryUtil.nmemAlloc(100);
        final long result = MemoryUtil.nmemAlloc(8);

        long dynamic(int n) {
            if (n > this.dynamicCapacity) {
                this.dynamicCapacity = Integer.highestOneBit(n) * 2;
                this.dynamic = MemoryUtil.nmemRealloc(this.dynamic, 40L * this.dynamicCapacity);
            }
            return this.dynamic;
        }

        void add(int id, PoseStack.Pose pose, int vertices) {
            if (this.count == this.capacity) {
                this.capacity *= 2;
                this.mem = MemoryUtil.nmemRealloc(this.mem, (long) RECORD * this.capacity);
            }
            long r = this.mem + (long) RECORD * this.count;
            Matrix4f m = pose.pose();
            Matrix3f n = pose.normal();
            MemoryUtil.memPutInt(r, id);
            MemoryUtil.memPutInt(r + 4, m.properties());
            MemoryUtil.memPutInt(r + 8, pose.trustedNormals ? 1 : 0);
            MemoryUtil.memPutInt(r + 12, 0);
            long f = r + 16;
            MemoryUtil.memPutFloat(f, m.m00());
            MemoryUtil.memPutFloat(f + 4, m.m01());
            MemoryUtil.memPutFloat(f + 8, m.m02());
            MemoryUtil.memPutFloat(f + 12, m.m03());
            MemoryUtil.memPutFloat(f + 16, m.m10());
            MemoryUtil.memPutFloat(f + 20, m.m11());
            MemoryUtil.memPutFloat(f + 24, m.m12());
            MemoryUtil.memPutFloat(f + 28, m.m13());
            MemoryUtil.memPutFloat(f + 32, m.m20());
            MemoryUtil.memPutFloat(f + 36, m.m21());
            MemoryUtil.memPutFloat(f + 40, m.m22());
            MemoryUtil.memPutFloat(f + 44, m.m23());
            MemoryUtil.memPutFloat(f + 48, m.m30());
            MemoryUtil.memPutFloat(f + 52, m.m31());
            MemoryUtil.memPutFloat(f + 56, m.m32());
            MemoryUtil.memPutFloat(f + 60, m.m33());
            long g = r + 80;
            MemoryUtil.memPutFloat(g, n.m00());
            MemoryUtil.memPutFloat(g + 4, n.m01());
            MemoryUtil.memPutFloat(g + 8, n.m02());
            MemoryUtil.memPutFloat(g + 12, n.m10());
            MemoryUtil.memPutFloat(g + 16, n.m11());
            MemoryUtil.memPutFloat(g + 20, n.m12());
            MemoryUtil.memPutFloat(g + 24, n.m20());
            MemoryUtil.memPutFloat(g + 28, n.m21());
            MemoryUtil.memPutFloat(g + 32, n.m22());
            this.count++;
            this.vertices += vertices;
        }
    }

    /** ModelPart.render: true when the model went natively (the caller then returns). */
    public static boolean render(ModelPart part, PoseStack poseStack, VertexConsumer buffer, int light, int overlay, int color) {
        if (OFF || !(buffer instanceof BufferBuilder builder) || !((VertexAppend) builder).cursed$entity() || !NativeTerrain.active()) {
            return false;
        }
        Batch b = BATCH.get();
        if (b.busy || b.game) {
            return false;
        }
        ModelJobs jobs = ((VertexAppend) builder).cursed$jobs();
        if (jobs != null && !CHECK && gpu(part, poseStack, builder, jobs, b, light, overlay, color)) {
            return true;
        }
        if (!CHECK && !JAVA_WALK && !NO_CPU_WALK && cpuNative(part, poseStack, builder, b, light, overlay, color)) {
            return true;
        }
        b.busy = true;
        b.count = 0;
        b.vertices = 0;
        long dst = 0L;
        int vertices = 0;
        try {
            walk(part, poseStack, b);
            if (b.vertices > 0) {
                vertices = b.vertices;
                dst = ((VertexAppend) builder).cursed$reserve(vertices);
                int written = Native.modelVertices(b.mem, b.count, light, overlay, color, dst);
                if (written != vertices) {
                    throw new IllegalStateException("Model vertices: wrote " + written + " of " + vertices);
                }
            }
        } finally {
            b.busy = false;
        }
        if (CHECK) {
            check(b, part, poseStack, light, overlay, color, dst, vertices);
        }
        return true;
    }

    /**
     * A part's tree laid out for the native walk (native/src/scene/walk.rs): its parts in pre-order and, per part, the
     * subtree's size, the first vertex in the mesh, the vertices drawn and the part's id. The tree does not change.
     */
    private static final class Flat {
        final ModelPart[] parts;
        final long statics;
        final int drawable;

        Flat(List<ModelPart> order, long statics, int drawable) {
            this.parts = order.toArray(new ModelPart[0]);
            this.statics = statics;
            this.drawable = drawable;
        }
    }

    private static Flat flat(ModelPart root, PartAccess a) {
        Object f = a.cursed$flat();
        if (f != null) {
            return (Flat) f;
        }
        synchronized (root) {
            f = a.cursed$flat();
            if (f != null) {
                return (Flat) f;
            }
            List<ModelPart> order = new java.util.ArrayList<>();
            List<Integer> sizes = new java.util.ArrayList<>();
            lay(root, order, sizes);
            long statics = MemoryUtil.nmemAlloc(16L * order.size());
            int drawable = 0;
            for (int i = 0; i < order.size(); i++) {
                ModelPart p = order.get(i);
                PartAccess pa = (PartAccess) (Object) p;
                int vertices = pa.cursed$cubes().isEmpty() ? 0 : pa.cursed$vertexCount();
                int id = vertices > 0 ? id(p, pa) : -1;
                MemoryUtil.memPutInt(statics + 16L * i, sizes.get(i));
                MemoryUtil.memPutInt(statics + 16L * i + 4, vertices > 0 ? pa.cursed$firstVertex() : 0);
                MemoryUtil.memPutInt(statics + 16L * i + 8, vertices);
                MemoryUtil.memPutInt(statics + 16L * i + 12, id);
                drawable += vertices > 0 ? 1 : 0;
            }
            Flat made = new Flat(order, statics, drawable);
            a.cursed$setFlat(made);
            return made;
        }
    }

    /** Pre-order; returns the subtree's size. */
    private static int lay(ModelPart part, List<ModelPart> order, List<Integer> sizes) {
        int at = order.size();
        order.add(part);
        sizes.add(1);
        int size = 1;
        for (ModelPart child : ((PartAccess) (Object) part).cursed$children().values()) {
            size += lay(child, order, sizes);
        }
        sizes.set(at, size);
        return size;
    }

    /**
     * A model drawn with the CPU's vertices (a sorted, translucent draw: skin layers) walked natively, as the GPU's
     * jobs are: the same poses, then the vertices written from them. False (nothing written) when a pose's normals are
     * untrusted; then the Java walk. CURSED_DEV_NOCPUWALK keeps the Java walk.
     */
    private static boolean cpuNative(ModelPart part, PoseStack poseStack, BufferBuilder builder, Batch b, int light, int overlay, int color) {
        if (!poseStack.last().trustedNormals) {
            return false;
        }
        long[] out = new long[1];
        b.busy = true;
        try {
            Flat f = parts(part, poseStack, b);
            Native.modelWalkCpu(f.statics, b.dynamic(f.parts.length), f.parts.length, b.root, b.result);
            if (MemoryUtil.memGetInt(b.result + 4) != 0) {
                return false;
            }
            int vertices = MemoryUtil.memGetInt(b.result);
            if (vertices > 0) {
                long dst = ((VertexAppend) builder).cursed$reserve(vertices);
                int written = Native.modelWalkWrite(light, overlay, color, dst);
                if (written != vertices) {
                    throw new IllegalStateException("Model vertices: wrote " + written + " of " + vertices);
                }
                out[0] = dst;
            }
        } finally {
            b.busy = false;
        }
        if (WALKCHECK) {
            walkCheck(part, poseStack, b, light, overlay, color, out[0], MemoryUtil.memGetInt(b.result));
        }
        return true;
    }

    private static long walkChecked;
    private static long walkDiffer;
    private static float walkWorst;

    /** Dev (CURSED_DEV_WALKCHECK): the Java walk's vertices for the same model next to the native walk's. */
    private static void walkCheck(ModelPart part, PoseStack poseStack, Batch b, int light, int overlay, int color, long ours, int vertices) {
        b.busy = true;
        b.count = 0;
        b.vertices = 0;
        long theirs = 0L;
        try {
            walk(part, poseStack, b);
            if (b.vertices != vertices) {
                walkDiffer++;
                LOGGER.warn("[walk check] {} vertices vs the Java walk's {}", vertices, b.vertices);
                return;
            }
            if (vertices == 0) {
                return;
            }
            theirs = MemoryUtil.nmemAlloc(36L * vertices);
            Native.modelVertices(b.mem, b.count, light, overlay, color, theirs);
            boolean differ = false;
            for (int v = 0; v < vertices; v++) {
                long x = ours + 36L * v;
                long y = theirs + 36L * v;
                for (int k = 0; k < 3; k++) {
                    float d = Math.abs(MemoryUtil.memGetFloat(x + 4L * k) - MemoryUtil.memGetFloat(y + 4L * k));
                    walkWorst = Math.max(walkWorst, d);
                    if (d > 1e-3F) {
                        differ = true;
                    }
                }
                for (int k = 12; k < 32; k++) {
                    if (MemoryUtil.memGetByte(x + k) != MemoryUtil.memGetByte(y + k)) {
                        differ = true;
                    }
                }
                for (int k = 32; k < 35; k++) {
                    if (Math.abs(MemoryUtil.memGetByte(x + k) - MemoryUtil.memGetByte(y + k)) > 1) {
                        differ = true;
                    }
                }
            }
            if (differ) {
                walkDiffer++;
            }
        } finally {
            if (theirs != 0L) {
                MemoryUtil.nmemFree(theirs);
            }
            b.busy = false;
        }
        if (++walkChecked % 5000 == 0) {
            LOGGER.info("[walk check] {} models, {} differ; positions at most {} apart", walkChecked, walkDiffer, walkWorst);
        }
    }

    /** A part's flags this frame (scene/walk.rs): 1 visible, 2 skipDraw; above them, a cut limb's half's bend at its
     *  cut (Living Motion's mitred joints: radians × 8192, 16 bits). */
    private static int flags(ModelPart p) {
        int f = (p.visible ? 1 : 0) | (p.skipDraw ? 2 : 0);
        float bend = cursed.recode.motion.Joints.bend(p);
        if (bend != 0.0F) {
            f |= (Math.round(Math.max(-3.9F, Math.min(3.9F, bend)) * 8192.0F) & 0xFFFF) << 16;
        }
        return f;
    }

    /** The parts' numbers this frame and the root pose into the batch, for the native walk. */
    private static Flat parts(ModelPart part, PoseStack poseStack, Batch b) {
        PoseStack.Pose pose = poseStack.last();
        Flat f = flat(part, (PartAccess) (Object) part);
        ModelPart[] parts = f.parts;
        long d = b.dynamic(parts.length);
        for (int i = 0; i < parts.length; i++) {
            ModelPart p = parts[i];
            long at = d + 40L * i;
            MemoryUtil.memPutInt(at, flags(p));
            MemoryUtil.memPutFloat(at + 4, p.x);
            MemoryUtil.memPutFloat(at + 8, p.y);
            MemoryUtil.memPutFloat(at + 12, p.z);
            MemoryUtil.memPutFloat(at + 16, p.xRot);
            MemoryUtil.memPutFloat(at + 20, p.yRot);
            MemoryUtil.memPutFloat(at + 24, p.zRot);
            MemoryUtil.memPutFloat(at + 28, p.xScale);
            MemoryUtil.memPutFloat(at + 32, p.yScale);
            MemoryUtil.memPutFloat(at + 36, p.zScale);
        }
        Matrix4f m = pose.pose();
        Matrix3f n = pose.normal();
        long r = b.root;
        MemoryUtil.memPutFloat(r, m.m00());
        MemoryUtil.memPutFloat(r + 4, m.m01());
        MemoryUtil.memPutFloat(r + 8, m.m02());
        MemoryUtil.memPutFloat(r + 12, m.m03());
        MemoryUtil.memPutFloat(r + 16, m.m10());
        MemoryUtil.memPutFloat(r + 20, m.m11());
        MemoryUtil.memPutFloat(r + 24, m.m12());
        MemoryUtil.memPutFloat(r + 28, m.m13());
        MemoryUtil.memPutFloat(r + 32, m.m20());
        MemoryUtil.memPutFloat(r + 36, m.m21());
        MemoryUtil.memPutFloat(r + 40, m.m22());
        MemoryUtil.memPutFloat(r + 44, m.m23());
        MemoryUtil.memPutFloat(r + 48, m.m30());
        MemoryUtil.memPutFloat(r + 52, m.m31());
        MemoryUtil.memPutFloat(r + 56, m.m32());
        MemoryUtil.memPutFloat(r + 60, m.m33());
        MemoryUtil.memPutFloat(r + 64, n.m00());
        MemoryUtil.memPutFloat(r + 68, n.m01());
        MemoryUtil.memPutFloat(r + 72, n.m02());
        MemoryUtil.memPutFloat(r + 76, n.m10());
        MemoryUtil.memPutFloat(r + 80, n.m11());
        MemoryUtil.memPutFloat(r + 84, n.m12());
        MemoryUtil.memPutFloat(r + 88, n.m20());
        MemoryUtil.memPutFloat(r + 92, n.m21());
        MemoryUtil.memPutFloat(r + 96, n.m22());
        return f;
    }

    /** The parts' numbers this frame and the root pose, then the native walk; false when the normals are untrusted. */
    private static boolean walkNative(ModelPart part, PoseStack poseStack, Batch b, ModelJobs jobs, int abgr, int overlay, int light, int color) {
        PoseStack.Pose pose = poseStack.last();
        if (!pose.trustedNormals) {
            return false;
        }
        Flat f = flat(part, (PartAccess) (Object) part);
        ModelPart[] parts = f.parts;
        long d = b.dynamic(parts.length);
        for (int i = 0; i < parts.length; i++) {
            ModelPart p = parts[i];
            long at = d + 40L * i;
            MemoryUtil.memPutInt(at, flags(p));
            MemoryUtil.memPutFloat(at + 4, p.x);
            MemoryUtil.memPutFloat(at + 8, p.y);
            MemoryUtil.memPutFloat(at + 12, p.z);
            MemoryUtil.memPutFloat(at + 16, p.xRot);
            MemoryUtil.memPutFloat(at + 20, p.yRot);
            MemoryUtil.memPutFloat(at + 24, p.zRot);
            MemoryUtil.memPutFloat(at + 28, p.xScale);
            MemoryUtil.memPutFloat(at + 32, p.yScale);
            MemoryUtil.memPutFloat(at + 36, p.zScale);
        }
        Matrix4f m = pose.pose();
        Matrix3f n = pose.normal();
        long r = b.root;
        MemoryUtil.memPutFloat(r, m.m00());
        MemoryUtil.memPutFloat(r + 4, m.m01());
        MemoryUtil.memPutFloat(r + 8, m.m02());
        MemoryUtil.memPutFloat(r + 12, m.m03());
        MemoryUtil.memPutFloat(r + 16, m.m10());
        MemoryUtil.memPutFloat(r + 20, m.m11());
        MemoryUtil.memPutFloat(r + 24, m.m12());
        MemoryUtil.memPutFloat(r + 28, m.m13());
        MemoryUtil.memPutFloat(r + 32, m.m20());
        MemoryUtil.memPutFloat(r + 36, m.m21());
        MemoryUtil.memPutFloat(r + 40, m.m22());
        MemoryUtil.memPutFloat(r + 44, m.m23());
        MemoryUtil.memPutFloat(r + 48, m.m30());
        MemoryUtil.memPutFloat(r + 52, m.m31());
        MemoryUtil.memPutFloat(r + 56, m.m32());
        MemoryUtil.memPutFloat(r + 60, m.m33());
        MemoryUtil.memPutFloat(r + 64, n.m00());
        MemoryUtil.memPutFloat(r + 68, n.m01());
        MemoryUtil.memPutFloat(r + 72, n.m02());
        MemoryUtil.memPutFloat(r + 76, n.m10());
        MemoryUtil.memPutFloat(r + 80, n.m11());
        MemoryUtil.memPutFloat(r + 84, n.m12());
        MemoryUtil.memPutFloat(r + 88, n.m20());
        MemoryUtil.memPutFloat(r + 92, n.m21());
        MemoryUtil.memPutFloat(r + 96, n.m22());
        long dst = jobs.reserve(f.drawable);
        int written = Native.modelWalk(f.statics, d, parts.length, r, dst, abgr, overlay, light, color, b.result);
        if (MemoryUtil.memGetInt(b.result + 4) != 0) {
            return false;
        }
        jobs.commit(written);
        b.vertices = MemoryUtil.memGetInt(b.result);
        return true;
    }

    /** The GPU way; false (nothing reserved, no jobs left) when a part's pose has untrusted normals. */
    private static boolean gpu(ModelPart part, PoseStack poseStack, BufferBuilder builder, ModelJobs jobs, Batch b, int light, int overlay, int color) {
        int start = jobs.count;
        b.busy = true;
        b.untrusted = false;
        b.vertices = 0;
        try {
            int abgr = (color & 0xFF00FF00) | ((color >> 16) & 0xFF) | ((color & 0xFF) << 16);
            if (JAVA_WALK) {
                walkGpu(part, poseStack, b, jobs, abgr, overlay, light, color);
            } else if (!walkNative(part, poseStack, b, jobs, abgr, overlay, light, color)) {
                b.untrusted = true;
            }
            if (b.untrusted) {
                jobs.count = start;
                return false;
            }
            if (b.vertices > 0) {
                VertexAppend v = (VertexAppend) builder;
                long dst = v.cursed$reserve(b.vertices);
                jobs.shift(start, (int) (dst - v.cursed$base()));
            }
            return true;
        } finally {
            b.busy = false;
        }
    }

    private static void walkGpu(ModelPart part, PoseStack poseStack, Batch b, ModelJobs jobs, int abgr, int overlay, int light, int color) {
        if (!part.visible || b.untrusted) {
            return;
        }
        PartAccess a = (PartAccess) (Object) part;
        List<ModelPart.Cube> cubes = a.cursed$cubes();
        Map<String, ModelPart> children = a.cursed$children();
        if (cubes.isEmpty() && children.isEmpty()) {
            return;
        }
        poseStack.pushPose();
        part.translateAndRotate(poseStack);
        if (!part.skipDraw && !cubes.isEmpty()) {
            int vertices = a.cursed$vertexCount();
            if (vertices > 0) {
                PoseStack.Pose pose = poseStack.last();
                if (!pose.trustedNormals) {
                    b.untrusted = true;
                } else {
                    int id = id(part, a);
                    jobs.add(pose, a.cursed$firstVertex(), vertices, b.vertices * 36, abgr, overlay, light, id, color);
                    b.vertices += vertices;
                }
            }
        }
        for (ModelPart child : children.values()) {
            walkGpu(child, poseStack, b, jobs, abgr, overlay, light, color);
        }
        poseStack.popPose();
    }

    private static void walk(ModelPart part, PoseStack poseStack, Batch b) {
        if (!part.visible) {
            return;
        }
        PartAccess a = (PartAccess) (Object) part;
        List<ModelPart.Cube> cubes = a.cursed$cubes();
        Map<String, ModelPart> children = a.cursed$children();
        if (cubes.isEmpty() && children.isEmpty()) {
            return;
        }
        poseStack.pushPose();
        part.translateAndRotate(poseStack);
        if (!part.skipDraw && !cubes.isEmpty()) {
            int vertices = a.cursed$vertexCount();
            if (vertices > 0) {
                b.add(id(part, a), poseStack.last(), vertices);
            }
        }
        for (ModelPart child : children.values()) {
            walk(child, poseStack, b);
        }
        poseStack.popPose();
    }

    /** The part's polygons go to the DLL the first time it is drawn. */
    private static int id(ModelPart part, PartAccess a) {
        int id = a.cursed$modelId();
        if (id >= 0) {
            return id;
        }
        synchronized (part) {
            id = a.cursed$modelId();
            if (id >= 0) {
                return id;
            }
            int polygons = a.cursed$vertexCount() / 4;
            // a cut limb's half: where its corners on the cut turn, after its faces
            float[] hinge = cursed.recode.motion.Joints.hinge(part);
            long mem = MemoryUtil.nmemAlloc(4L + polygons * 23L * 4L + 8L);
            try {
                MemoryUtil.memPutInt(mem, polygons);
                long at = mem + 4;
                for (ModelPart.Cube cube : a.cursed$cubes()) {
                    for (ModelPart.Polygon polygon : cube.polygons) {
                        Vector3fc normal = polygon.normal();
                        MemoryUtil.memPutFloat(at, normal.x());
                        MemoryUtil.memPutFloat(at + 4, normal.y());
                        MemoryUtil.memPutFloat(at + 8, normal.z());
                        at += 12;
                        for (ModelPart.Vertex v : polygon.vertices()) {
                            MemoryUtil.memPutFloat(at, v.x());
                            MemoryUtil.memPutFloat(at + 4, v.y());
                            MemoryUtil.memPutFloat(at + 8, v.z());
                            MemoryUtil.memPutFloat(at + 12, v.u());
                            MemoryUtil.memPutFloat(at + 16, v.v());
                            at += 20;
                        }
                    }
                }
                if (hinge != null) {
                    MemoryUtil.memPutFloat(at, hinge[0]);
                    MemoryUtil.memPutFloat(at + 4, hinge[1]);
                    at += 8;
                }
                id = Native.modelPart(mem, (int) (at - mem));
            } finally {
                MemoryUtil.nmemFree(mem);
            }
            a.cursed$setFirstVertex(Native.modelPartFirst(id));
            a.cursed$setModelId(id);
            return id;
        }
    }

    /** Dev: the same model the game's way into a scratch builder; the bytes must match (all but each padding byte). */
    private static void check(Batch b, ModelPart part, PoseStack poseStack, int light, int overlay, int color, long ours, int vertices) {
        if (b.scratch == null) {
            b.scratch = new ByteBufferBuilder(1 << 20);
        }
        BufferBuilder game = new BufferBuilder(b.scratch, PrimitiveTopology.QUADS, DefaultVertexFormat.ENTITY);
        b.game = true;
        try {
            part.render(poseStack, game, light, overlay, color);
        } finally {
            b.game = false;
        }
        int differ = 0;
        String first = null;
        MeshData mesh = game.build();
        try {
            int gameVertices = mesh == null ? 0 : mesh.drawState().vertexCount();
            if (gameVertices != vertices) {
                first = vertices + " vertices vs the game's " + gameVertices;
                differ = 1;
            } else if (mesh != null) {
                ByteBuffer g = mesh.vertexBuffer();
                for (int v = 0; v < vertices; v++) {
                    for (int k = 0; k < 35; k++) {
                        byte x = MemoryUtil.memGetByte(ours + v * 36L + k);
                        byte y = g.get(g.position() + v * 36 + k);
                        if (x != y) {
                            if (first == null) {
                                first = "vertex " + v + " byte " + k + ": " + (x & 0xFF) + " vs the game's " + (y & 0xFF);
                            }
                            differ++;
                            break;
                        }
                    }
                }
            }
        } finally {
            if (mesh != null) {
                mesh.close();
            }
        }
        checked.increment();
        if (differ > 0) {
            long n = mismatched.sum();
            mismatched.increment();
            if (n < 20) {
                LOGGER.warn("[models] a model differs from the game: {} vertices differ; first {}", differ, first);
            }
        }
        if (checked.sum() % 5000 == 0) {
            LOGGER.info("[models] checked {} models, {} differ", checked.sum(), mismatched.sum());
        }
    }
}
