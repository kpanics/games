package cursed.recode.render;

import com.mojang.renderpearl.api.buffers.GpuBuffer;
import com.mojang.renderpearl.api.buffers.GpuBufferSlice;
import com.mojang.renderpearl.api.buffers.TransientMemory;
import java.nio.ByteBuffer;
import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import org.lwjgl.system.MemoryUtil;

/**
 * Memory for one frame (vk/transient.rs). The DLL hands out the pieces; copying the game's bytes into them and packing
 * several uploads into one block happen here, as in Mojang's VulkanTransientMemory.
 */
final class CursedTransient implements TransientMemory {
    private static final int CPU = 0;
    private static final int STAGING = 1;
    private static final int GPU = 2;
    private static final int MAPPED = 3;

    private final CursedDevice device;
    private final CursedEncoder encoder;
    private final long reply;

    CursedTransient(CursedDevice device, CursedEncoder encoder) {
        this.device = device;
        this.encoder = encoder;
        this.reply = Native.reply(device.r);
    }

    private void alloc(int kind, long size, long alignment, long minimum, long element) {
        Native.transientAlloc(this.device.r, kind, size, alignment, minimum, element);
    }

    private long buffer() {
        return MemoryUtil.memGetLong(this.reply);
    }

    private long offset() {
        return MemoryUtil.memGetLong(this.reply + 8);
    }

    private long size() {
        return MemoryUtil.memGetLong(this.reply + 16);
    }

    private long host() {
        return MemoryUtil.memGetLong(this.reply + 24);
    }

    private long blockSize() {
        return MemoryUtil.memGetLong(this.reply + 32);
    }

    private GpuBufferSlice slice(@GpuBuffer.Usage int usage) {
        CursedBuffer.Transient b = new CursedBuffer.Transient(this.device, this.buffer(), usage, this.blockSize(), this.encoder.submitIndex);
        return new GpuBufferSlice(b, this.offset(), this.size());
    }

    private static final Runnable NOTHING = () -> {
    };

    @Override
    public ByteBuffer allocateCpu(long size, long alignment, long minimumAllocation, long elementSize) {
        this.alloc(CPU, size, alignment, minimumAllocation, elementSize);
        return MemoryUtil.memByteBuffer(this.host(), (int) this.size());
    }

    @Override
    public GpuBufferSlice.MappedView allocateStaging(long size, long alignment, @GpuBuffer.Usage int usage, long minimumAllocation, long elementSize) {
        this.alloc(STAGING, size, alignment, minimumAllocation, elementSize);
        long host = this.host();
        GpuBufferSlice slice = this.slice(usage);
        return new GpuBufferSlice.MappedView(slice, MemoryUtil.memByteBuffer(host, (int) slice.length()), NOTHING);
    }

    @Override
    public GpuBufferSlice allocateGpu(long size, long alignment, @GpuBuffer.Usage int usage, long minimumAllocation, long elementSize) {
        this.alloc(GPU, size, alignment, minimumAllocation, elementSize);
        return this.slice(usage);
    }

    @Override
    public GpuBufferSlice.MappedView allocateGpuMapped(long size, long alignment, @GpuBuffer.Usage int usage, long minimumAllocation, long elementSize) {
        this.alloc(MAPPED, size, alignment, minimumAllocation, elementSize);
        long host = this.host();
        GpuBufferSlice slice = this.slice(usage);
        return new GpuBufferSlice.MappedView(slice, MemoryUtil.memByteBuffer(host, (int) slice.length()), NOTHING);
    }

    private static long roundUp(long v, long alignment) {
        long a = Math.max(1L, alignment);
        return (v + a - 1) / a * a;
    }

    private GpuBufferSlice upload(List<ByteBuffer> data, long alignment, @GpuBuffer.Usage int usage, long minimumAllocation, long elementSize, boolean staging) {
        long total = 0L;
        for (ByteBuffer b : data) {
            total = roundUp(total + b.remaining(), alignment);
        }
        GpuBufferSlice.MappedView view = staging
                ? this.allocateStaging(total, alignment, usage, minimumAllocation, elementSize)
                : this.allocateGpuMapped(total, alignment, usage, minimumAllocation, elementSize);
        long to = MemoryUtil.memAddress(view.data());
        long length = view.slice().length();
        long at = 0L;
        for (ByteBuffer b : data) {
            long n = Math.min(length - at, b.remaining());
            ByteBuffer src = b;
            if (b.isDirect()) {
                MemoryUtil.memCopy(MemoryUtil.memAddress(b), to + at, n);
            } else {
                MemoryUtil.memByteBuffer(to + at, (int) n).put(src.duplicate().limit(src.position() + (int) n));
            }
            at = roundUp(at + b.remaining(), alignment);
            if (at >= length) {
                break;
            }
        }
        view.close();
        return view.slice();
    }

    @Override
    public GpuBufferSlice uploadStaging(List<ByteBuffer> data, long alignment, @GpuBuffer.Usage int usage, long minimumAllocation, long elementSize) {
        return this.upload(data, alignment, usage, minimumAllocation, elementSize, true);
    }

    @Override
    public GpuBufferSlice uploadGpu(List<ByteBuffer> data, long alignment, @GpuBuffer.Usage int usage, long minimumAllocation, long elementSize) {
        return this.upload(data, alignment, usage, minimumAllocation, elementSize, false);
    }

    @Override
    public List<GpuBufferSlice> multiUploadStaging(List<ByteBuffer> data, long alignment, @GpuBuffer.Usage int usage) {
        return this.multiUpload(data, alignment, usage, true);
    }

    @Override
    public List<GpuBufferSlice> multiUploadGpu(List<ByteBuffer> data, long alignment, @GpuBuffer.Usage int usage) {
        return this.multiUpload(data, alignment, usage, false);
    }

    /** Biggest first, each into the current block if it still fits there, so few blocks are opened (Mojang's packing). */
    private List<GpuBufferSlice> multiUpload(List<ByteBuffer> data, long alignment, @GpuBuffer.Usage int usage, boolean staging) {
        List<GpuBufferSlice> out = new ArrayList<>(Collections.nCopies(data.size(), (GpuBufferSlice) null));
        List<Integer> order = new ArrayList<>();
        for (int i = 0; i < data.size(); i++) {
            order.add(i);
        }
        order.sort((a, b) -> Integer.compare(data.get(a).remaining(), data.get(b).remaining()));
        while (!order.isEmpty()) {
            boolean placed = false;
            for (int i = order.size() - 1; i >= 0; i--) {
                int index = order.get(i);
                ByteBuffer b = data.get(index);
                if (Native.transientFits(this.device.r, !staging, b.remaining(), alignment)) {
                    order.remove(i);
                    out.set(index, this.copyInto(b, alignment, usage, staging));
                    placed = true;
                    break;
                }
            }
            if (!placed) {
                int index = order.remove(order.size() - 1);
                out.set(index, this.copyInto(data.get(index), alignment, usage, false));
            }
        }
        return out;
    }

    private GpuBufferSlice copyInto(ByteBuffer b, long alignment, @GpuBuffer.Usage int usage, boolean staging) {
        GpuBufferSlice.MappedView view = staging
                ? this.allocateStaging(b.remaining(), alignment, usage)
                : this.allocateGpuMapped(b.remaining(), alignment, usage);
        MemoryUtil.memCopy(b, view.data());
        view.close();
        return view.slice();
    }
}
