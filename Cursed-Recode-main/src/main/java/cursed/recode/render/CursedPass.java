package cursed.recode.render;

import com.mojang.renderpearl.api.buffers.GpuBuffer;
import com.mojang.renderpearl.api.buffers.GpuBufferSlice;
import com.mojang.renderpearl.api.commands.GpuQueryPool;
import com.mojang.renderpearl.api.pipeline.IndexType;
import com.mojang.renderpearl.backend.api.BackendRenderPipeline;
import com.mojang.renderpearl.backend.api.RenderPassBackend;
import com.mojang.renderpearl.util.TextureViewAndSampler;
import java.nio.ByteBuffer;
import java.nio.IntBuffer;
import java.util.function.Supplier;
import org.jspecify.annotations.Nullable;
import org.lwjgl.PointerBuffer;
import org.lwjgl.system.MemoryUtil;

/**
 * The calls of a render pass, written into the frame's stream (CursedEncoder) for the DLL's frame thread; the list of
 * calls and their bytes is in vk/pass.rs. One object, reused for every pass.
 */
final class CursedPass implements RenderPassBackend {
    private static final int PIPELINE = 1;
    private static final int UNIFORM = 2;
    private static final int PUSH = 3;
    private static final int SCISSOR = 4;
    private static final int NO_SCISSOR = 5;
    private static final int VERTICES = 6;
    private static final int INDICES = 7;
    private static final int DRAW_INDEXED = 8;
    private static final int DRAW = 9;
    private static final int MULTI_DRAW_INDEXED = 10;
    private static final int MULTI_DRAW = 11;
    private static final int DRAW_INDEXED_INDIRECT = 12;
    private static final int DRAW_INDIRECT = 13;
    private static final int PUSH_GROUP = 14;
    private static final int POP_GROUP = 15;
    private static final int TIMESTAMP = 16;
    private static final int TERRAIN = 17;

    private final CursedDevice device;
    private final CursedEncoder encoder;
    private int groups;
    /** The pass object of the device (there is one, reused). */
    static @Nullable CursedPass instance;

    CursedPass(CursedDevice device, CursedEncoder encoder) {
        this.device = device;
        this.encoder = encoder;
        instance = this;
    }

    /** The frame's terrain of a layer (NativeTerrain), drawn with the pipeline set just before. */
    void terrain(int layer) {
        this.call(TERRAIN, 4).i32(layer);
    }

    void begin() {
        this.groups = 0;
    }

    private Pack call(int op, int bytes) {
        return this.encoder.op(op, bytes);
    }

    @Override
    public void pushDebugGroup(Supplier<String> label) {
        this.groups++;
        if (this.device.debug) {
            String text = label.get();
            this.call(PUSH_GROUP, 8 + text.length() * 3).str(text);
        }
    }

    @Override
    public void popDebugGroup() {
        if (this.groups == 0) {
            throw new IllegalStateException("Can't pop more debug groups than was pushed!");
        }
        this.groups--;
        if (this.device.debug) {
            this.call(POP_GROUP, 0);
        }
    }

    @Override
    public void setPipeline(BackendRenderPipeline pipeline) {
        if (!(pipeline instanceof CursedPipeline)) {
            throw new IllegalArgumentException("Pipeline must be instance of CursedPipeline");
        }
        this.call(PIPELINE, 8).i64(CursedPipeline.handle(pipeline));
    }

    @Override
    public void setUniform(int index, @Nullable Object value) {
        Pack p = this.call(UNIFORM, 32).i32(index);
        if (value == null) {
            p.i32(0);
        } else if (value instanceof GpuBufferSlice slice) {
            p.i32(1).i64(CursedBuffer.handle(slice.buffer())).i64(slice.offset()).i64(slice.length());
        } else if (value instanceof TextureViewAndSampler image) {
            p.i32(2).i64(CursedView.handle(image.view())).i64(((CursedSampler) image.sampler()).handle);
        } else {
            throw new IllegalArgumentException("Unknown uniform value " + value.getClass());
        }
    }

    @Override
    public void pushConstants(ByteBuffer value) {
        int n = value.remaining();
        Pack p = this.call(PUSH, 8 + n).i32(n);
        if (value.isDirect()) {
            p.copy(MemoryUtil.memAddress(value), n);
        } else {
            CursedEncoder.withAddress(value, address -> p.copy(address, n));
        }
    }

    @Override
    public void enableScissor(int x, int y, int width, int height) {
        this.call(SCISSOR, 16).i32(x).i32(y).i32(width).i32(height);
    }

    @Override
    public void disableScissor() {
        this.call(NO_SCISSOR, 0);
    }

    @Override
    public void setVertexBuffer(int slot, @Nullable GpuBufferSlice vertexBuffer) {
        if (vertexBuffer != null) {
            this.call(VERTICES, 20).i32(slot).i64(CursedBuffer.handle(vertexBuffer.buffer())).i64(vertexBuffer.offset());
        }
    }

    @Override
    public void setIndexBuffer(GpuBuffer indexBuffer, IndexType indexType) {
        this.call(INDICES, 12).i64(CursedBuffer.handle(indexBuffer)).i32(indexType == IndexType.SHORT ? 0 : 1);
    }

    @Override
    public void drawIndexed(int indexCount, int instanceCount, int firstIndex, int vertexOffset, int firstInstance) {
        this.call(DRAW_INDEXED, 20).i32(indexCount).i32(instanceCount).i32(firstIndex).i32(vertexOffset).i32(firstInstance);
    }

    @Override
    public void multiDrawIndexed(IntBuffer drawParameters, int instanceCount, int firstInstance, int drawCount) {
        int bytes = drawCount * 12;
        this.call(MULTI_DRAW_INDEXED, 12 + bytes).i32(instanceCount).i32(firstInstance).i32(drawCount).copy(MemoryUtil.memAddress(drawParameters), bytes);
    }

    @Override
    public void multiDrawIndexed(PointerBuffer firstIndexOffsets, IntBuffer indexCounts, IntBuffer vertexOffsets, int drawCount) {
        throw new UnsupportedOperationException("Vulkan does not support the multiDrawDirectSeparate device feature");
    }

    @Override
    public void drawIndexedIndirect(GpuBufferSlice commands, int drawCount) {
        this.call(DRAW_INDEXED_INDIRECT, 20).i64(CursedBuffer.handle(commands.buffer())).i64(commands.offset()).i32(drawCount);
    }

    @Override
    public void draw(int vertexCount, int instanceCount, int firstVertex, int firstInstance) {
        this.call(DRAW, 16).i32(vertexCount).i32(instanceCount).i32(firstVertex).i32(firstInstance);
    }

    @Override
    public void multiDraw(IntBuffer drawParameters, int instanceCount, int firstInstance, int drawCount) {
        int bytes = drawCount * 8;
        this.call(MULTI_DRAW, 12 + bytes).i32(instanceCount).i32(firstInstance).i32(drawCount).copy(MemoryUtil.memAddress(drawParameters), bytes);
    }

    @Override
    public void multiDraw(IntBuffer firstVertices, IntBuffer vertexCounts, int drawCount) {
        throw new UnsupportedOperationException("Vulkan does not support the multiDrawDirectSeparate device feature");
    }

    @Override
    public void drawIndirect(GpuBufferSlice commands, int drawCount) {
        this.call(DRAW_INDIRECT, 20).i64(CursedBuffer.handle(commands.buffer())).i64(commands.offset()).i32(drawCount);
    }

    @Override
    public void writeTimestamp(GpuQueryPool pool, int index) {
        this.call(TIMESTAMP, 12).i64(CursedQueries.handle(pool)).i32(index);
    }
}
