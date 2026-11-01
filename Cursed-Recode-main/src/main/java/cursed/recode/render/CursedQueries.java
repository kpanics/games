package cursed.recode.render;

import com.mojang.renderpearl.api.commands.GpuQueryPool;
import java.util.OptionalLong;
import org.lwjgl.system.MemoryStack;

/** A pool of GPU timestamps (vk/resources.rs Queries). */
final class CursedQueries implements GpuQueryPool {
    private final CursedDevice device;
    final long handle;
    private final int size;

    CursedQueries(CursedDevice device, int size) {
        this.device = device;
        this.size = size;
        this.handle = Native.queriesCreate(device.r, size);
    }

    static long handle(GpuQueryPool pool) {
        return ((CursedQueries) pool).handle;
    }

    @Override
    public int size() {
        return this.size;
    }

    @Override
    public OptionalLong getValue(int index) {
        return this.getValues(index, 1)[0];
    }

    @Override
    public OptionalLong[] getValues(int index, int count) {
        if (index + count > this.size) {
            throw new IndexOutOfBoundsException("getValues would read out-of-bounds for an array of " + count + " starting at " + index + ", when total size is " + this.size);
        }
        OptionalLong[] values = new OptionalLong[count];
        try (MemoryStack stack = MemoryStack.stackPush()) {
            long out = stack.ncalloc(8, count * 2, 8);
            Native.queriesRead(this.device.r, this.handle, index, count, out);
            for (int i = 0; i < count; i++) {
                long value = org.lwjgl.system.MemoryUtil.memGetLong(out + i * 16L);
                long ready = org.lwjgl.system.MemoryUtil.memGetLong(out + i * 16L + 8);
                values[i] = ready != 0 ? OptionalLong.of(value) : OptionalLong.empty();
            }
        }
        return values;
    }

    @Override
    public void close() {
        this.device.encoder.close(4, this.handle);
    }
}
