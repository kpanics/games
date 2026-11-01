package cursed.recode.render;

import com.mojang.renderpearl.api.buffers.GpuBuffer;
import com.mojang.renderpearl.api.buffers.GpuBufferSlice;
import com.mojang.renderpearl.backend.common.BaseGpuBuffer;
import org.lwjgl.system.MemoryUtil;

/** A buffer of the game: the address of its Rust struct (vk/resources.rs Buffer). */
class CursedBuffer extends BaseGpuBuffer {
    final CursedDevice device;
    final long handle;
    private boolean closed;
    private int mappings;

    CursedBuffer(CursedDevice device, long handle, @GpuBuffer.Usage int usage, long size) {
        super(usage, size);
        this.device = device;
        this.handle = handle;
    }

    static long handle(GpuBuffer buffer) {
        return ((CursedBuffer) buffer).handle;
    }

    @Override
    public boolean isClosed() {
        return this.closed;
    }

    @Override
    public void close() {
        if (!this.closed) {
            this.closed = true;
            if (this.mappings != 0) {
                throw new IllegalStateException("Attempt to close a mapped buffer");
            }
            this.device.encoder.close(0, this.handle);
        }
    }

    @Override
    public GpuBufferSlice.MappedView map(long offset, long length, boolean read, boolean write) {
        if (this.isClosed()) {
            throw new IllegalStateException("Buffer already closed");
        }
        if (!read && !write) {
            throw new IllegalArgumentException("At least read or write must be true");
        }
        if (read && (this.usage() & GpuBuffer.USAGE_MAP_READ) == 0) {
            throw new IllegalStateException("Buffer is not readable");
        }
        if (write && (this.usage() & GpuBuffer.USAGE_MAP_WRITE) == 0) {
            throw new IllegalStateException("Buffer is not writable");
        }
        if (offset + length > this.size()) {
            throw new IllegalArgumentException("Cannot map more data than this buffer can hold (attempting to map " + length + " bytes at offset " + offset + " from " + this.size() + " size buffer)");
        }
        if (length > Integer.MAX_VALUE) {
            throw new IllegalArgumentException("Mapping buffer slice larger than 2GB is not supported");
        }
        if (offset < 0L || length < 0L) {
            throw new IllegalArgumentException("Offset or length must be positive integer values");
        }
        long memory = Native.bufferMemory(this.handle);
        if (memory == 0L) {
            throw new IllegalStateException("Buffer memory is not visible to the CPU");
        }
        this.mappings++;
        // The memory stays mapped for the buffer's whole life; "unmapping" only counts.
        return new GpuBufferSlice.MappedView(this.slice(offset, length), MemoryUtil.memByteBuffer(memory + offset, (int) length), new Runnable() {
            private boolean done;

            @Override
            public void run() {
                if (!this.done) {
                    this.done = true;
                    CursedBuffer.this.mappings--;
                }
            }
        });
    }

    /** A piece of transient memory's block: valid until the end of the frame it was handed out in. */
    static final class Transient extends CursedBuffer {
        private final long submit;
        private boolean closed;

        Transient(CursedDevice device, long handle, @GpuBuffer.Usage int usage, long blockSize, long submit) {
            super(device, handle, usage, blockSize);
            this.submit = submit;
        }

        @Override
        public boolean isClosed() {
            if (!this.closed) {
                this.closed = this.submit < this.device.encoder.submitIndex;
            }
            return this.closed;
        }

        @Override
        public void close() {
            this.closed = true;
        }

        @Override
        public GpuBufferSlice.MappedView map(long offset, long length, boolean read, boolean write) {
            throw new IllegalStateException("Cannot map transient buffer");
        }

        @Override
        public GpuBufferSlice slice(long offset, long length) {
            throw new IllegalStateException("Cannot slice transient buffer");
        }

        @Override
        public GpuBufferSlice slice() {
            throw new IllegalStateException("Cannot slice transient buffer");
        }
    }
}
