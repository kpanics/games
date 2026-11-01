package cursed.recode.render;

import java.nio.ByteBuffer;
import java.util.List;
import java.util.concurrent.ConcurrentLinkedQueue;
import net.minecraft.client.renderer.chunk.CompiledSectionMesh;
import net.minecraft.client.renderer.chunk.SectionMesh;
import net.minecraft.client.renderer.chunk.SectionRenderDispatcher;
import net.minecraft.core.BlockPos;
import org.jspecify.annotations.Nullable;
import org.lwjgl.system.MemoryUtil;

/**
 * The world's terrain in the native renderer (vk/terrain.rs): the game still compiles the sections, but their geometry
 * goes to Rust instead of the game's buffers, and the frame's terrain draws are built there from the visible sections.
 * The terrain mixins call this; with Mojang's renderers (dev comparisons) it stands aside.
 */
public final class NativeTerrain {
    /** What the game does when an upload is done (its section becomes current), run on the render thread. */
    private static final ConcurrentLinkedQueue<Runnable> APPLIED = new ConcurrentLinkedQueue<>();
    private static @Nullable Pack visible;
    /**
     * Which terrain build the game is at, counted up as each begins (before it reads the sections' meshes): a mesh let
     * go of goes on the native side only once its frame thread makes a later build, as the builds recorded up to then
     * may still draw it.
     */
    private static volatile long builds;
    /** The level's projection this frame (GameRendererMixin), for the occlusion test's ProjMat · ModelViewMat. */
    public static final org.joml.Matrix4f PROJECTION = new org.joml.Matrix4f();
    private static final org.joml.Matrix4f VIEW_PROJ = new org.joml.Matrix4f();
    private static final float[] VIEW_PROJ_ARRAY = new float[16];
    private static final org.joml.Matrix4f INVERSE = new org.joml.Matrix4f();
    private static final org.joml.Vector4f EYE = new org.joml.Vector4f();
    /** The eye's place with the last terrain sent (null: none yet). */
    private static net.minecraft.world.phys.@Nullable Vec3 eye;

    private NativeTerrain() {
    }

    public static boolean active() {
        return CursedDevice.current != null;
    }

    private static long r() {
        CursedDevice d = CursedDevice.current;
        if (d == null) {
            throw new IllegalStateException("Native terrain without the Cursed device");
        }
        return d.r;
    }

    /** The native mesh of a compiled section, made on its first upload. Any thread. */
    public static int meshOf(TerrainMeshHolder holder) {
        synchronized (holder) {
            int id = holder.cursed$mesh();
            if (id == 0) {
                id = Native.terrainMesh(r());
                holder.cursed$setMesh(id);
            }
            return id;
        }
    }

    /** A layer's geometry (and its own sorted indices for translucent geometry), or new indices only. Any thread. */
    private static final ThreadLocal<ByteBuffer> FACINGS = ThreadLocal.withInitial(() -> MemoryUtil.memAlloc(cursed.recode.world.FacingHolder.COUNT * 4));

    /**
     * A layer's geometry (and its own sorted indices for translucent geometry), or new indices only; `facings` is the
     * mesh's quads per facing (3 layers x FacingHolder.COUNT) when the native mesher made it; x, y, z the section's
     * origin (the far terrain's faces draw every mesh they hold, not only those in view: vk/terrain.rs). Any thread.
     */
    public static void upload(int mesh, int layer, @Nullable ByteBuffer vertices, @Nullable ByteBuffer indices, int indexCount, boolean indexU32,
                              int @Nullable [] facings, int x, int y, int z) {
        long r = r();
        long va = vertices == null ? 0L : MemoryUtil.memAddress(vertices);
        long ia = indices == null ? 0L : MemoryUtil.memAddress(indices);
        long fa = 0L;
        if (facings != null && vertices != null) {
            ByteBuffer f = FACINGS.get();
            fa = MemoryUtil.memAddress(f);
            int count = cursed.recode.world.FacingHolder.COUNT;
            for (int k = 0; k < count; k++) {
                MemoryUtil.memPutInt(fa + k * 4L, facings[layer * count + k]);
            }
        }
        Native.terrainUpload(r, mesh, layer, va, vertices == null ? 0 : vertices.remaining(), ia, indices == null ? 0 : indices.remaining(), indexCount, indexU32, fa, x, y, z);
    }

    public static void release(TerrainMeshHolder holder) {
        int id;
        synchronized (holder) {
            id = holder.cursed$mesh();
            holder.cursed$setMesh(0);
        }
        if (id != 0 && active()) {
            Native.terrainRelease(r(), id, builds);
        }
    }

    public static void reset() {
        if (active()) {
            Native.terrainReset(r());
        }
        APPLIED.clear();
    }

    public static void afterUpload(Runnable r) {
        APPLIED.add(r);
    }

    /**
     * The frame's terrain: first what finished uploading becomes current (as the game's upload callbacks would), then the
     * visible sections go over in the game's order. Render thread.
     */
    public static void build(List<SectionRenderDispatcher.RenderSection> sections, long now, long fadeMillis, boolean ordered, org.joml.Matrix4fc modelView) {
        long number = builds + 1;
        builds = number;
        GpuShaders.ready();
        Runnable done;
        while ((done = APPLIED.poll()) != null) {
            done.run();
        }
        Pack p = visible;
        if (p == null) {
            p = visible = new Pack(64 * 1024);
        }
        p.reset();
        int count = 0;
        for (int i = 0, n = sections.size(); i < n; i++) {
            SectionRenderDispatcher.RenderSection section = sections.get(i);
            SectionMirror mirror = (SectionMirror) (Object) section;
            int id = mirror.cursed$drawId();
            if (id == 0) {
                continue;
            }
            p.i32(id).i32(mirror.cursed$originX()).i32(mirror.cursed$originY()).i32(mirror.cursed$originZ()).f32(section.getVisibility(now, fadeMillis));
            count++;
        }
        if ((number & 8191) == 0 && cursed.recode.dev.Dev.on("CURSED_DEV_FPS")) {
            cursed.recode.CursedRecode.LOGGER.info("[terrain] sections listed {}, with a mesh {}", sections.size(), count);
        }
        CursedDevice d = CursedDevice.current;
        if (d != null) {
            net.minecraft.world.phys.Vec3 camera = net.minecraft.client.Minecraft.getInstance().gameRenderer.mainCamera().position();
            // The terrain shader's gl_Position = ProjMat * ModelViewMat * position relative to the camera.
            VIEW_PROJ.set(PROJECTION).mul(modelView).get(VIEW_PROJ_ARRAY);
            d.encoder.terrain(p, count, ordered, number, camera.x, camera.y, camera.z, VIEW_PROJ_ARRAY);
            // the eye: what the projection sends to (0, 0, z, 0) — off the camera's place by the view's bob
            VIEW_PROJ.invert(INVERSE).transform(EYE.set(0.0F, 0.0F, 1.0F, 0.0F));
            eye = Math.abs(EYE.w) > 1e-12F ? camera.add(EYE.x / EYE.w, EYE.y / EYE.w, EYE.z / EYE.w) : camera;
        }
    }

    /**
     * Where the eye is (the camera's place moved by the view's bob, which the game puts in the projection:
     * GameRenderer.bobView): the place the effects, the hitboxes and Target ESP give their points from, as the engine
     * casts their rays from it (engine/gpu.rs from_eye). `camera` before any terrain went.
     */
    public static net.minecraft.world.phys.Vec3 eye(net.minecraft.world.phys.Vec3 camera) {
        net.minecraft.world.phys.Vec3 e = eye;
        return e != null ? e : camera;
    }

    /**
     * The translucent layer drawn now is only its sections from vk/terrain.rs FAR_WATER out (layer 3 natively): with
     * Improved Transparency they go the classic way in the opaque pass (LevelRendererCloudsMixin), the rest through
     * the transparency's passes.
     */
    public static boolean farTranslucent;

    public static void drawLayer(int layer) {
        CursedPass pass = CursedPass.instance;
        if (pass == null) {
            throw new IllegalStateException("Terrain draw outside a Cursed render pass");
        }
        pass.terrain(farTranslucent && layer == 2 ? 3 : layer);
    }
}
