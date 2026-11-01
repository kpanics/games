package cursed.recode.scene;

import com.mojang.blaze3d.vertex.BufferBuilder;
import com.mojang.blaze3d.vertex.ByteBufferBuilder;
import com.mojang.blaze3d.vertex.DefaultVertexFormat;
import com.mojang.blaze3d.vertex.MeshData;
import com.mojang.blaze3d.vertex.PoseStack;
import com.mojang.blaze3d.vertex.QuadInstance;
import com.mojang.blaze3d.vertex.VertexConsumer;
import com.mojang.logging.LogUtils;
import com.mojang.renderpearl.api.pipeline.PrimitiveTopology;
import cursed.recode.dev.Dev;
import cursed.recode.render.Native;
import cursed.recode.render.NativeTerrain;
import cursed.recode.world.VertexAppend;
import java.nio.ByteBuffer;
import java.util.concurrent.atomic.LongAdder;
import java.util.function.Function;
import net.minecraft.client.renderer.feature.ItemFeatureRenderer;
import net.minecraft.client.renderer.item.ItemStackRenderState;
import net.minecraft.client.renderer.rendertype.RenderType;
import net.minecraft.client.resources.model.geometry.BakedQuad;
import org.joml.Matrix3f;
import org.joml.Matrix4f;
import org.joml.Vector3fc;
import org.lwjgl.system.MemoryUtil;
import org.slf4j.Logger;

/**
 * Items' quads (ItemFeatureRenderer.prepareMainSubmit) made in the DLL (native/src/scene/models.rs `quads`): the quads
 * of a submit that go to the same entity-format BufferBuilder one after another are written by one native call — the
 * same bytes as VertexConsumer.putBakedQuad (CURSED_DEV_MODELCHECK compares). Into a builder of the frame's staged
 * vertex buffer the GPU makes them instead (ModelJobs, native/src/vk/models.rs): each quad is handed to the GPU's mesh
 * once (a model's quads together, so a run of them is one range there) and a run of quads leaves one job. Untrusted
 * poses and quads tinted by a third layer or later go the CPU way. Special foil (its decal UVs) and any other consumer
 * keep the game's way, quad by quad, in the same order.
 */
public final class NativeItems {
    private static final Logger LOGGER = LogUtils.getLogger();
    private static final boolean CHECK = Dev.on("CURSED_DEV_MODELCHECK");
    private static final boolean OFF = Dev.on("CURSED_DEV_NOMODELS");
    /** CURSED_DEV_NOITEMPLAN: every quad looked at every time (no plans). */
    private static final boolean NO_PLAN = Dev.on("CURSED_DEV_NOITEMPLAN");

    /**
     * What a model's quad list comes to on the GPU, worked out once: one render type for all of it (consolidating), no
     * quad tinted past the second layer, and the runs of the GPU mesh's vertices — the same jobs the quad loop leaves.
     * `quads` and `last` tell a list from another that starts with the same quad.
     */
    private record Plan(int quads, BakedQuad last, RenderType type, int[] runs) {
    }

    /** The two foils' plans of a list (index 0 none, 1 standard); a plan with a null type: the list cannot have one. */
    private static final class Plans {
        final Plan[] byFoil = new Plan[2];
    }

    private static Plan plan(java.util.List<BakedQuad> quads, ItemStackRenderState.FoilType foil) {
        int f = foil == ItemStackRenderState.FoilType.STANDARD ? 1 : 0;
        BakedQuad first = quads.get(0);
        QuadAccess a = (QuadAccess) (Object) first;
        Object kept = a.cursed$plan();
        Plans plans = kept instanceof Plans p ? p : null;
        BakedQuad last = quads.get(quads.size() - 1);
        if (plans != null) {
            Plan p = plans.byFoil[f];
            if (p != null && p.quads() == quads.size() && p.last() == last) {
                return p;
            }
        } else {
            plans = new Plans();
            a.cursed$setPlan(plans);
        }
        RenderType type = null;
        boolean ok = true;
        java.util.List<Integer> runs = new java.util.ArrayList<>();
        int runFirst = -1;
        int runQuads = 0;
        for (BakedQuad quad : quads) {
            BakedQuad.MaterialInfo material = quad.materialInfo();
            RenderType t = f == 1 ? material.itemGlintRenderType() : material.itemRenderType();
            if (type == null) {
                type = t;
            }
            if (t != type || !t.canConsolidateConsecutiveGeometry() || (material.isTinted() && material.tintIndex() >= 2)) {
                ok = false;
                break;
            }
            int v = gpuVertex(quad, quads);
            if (runQuads > 0 && v != runFirst + 4 * runQuads) {
                runs.add(runFirst);
                runs.add(runQuads);
                runQuads = 0;
            }
            if (runQuads == 0) {
                runFirst = v;
            }
            runQuads++;
        }
        if (ok && runQuads > 0) {
            runs.add(runFirst);
            runs.add(runQuads);
        }
        Plan made = new Plan(quads.size(), last, ok ? type : null, ok ? runs.stream().mapToInt(Integer::intValue).toArray() : null);
        plans.byFoil[f] = made;
        return made;
    }

    /** The plan's jobs, when the builder is the GPU's; false to go the quad loop. */
    private static boolean planned(ItemFeatureRenderer.Submit submit, Function<RenderType, VertexConsumer> vertexBuilder) {
        java.util.List<BakedQuad> quads = submit.quads();
        if (NO_PLAN || quads.isEmpty() || !submit.pose().trustedNormals || CHECK) {
            return false;
        }
        Plan plan = plan(quads, submit.foilType());
        if (plan.type() == null) {
            return false;
        }
        if (!(vertexBuilder.apply(plan.type()) instanceof BufferBuilder builder) || !((VertexAppend) builder).cursed$entity()) {
            return false;
        }
        ModelJobs jobs = ((VertexAppend) builder).cursed$jobs();
        if (jobs == null) {
            return false;
        }
        VertexAppend v = (VertexAppend) builder;
        int[] runs = plan.runs();
        for (int i = 0; i < runs.length; i += 2) {
            int vertices = runs[i + 1] * 4;
            long dst = v.cursed$reserve(vertices);
            jobs.addItem(submit.pose(), runs[i], vertices, (int) (dst - v.cursed$base()), submit.overlayCoords(), submit.lightCoords(), submit.tintLayers());
        }
        return true;
    }
    private static final int HEADER = 116;
    private static final int QUAD = 100;
    private static final ThreadLocal<Run> RUN = ThreadLocal.withInitial(Run::new);
    private static final LongAdder checked = new LongAdder();
    private static final LongAdder mismatched = new LongAdder();

    private NativeItems() {
    }

    private static final class Run {
        final long header = MemoryUtil.nmemAlloc(HEADER);
        long quads = MemoryUtil.nmemAlloc((long) QUAD * 64);
        int capacity = 64;
        int count;
        BufferBuilder builder;
        /** The GPU's jobs of `builder` when its quads can go there. */
        ModelJobs jobs;
        int gpuFirst;
        int gpuQuads;
        ByteBufferBuilder scratch;
        final QuadInstance instance = new QuadInstance();
    }

    /**
     * prepareMainSubmit: false when the game should do it (special foil, or natives off). `vertexBuilder` is the
     * renderer's getVertexBuilder.
     */
    public static boolean prepare(ItemFeatureRenderer.Submit submit, QuadInstance instance, Function<RenderType, VertexConsumer> vertexBuilder) {
        ItemStackRenderState.FoilType foil = submit.foilType();
        if (OFF || foil == ItemStackRenderState.FoilType.SPECIAL || !NativeTerrain.active()) {
            return false;
        }
        if (planned(submit, vertexBuilder)) {
            return true;
        }
        instance.setLightCoords(submit.lightCoords());
        instance.setOverlayCoords(submit.overlayCoords());
        Run run = RUN.get();
        run.count = 0;
        run.gpuQuads = 0;
        run.builder = null;
        header(run, submit);
        java.util.List<BakedQuad> quads = submit.quads();
        boolean trusted = submit.pose().trustedNormals && !CHECK;
        int[] layers = submit.tintLayers();
        RenderType lastType = null;
        VertexConsumer lastConsumer = null;
        for (int i = 0, n = quads.size(); i < n; i++) {
            BakedQuad quad = quads.get(i);
            BakedQuad.MaterialInfo material = quad.materialInfo();
            RenderType type = foil == ItemStackRenderState.FoilType.STANDARD ? material.itemGlintRenderType() : material.itemRenderType();
            int color = material.isTinted() ? layerColor(layers, material.tintIndex()) : -1;
            // The renderer hands out the same builder again for the same consolidating type while nothing else draws
            // in between (only this loop does); other types get a new draw each call, so they always ask.
            VertexConsumer consumer;
            if (type == lastType && type.canConsolidateConsecutiveGeometry()) {
                consumer = lastConsumer;
            } else {
                consumer = vertexBuilder.apply(type);
                lastType = type;
                lastConsumer = consumer;
            }
            if (consumer instanceof BufferBuilder builder && ((VertexAppend) builder).cursed$entity()) {
                if (run.builder != builder) {
                    flush(run, submit);
                    run.builder = builder;
                    run.jobs = trusted ? ((VertexAppend) builder).cursed$jobs() : null;
                }
                int tint = material.tintIndex();
                // The GPU knows the first two tint layers; an index past the layers is white either way.
                if (run.jobs != null && (!material.isTinted() || tint < 2 || tint >= layers.length)) {
                    int v = gpuVertex(quad, quads);
                    flushCpu(run, submit);
                    if (run.gpuQuads > 0 && v != run.gpuFirst + 4 * run.gpuQuads) {
                        flushGpu(run, submit);
                    }
                    if (run.gpuQuads == 0) {
                        run.gpuFirst = v;
                    }
                    run.gpuQuads++;
                } else {
                    flushGpu(run, submit);
                    add(run, quad, color);
                }
            } else {
                flush(run, submit);
                instance.setColor(color);
                consumer.putBakedQuad(submit.pose(), quad, instance);
            }
        }
        flush(run, submit);
        return true;
    }

    private static int layerColor(int[] layers, int layer) {
        return layer >= 0 && layer < layers.length ? layers[layer] : -1;
    }

    private static void header(Run run, ItemFeatureRenderer.Submit submit) {
        PoseStack.Pose pose = submit.pose();
        Matrix4f m = pose.pose();
        Matrix3f n = pose.normal();
        long h = run.header;
        MemoryUtil.memPutInt(h, m.properties());
        MemoryUtil.memPutInt(h + 4, pose.trustedNormals ? 1 : 0);
        MemoryUtil.memPutInt(h + 8, submit.lightCoords());
        MemoryUtil.memPutInt(h + 12, submit.overlayCoords());
        long f = h + 16;
        float[] v = {m.m00(), m.m01(), m.m02(), m.m03(), m.m10(), m.m11(), m.m12(), m.m13(), m.m20(), m.m21(), m.m22(), m.m23(), m.m30(), m.m31(), m.m32(), m.m33(),
                n.m00(), n.m01(), n.m02(), n.m10(), n.m11(), n.m12(), n.m20(), n.m21(), n.m22()};
        for (int i = 0; i < v.length; i++) {
            MemoryUtil.memPutFloat(f + i * 4L, v[i]);
        }
    }

    private static void add(Run run, BakedQuad quad, int color) {
        if (run.count == run.capacity) {
            run.capacity *= 2;
            run.quads = MemoryUtil.nmemRealloc(run.quads, (long) QUAD * run.capacity);
        }
        long q = run.quads + (long) QUAD * run.count;
        for (int j = 0; j < 4; j++) {
            Vector3fc p = quad.position(j);
            MemoryUtil.memPutFloat(q + j * 12L, p.x());
            MemoryUtil.memPutFloat(q + j * 12L + 4, p.y());
            MemoryUtil.memPutFloat(q + j * 12L + 8, p.z());
            MemoryUtil.memPutLong(q + 48 + j * 8L, quad.packedUV(j));
        }
        Vector3fc normal = quad.direction().getUnitVec3f();
        MemoryUtil.memPutFloat(q + 80, normal.x());
        MemoryUtil.memPutFloat(q + 84, normal.y());
        MemoryUtil.memPutFloat(q + 88, normal.z());
        MemoryUtil.memPutInt(q + 92, quad.materialInfo().lightEmission());
        MemoryUtil.memPutInt(q + 96, color);
        run.count++;
    }

    private static void flush(Run run, ItemFeatureRenderer.Submit submit) {
        flushGpu(run, submit);
        flushCpu(run, submit);
    }

    /** The GPU's run: its vertices reserved, one job. */
    private static void flushGpu(Run run, ItemFeatureRenderer.Submit submit) {
        if (run.gpuQuads == 0) {
            return;
        }
        int vertices = run.gpuQuads * 4;
        VertexAppend v = (VertexAppend) run.builder;
        long dst = v.cursed$reserve(vertices);
        run.jobs.addItem(submit.pose(), run.gpuFirst, vertices, (int) (dst - v.cursed$base()), submit.overlayCoords(), submit.lightCoords(), submit.tintLayers());
        run.gpuQuads = 0;
    }

    /** The quad's first vertex in the GPU's mesh; the first time, the model's quads go there together. */
    private static int gpuVertex(BakedQuad quad, java.util.List<BakedQuad> quads) {
        int v = ((QuadAccess) (Object) quad).cursed$gpuVertex();
        if (v >= 0) {
            return v;
        }
        register(quads);
        return ((QuadAccess) (Object) quad).cursed$gpuVertex();
    }

    private static synchronized void register(java.util.List<BakedQuad> quads) {
        int n = 0;
        for (BakedQuad q : quads) {
            if (((QuadAccess) (Object) q).cursed$gpuVertex() < 0) {
                n++;
            }
        }
        if (n == 0) {
            return;
        }
        long mem = MemoryUtil.nmemAlloc(n * 96L);
        try {
            long at = mem;
            for (BakedQuad q : quads) {
                if (((QuadAccess) (Object) q).cursed$gpuVertex() >= 0) {
                    continue;
                }
                for (int j = 0; j < 4; j++) {
                    Vector3fc p = q.position(j);
                    long uv = q.packedUV(j);
                    MemoryUtil.memPutFloat(at, p.x());
                    MemoryUtil.memPutFloat(at + 4, p.y());
                    MemoryUtil.memPutFloat(at + 8, p.z());
                    MemoryUtil.memPutInt(at + 12, (int) (uv >>> 32));
                    MemoryUtil.memPutInt(at + 16, (int) uv);
                    at += 20;
                }
                Vector3fc normal = q.direction().getUnitVec3f();
                MemoryUtil.memPutFloat(at, normal.x());
                MemoryUtil.memPutFloat(at + 4, normal.y());
                MemoryUtil.memPutFloat(at + 8, normal.z());
                BakedQuad.MaterialInfo material = q.materialInfo();
                int info = material.lightEmission() & 15;
                if (material.isTinted()) {
                    int index = material.tintIndex();
                    info |= 16 | ((index >= 0 && index < 255 ? index : 255) << 8);
                }
                MemoryUtil.memPutInt(at + 12, info);
                at += 16;
            }
            int first = Native.modelQuadsAdd(mem, n);
            int k = 0;
            for (BakedQuad q : quads) {
                QuadAccess a = (QuadAccess) (Object) q;
                if (a.cursed$gpuVertex() < 0) {
                    a.cursed$setGpuVertex(first + 4 * k++);
                }
            }
        } finally {
            MemoryUtil.nmemFree(mem);
        }
    }

    private static void flushCpu(Run run, ItemFeatureRenderer.Submit submit) {
        if (run.count == 0) {
            return;
        }
        int vertices = run.count * 4;
        long dst = ((VertexAppend) run.builder).cursed$reserve(vertices);
        int written = Native.modelQuads(run.header, run.quads, run.count, dst);
        if (written != vertices) {
            throw new IllegalStateException("Item vertices: wrote " + written + " of " + vertices);
        }
        if (CHECK) {
            check(run, submit, dst, vertices);
        }
        run.count = 0;
    }

    /** Dev: the run's quads the game's way (putBakedQuad) into a scratch builder; the bytes must match but padding. */
    private static void check(Run run, ItemFeatureRenderer.Submit submit, long ours, int vertices) {
        if (run.scratch == null) {
            run.scratch = new ByteBufferBuilder(1 << 16);
        }
        BufferBuilder game = new BufferBuilder(run.scratch, PrimitiveTopology.QUADS, DefaultVertexFormat.ENTITY);
        QuadInstance instance = run.instance;
        instance.setLightCoords(submit.lightCoords());
        instance.setOverlayCoords(submit.overlayCoords());
        for (int i = 0; i < run.count; i++) {
            long q = run.quads + (long) QUAD * i;
            instance.setColor(MemoryUtil.memGetInt(q + 96));
            game.putBakedQuad(submit.pose(), quadOf(q), instance);
        }
        int differ = 0;
        String first = null;
        MeshData mesh = game.build();
        try {
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
        } finally {
            mesh.close();
        }
        checked.increment();
        if (differ > 0) {
            long n = mismatched.sum();
            mismatched.increment();
            if (n < 20) {
                LOGGER.warn("[items] quads differ from the game: {} vertices; first {}", differ, first);
            }
        }
        if (checked.sum() % 5000 == 0) {
            LOGGER.info("[items] checked {} runs, {} differ", checked.sum(), mismatched.sum());
        }
    }

    /** Dev: a BakedQuad back from a record (positions, UVs, direction from the normal, emission). */
    private static BakedQuad quadOf(long q) {
        org.joml.Vector3f[] p = new org.joml.Vector3f[4];
        for (int j = 0; j < 4; j++) {
            p[j] = new org.joml.Vector3f(MemoryUtil.memGetFloat(q + j * 12L), MemoryUtil.memGetFloat(q + j * 12L + 4), MemoryUtil.memGetFloat(q + j * 12L + 8));
        }
        net.minecraft.core.Direction d = net.minecraft.core.Direction.getApproximateNearest(
                MemoryUtil.memGetFloat(q + 80), MemoryUtil.memGetFloat(q + 84), MemoryUtil.memGetFloat(q + 88));
        BakedQuad.MaterialInfo info = new BakedQuad.MaterialInfo(null, null, null, null, null, -1, null, MemoryUtil.memGetInt(q + 92));
        return new BakedQuad(p[0], p[1], p[2], p[3], MemoryUtil.memGetLong(q + 48), MemoryUtil.memGetLong(q + 56), MemoryUtil.memGetLong(q + 64),
                MemoryUtil.memGetLong(q + 72), d, info);
    }
}
