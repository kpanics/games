package cursed.recode.render;

import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.nio.charset.StandardCharsets;
import org.lwjgl.system.MemoryUtil;

/**
 * Bytes for the DLL, little-endian, in native memory that grows as needed. Strings are their UTF-8 length, the bytes and
 * zeros up to a multiple of 4 (the DLL reads them so: vk/pipeline.rs Reader). Not thread-safe: one per user.
 */
final class Pack {
    private ByteBuffer buf;

    Pack(int capacity) {
        this.buf = MemoryUtil.memAlloc(capacity).order(ByteOrder.LITTLE_ENDIAN);
    }

    void reset() {
        this.buf.clear();
    }

    long address() {
        return MemoryUtil.memAddress0(this.buf);
    }

    int length() {
        return this.buf.position();
    }

    boolean isEmpty() {
        return this.buf.position() == 0;
    }

    int remaining() {
        return this.buf.remaining();
    }

    void ensure(int bytes) {
        if (this.buf.remaining() < bytes) {
            int position = this.buf.position();
            int capacity = Math.max(this.buf.capacity() * 2, position + bytes);
            this.buf = MemoryUtil.memRealloc(this.buf, capacity).order(ByteOrder.LITTLE_ENDIAN);
            this.buf.limit(capacity).position(position);
        }
    }

    Pack i32(int v) {
        ensure(4);
        this.buf.putInt(v);
        return this;
    }

    Pack i64(long v) {
        ensure(8);
        this.buf.putLong(v);
        return this;
    }

    Pack f32(float v) {
        ensure(4);
        this.buf.putFloat(v);
        return this;
    }

    Pack f64(double v) {
        ensure(8);
        this.buf.putDouble(v);
        return this;
    }

    Pack str(String s) {
        byte[] bytes = s.getBytes(StandardCharsets.UTF_8);
        int padded = (bytes.length + 3) & ~3;
        ensure(4 + padded);
        this.buf.putInt(bytes.length);
        this.buf.put(bytes);
        for (int i = bytes.length; i < padded; i++) {
            this.buf.put((byte) 0);
        }
        return this;
    }

    /** `length` bytes from native memory at `from`, then zeros up to a multiple of 4. */
    Pack copy(long from, int length) {
        int padded = (length + 3) & ~3;
        ensure(padded);
        int at = this.buf.position();
        MemoryUtil.memCopy(from, address() + at, length);
        for (int i = length; i < padded; i++) {
            this.buf.put(at + i, (byte) 0);
        }
        this.buf.position(at + padded);
        return this;
    }

    void free() {
        MemoryUtil.memFree(this.buf);
    }
}
