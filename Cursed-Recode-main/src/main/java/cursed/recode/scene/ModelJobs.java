package cursed.recode.scene;

import com.mojang.blaze3d.vertex.PoseStack;
import java.nio.ByteBuffer;
import org.joml.Matrix3f;
import org.joml.Matrix4f;
import org.lwjgl.system.MemoryUtil;

/**
 * The GPU's share of one StagedVertexBuffer (native/src/vk/models.rs, shaders/models.comp): one 128-byte job per drawn
 * model part whose vertices the GPU makes. The part's vertices are reserved in the game's builder but left unwritten;
 * the upload (StagedVertexBufferMixin) copies everything else, tells each job where in the vertex buffer its vertices
 * land, and after the game's copy the jobs go to the frame thread, which fills them in.
 */
public final class ModelJobs {
    public static final int JOB = 128;
    private static final int FIRST = 21 * 4;
    private static final int COUNT = 22 * 4;
    /** Where the reserved vertices are in the builder's ByteBufferBuilder (bytes); jobs come in rising order. */
    private static final int SOURCE = 23 * 4;
    /** Where they go in the vertex buffer, in 4-byte words; -1 until the upload says. */
    private static final int DESTINATION = 30 * 4;
    private static final int VERTEX = 36;

    private long mem = MemoryUtil.nmemAlloc((long) JOB * 256);
    private int capacity = 256;
    public int count;

    public long address() {
        return this.mem;
    }

    public void clear() {
        this.count = 0;
    }

    /**
     * A part with a trusted pose: `vertices` vertices from `first` in the DLL's mesh, at `source` bytes (relative to
     * the model until {@link #shift}).
     */
    void add(PoseStack.Pose pose, int first, int vertices, int source, int abgr, int overlay, int light, int part, int argb) {
        long j = this.next();
        Matrix4f m = pose.pose();
        this.matrices(j, m, pose.normal());
        int props = m.properties();
        MemoryUtil.memPutInt(j + FIRST, first);
        MemoryUtil.memPutInt(j + COUNT, vertices);
        MemoryUtil.memPutInt(j + SOURCE, source);
        MemoryUtil.memPutInt(j + 96, abgr);
        MemoryUtil.memPutInt(j + 100, overlay);
        MemoryUtil.memPutInt(j + 104, light);
        // Matrix4f.PROPERTY_IDENTITY / PROPERTY_TRANSLATION: JOML's shortcuts in transformPosition.
        MemoryUtil.memPutInt(j + 108, ((props & 4) != 0 ? 1 : 0) | ((props & 8) != 0 ? 2 : 0));
        MemoryUtil.memPutInt(j + 112, part);
        MemoryUtil.memPutInt(j + 116, argb);
        MemoryUtil.memPutInt(j + DESTINATION, -1);
        MemoryUtil.memPutInt(j + 124, 0);
    }

    /**
     * A run of an item's quads with a trusted pose: `vertices` vertices from `first` in the DLL's mesh, reserved at
     * `source` bytes; the colours of tinted quads from the submit's first two layers.
     */
    void addItem(PoseStack.Pose pose, int first, int vertices, int source, int overlay, int light, int[] layers) {
        long j = this.next();
        Matrix4f m = pose.pose();
        this.matrices(j, m, pose.normal());
        int props = m.properties();
        int count = Math.min(layers.length, 2);
        MemoryUtil.memPutInt(j + FIRST, first);
        MemoryUtil.memPutInt(j + COUNT, vertices);
        MemoryUtil.memPutInt(j + SOURCE, source);
        MemoryUtil.memPutInt(j + 96, -1);
        MemoryUtil.memPutInt(j + 100, overlay);
        MemoryUtil.memPutInt(j + 104, light);
        MemoryUtil.memPutInt(j + 108, ((props & 4) != 0 ? 1 : 0) | ((props & 8) != 0 ? 2 : 0) | 4 | (count << 8));
        MemoryUtil.memPutInt(j + 112, count > 0 ? abgr(layers[0]) : -1);
        MemoryUtil.memPutInt(j + 116, count > 1 ? abgr(layers[1]) : -1);
        MemoryUtil.memPutInt(j + DESTINATION, -1);
        MemoryUtil.memPutInt(j + 124, 0);
    }

    static int abgr(int argb) {
        return (argb & 0xFF00FF00) | ((argb >> 16) & 0xFF) | ((argb & 0xFF) << 16);
    }

    /** Room for `n` more jobs; the address of the next one (count unchanged: see {@link #commit}). */
    long reserve(int n) {
        while (this.count + n > this.capacity) {
            this.capacity *= 2;
            this.mem = MemoryUtil.nmemRealloc(this.mem, (long) JOB * this.capacity);
        }
        return this.mem + (long) JOB * this.count;
    }

    /** `n` jobs were written at {@link #reserve}'s address. */
    void commit(int n) {
        this.count += n;
    }

    private long next() {
        if (this.count == this.capacity) {
            this.capacity *= 2;
            this.mem = MemoryUtil.nmemRealloc(this.mem, (long) JOB * this.capacity);
        }
        return this.mem + (long) JOB * this.count++;
    }

    private void matrices(long j, Matrix4f m, Matrix3f n) {
        MemoryUtil.memPutFloat(j, m.m00());
        MemoryUtil.memPutFloat(j + 4, m.m01());
        MemoryUtil.memPutFloat(j + 8, m.m02());
        MemoryUtil.memPutFloat(j + 12, m.m10());
        MemoryUtil.memPutFloat(j + 16, m.m11());
        MemoryUtil.memPutFloat(j + 20, m.m12());
        MemoryUtil.memPutFloat(j + 24, m.m20());
        MemoryUtil.memPutFloat(j + 28, m.m21());
        MemoryUtil.memPutFloat(j + 32, m.m22());
        MemoryUtil.memPutFloat(j + 36, m.m30());
        MemoryUtil.memPutFloat(j + 40, m.m31());
        MemoryUtil.memPutFloat(j + 44, m.m32());
        MemoryUtil.memPutFloat(j + 48, n.m00());
        MemoryUtil.memPutFloat(j + 52, n.m01());
        MemoryUtil.memPutFloat(j + 56, n.m02());
        MemoryUtil.memPutFloat(j + 60, n.m10());
        MemoryUtil.memPutFloat(j + 64, n.m11());
        MemoryUtil.memPutFloat(j + 68, n.m12());
        MemoryUtil.memPutFloat(j + 72, n.m20());
        MemoryUtil.memPutFloat(j + 76, n.m21());
        MemoryUtil.memPutFloat(j + 80, n.m22());
    }

    /** The jobs from `from` on: their vertices were reserved at `base` bytes of the ByteBufferBuilder. */
    void shift(int from, int base) {
        for (int i = from; i < this.count; i++) {
            long j = this.mem + (long) JOB * i;
            MemoryUtil.memPutInt(j + SOURCE, MemoryUtil.memGetInt(j + SOURCE) + base);
        }
    }

    /**
     * ByteBuffer.put in StagedVertexBuffer.uploadDrawsToBuffers: the slice `src` (of the ByteBufferBuilder at `base`)
     * goes to `dst` at its position — all but the jobs' vertices, which learn where they go instead (the staging
     * buffer is copied whole to the vertex buffer, so a position in it is the position in the vertex buffer).
     */
    public ByteBuffer put(ByteBuffer dst, ByteBuffer src, long base) {
        int size = src.remaining();
        long from = MemoryUtil.memAddress(src) - base;
        long srcAt = MemoryUtil.memAddress(src);
        long dstAt = MemoryUtil.memAddress(dst);
        int at = dst.position();
        int i = this.firstAtOrAfter(from);
        long done = 0;
        for (; i < this.count; i++) {
            long j = this.mem + (long) JOB * i;
            long rel = MemoryUtil.memGetInt(j + SOURCE) - from;
            if (rel >= size) {
                break;
            }
            if (rel > done) {
                MemoryUtil.memCopy(srcAt + done, dstAt + done, rel - done);
            }
            MemoryUtil.memPutInt(j + DESTINATION, (int) ((at + rel) / 4));
            done = rel + (long) MemoryUtil.memGetInt(j + COUNT) * VERTEX;
        }
        if (size > done) {
            MemoryUtil.memCopy(srcAt + done, dstAt + done, size - done);
        }
        dst.position(at + size);
        return dst;
    }

    /**
     * put for the direct upload (cursed.recode.render.DirectStaging): the slice of `size` bytes at `from` in the
     * builder goes to `at` in the vertex buffer as the GPU's pieces, all but the jobs' vertices, which learn where they go.
     */
    public void pieces(long from, int size, long at, cursed.recode.render.DirectStaging direct) {
        int i = this.firstAtOrAfter(from);
        long done = 0;
        for (; i < this.count; i++) {
            long j = this.mem + (long) JOB * i;
            long rel = MemoryUtil.memGetInt(j + SOURCE) - from;
            if (rel >= size) {
                break;
            }
            direct.piece(from + done, at + done, rel - done);
            MemoryUtil.memPutInt(j + DESTINATION, (int) ((at + rel) / 4));
            done = rel + (long) MemoryUtil.memGetInt(j + COUNT) * VERTEX;
        }
        direct.piece(from + done, at + done, size - done);
    }

    private int firstAtOrAfter(long offset) {
        int lo = 0;
        int hi = this.count;
        while (lo < hi) {
            int mid = (lo + hi) >>> 1;
            if (MemoryUtil.memGetInt(this.mem + (long) JOB * mid + SOURCE) < offset) {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        return lo;
    }

    /** Jobs the upload never reached (their vertices would stay unwritten). */
    public int unplaced() {
        int n = 0;
        for (int i = 0; i < this.count; i++) {
            if (MemoryUtil.memGetInt(this.mem + (long) JOB * i + DESTINATION) == -1) {
                n++;
            }
        }
        return n;
    }
}
