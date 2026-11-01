package cursed.recode.render;

import com.mojang.blaze3d.systems.RenderSystem;
import com.mojang.blaze3d.vertex.ByteBufferBuilder;
import com.mojang.logging.LogUtils;
import com.mojang.renderpearl.api.buffers.GpuBuffer;
import com.mojang.renderpearl.api.commands.GpuFence;
import cursed.recode.dev.Dev;
import java.util.ArrayDeque;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;
import org.jspecify.annotations.Nullable;
import org.lwjgl.system.MemoryUtil;
import org.slf4j.Logger;

/**
 * A StagedVertexBuffer's vertices written where the GPU copies them from. The game builds a frame's vertices in its
 * own memory and, at the upload, the CPU copies them all into a staging buffer for the GPU (in a crowd some 2 MB a
 * frame, an eighth of the render thread). Here the builder's memory is itself a mapped buffer of ours (the host's
 * cached memory: the CPU writes and reads it as fast as its own), and the upload hands the GPU one command with a piece
 * per slice: no copy on the CPU.
 *
 * The GPU reads a slot when the frame runs, so after an upload the builder goes on after it in the same slot while
 * there is room (a screen of new items uploads a hundred times in a frame: a slot each was 140 slots of 4 MB made in
 * that frame, 22.10), else in another slot; a slot is taken again once the fence made when it was left has passed.
 * Growing the builder past its slot takes a bigger one (ByteBufferBuilderMixin); closing it gives it its own memory
 * back. Without cached memory, or a slot, the builder keeps the game's way. CURSED_DEV_NOSTAGINGDIRECT: the game's copy.
 */
public final class DirectStaging {
    private static final Logger LOGGER = LogUtils.getLogger();
    private static final boolean OFF = Dev.on("CURSED_DEV_NOSTAGINGDIRECT");
    private static final boolean STALLS = Dev.on("CURSED_DEV_STALLS");
    /** CURSED_DEV_STAGINGFAIL=n: no slot can be had after n uploads (the way back to the game's memory, tested). */
    private static final int FAIL_AFTER = Dev.on("CURSED_DEV_STAGINGFAIL") ? Integer.parseInt(System.getenv("CURSED_DEV_STAGINGFAIL")) : -1;
    private static int takes;
    private static final int USAGE = GpuBuffer.USAGE_MAP_READ | GpuBuffer.USAGE_MAP_WRITE | GpuBuffer.USAGE_COPY_SRC;
    private static final long STEP = 1L << 20;
    /** A new slot's least size: room for the small uploads of a frame one after another. */
    private static final long LEAST = 4L << 20;
    private static final int SPARE = 3;
    private static boolean failedSaid;

    /** The builder's memory (ByteBufferBuilderMixin). */
    public interface Builder {
        long cursed$pointer();

        long cursed$capacity();

        long cursed$writeOffset();

        void cursed$memory(long pointer, long capacity);

        @Nullable DirectStaging cursed$direct();

        void cursed$direct(@Nullable DirectStaging direct);
    }

    private record Slot(GpuBuffer buffer, long address, long size) {
    }

    private record Left(Slot slot, GpuFence fence) {
    }

    private final Builder builder;
    private final ArrayDeque<Left> left = new ArrayDeque<>();
    private final List<Slot> spare = new ArrayList<>();
    private @Nullable Slot current;
    /** Where in the current slot the builder's memory starts (the uploads before it in this slot are before it), the
     *  end of what the uploads put in it so far, the most one upload took lately (bytes). */
    private long start;
    private long high;
    private long peak;
    /** The builder's own memory, while it writes into ours. */
    private long ownPointer;
    private long ownCapacity;
    private boolean failed;
    private long[] regions = new long[3 * 64];
    private int count;

    public DirectStaging(ByteBufferBuilder builder) {
        this.builder = (Builder) builder;
        this.builder.cursed$direct(this);
    }

    /** The builder writes into a slot of ours (the upload goes the direct way). */
    public boolean on() {
        return this.current != null;
    }

    /** Where the builder's memory starts: the upload's offsets are from here (the jobs' too, ModelJobs). */
    public long address() {
        return this.current.address + this.start;
    }

    public GpuBuffer buffer() {
        return this.current.buffer;
    }

    /**
     * After an upload (the GPU copies from the slot when the frame runs) the builder goes on after it in the slot while
     * there is room, else in another slot; the first time, it leaves its own memory. What it holds past its results
     * (nothing, as a rule) goes along.
     */
    public void next() {
        if (OFF || this.failed || !RenderSystem.isOnRenderThread()) {
            return;
        }
        long keep = this.builder.cursed$writeOffset();
        if (this.current != null) {
            // what this upload took; the builder goes on after it in the same slot while that leaves it room for twice
            // as much as an upload takes lately
            this.peak = Math.max(this.peak - (this.peak >> 4), Math.max(this.high - this.start, keep));
            long at = (Math.max(this.high, this.start + keep) + 255) & ~255L;
            long room = this.current.size - at;
            if (room >= Math.max(2 * this.peak, 64L << 10) && room > keep) {
                if (keep > 0) {
                    MemoryUtil.memCopy(this.current.address + this.start, this.current.address + at, keep);
                }
                this.start = at;
                this.high = at + keep;
                this.builder.cursed$memory(this.current.address + at, room);
                return;
            }
        }
        Slot slot = this.take(Math.max(Math.max(this.builder.cursed$capacity(), keep), 4 * this.peak));
        if (slot == null) {
            if (this.current != null) {
                // the GPU still reads this slot: the builder goes back to its own memory
                long capacity = Math.max(this.ownCapacity, keep);
                long own = MemoryUtil.nmemRealloc(this.ownPointer, capacity);
                if (own == 0L) {
                    throw new OutOfMemoryError("Failed to give the vertex buffer " + capacity + " bytes");
                }
                if (keep > 0) {
                    MemoryUtil.memCopy(this.current.address + this.start, own, keep);
                }
                this.builder.cursed$memory(own, capacity);
                this.left.add(new Left(this.current, RenderSystem.getDevice().createCommandEncoder().createFence()));
                this.current = null;
            }
            return;
        }
        long from = this.builder.cursed$pointer();
        if (keep > 0) {
            MemoryUtil.memCopy(from, slot.address, keep);
        }
        if (this.current == null) {
            this.ownPointer = from;
            this.ownCapacity = this.builder.cursed$capacity();
        } else {
            this.left.add(new Left(this.current, RenderSystem.getDevice().createCommandEncoder().createFence()));
        }
        this.current = slot;
        this.start = 0;
        this.high = keep;
        this.builder.cursed$memory(slot.address, slot.size);
    }

    /** ByteBufferBuilder.resize while it writes into a slot: a bigger slot with what it has so far. */
    public void grow(long capacity) {
        Slot old = this.current;
        long keep = this.builder.cursed$writeOffset();
        Slot slot = this.failed ? null : this.take(capacity);
        if (slot == null) {
            // no slot: back to its own memory, grown, for good
            this.failed = true;
            long own = MemoryUtil.nmemRealloc(this.ownPointer, capacity);
            if (own == 0L) {
                throw new OutOfMemoryError("Failed to grow the vertex buffer to " + capacity + " bytes");
            }
            MemoryUtil.memCopy(old.address + this.start, own, keep);
            this.builder.cursed$memory(own, capacity);
            this.current = null;
            this.leave(old);
            return;
        }
        MemoryUtil.memCopy(old.address + this.start, slot.address, keep);
        this.current = slot;
        this.builder.cursed$memory(slot.address, slot.size);
        this.leave(old);
        this.start = 0;
        this.high = keep;
    }

    /** A slot the builder leaves while it grows: free again at once when nothing went from it yet, else once the GPU
     *  has copied this frame's uploads out of it. */
    private void leave(Slot old) {
        if (this.start == 0) {
            this.retire(old);
        } else {
            this.left.add(new Left(old, RenderSystem.getDevice().createCommandEncoder().createFence()));
        }
    }

    /** Room after the builder's data for `bytes` more (the upload's sorted indices): its offset from address(). */
    public long tail(int bytes) {
        long at = (this.builder.cursed$writeOffset() + 15) & ~15L;
        if (this.start + at + bytes > this.current.size) {
            this.grow(at + bytes);
            if (this.current == null) {
                throw new IllegalStateException("No slot for the indices");
            }
            at = (this.builder.cursed$writeOffset() + 15) & ~15L;
        }
        return at;
    }

    /** The upload's pieces: from source (from address()), to target in the destination, size bytes. */
    public void piece(long source, long target, long size) {
        if (size <= 0) {
            return;
        }
        source += this.start;
        this.high = Math.max(this.high, source + size);
        int n = this.count * 3;
        // right after the last piece on both sides: the same piece
        if (n > 0 && this.regions[n - 3] + this.regions[n - 1] == source && this.regions[n - 2] + this.regions[n - 1] == target) {
            this.regions[n - 1] += size;
            return;
        }
        if (n + 3 > this.regions.length) {
            this.regions = Arrays.copyOf(this.regions, this.regions.length * 2);
        }
        this.regions[n] = source;
        this.regions[n + 1] = target;
        this.regions[n + 2] = size;
        this.count++;
    }

    /** The pieces so far, copied by the GPU from the slot into target. */
    public void copy(GpuBuffer target) {
        CursedDevice.current.encoder.copyRegions(this.current.buffer, target, this.regions, this.count);
        this.count = 0;
    }

    /** The builder closes: its own memory back (the game frees that), our slots closed. */
    public void close() {
        if (this.current != null) {
            this.builder.cursed$memory(this.ownPointer, this.ownCapacity);
            this.current.buffer.close();
            this.current = null;
        }
        for (Left l : this.left) {
            l.slot.buffer.close();
        }
        this.left.clear();
        for (Slot s : this.spare) {
            s.buffer.close();
        }
        this.spare.clear();
        this.builder.cursed$direct(null);
    }

    private void retire(Slot slot) {
        if (this.spare.size() < SPARE) {
            this.spare.add(slot);
        } else {
            slot.buffer.close();
        }
    }

    private @Nullable Slot take(long size) {
        if (FAIL_AFTER >= 0 && ++takes > FAIL_AFTER) {
            this.fail("dev: CURSED_DEV_STAGINGFAIL");
            return null;
        }
        while (!this.left.isEmpty() && this.left.peek().fence.awaitCompletion(0L)) {
            Left l = this.left.poll();
            l.fence.close();
            this.retire(l.slot);
        }
        Slot best = null;
        for (Slot s : this.spare) {
            if (s.size >= size && (best == null || s.size < best.size)) {
                best = s;
            }
        }
        if (best != null) {
            this.spare.remove(best);
            return best;
        }
        long rounded = (Math.max(size, LEAST) + STEP - 1) / STEP * STEP;
        try {
            long t0 = System.nanoTime();
            GpuBuffer buffer = RenderSystem.getDevice().createBuffer(() -> "Cursed vertices", USAGE, rounded);
            if (STALLS) {
                LOGGER.info("[staging] a slot of {} KiB for {} KiB made in {} us ({} spare, {} left)", rounded >> 10, size >> 10, (System.nanoTime() - t0) / 1000, this.spare.size(), this.left.size());
            }
            long handle = CursedBuffer.handle(buffer);
            long address = Native.bufferMemory(handle);
            if (address == 0L || !Native.bufferCached(handle)) {
                buffer.close();
                this.fail("the host has no cached memory for it");
                return null;
            }
            return new Slot(buffer, address, rounded);
        } catch (RuntimeException e) {
            this.fail(e.toString());
            return null;
        }
    }

    private void fail(String why) {
        this.failed = true;
        if (!failedSaid) {
            failedSaid = true;
            LOGGER.warn("[staging] vertices go the game's way: {}", why);
        }
    }
}
