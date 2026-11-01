package cursed.recode.mixin.client;

import com.mojang.blaze3d.vertex.BufferBuilder;
import com.mojang.blaze3d.vertex.ByteBufferBuilder;
import com.mojang.blaze3d.vertex.CompactVectorArray;
import com.mojang.blaze3d.vertex.DefaultVertexFormat;
import com.mojang.blaze3d.vertex.MeshData;
import com.mojang.blaze3d.vertex.VertexConsumer;
import com.mojang.blaze3d.vertex.VertexSorting;
import com.mojang.renderpearl.api.buffers.GpuBuffer;
import com.mojang.renderpearl.api.device.GpuDevice;
import cursed.recode.dev.FeatureTimes;
import cursed.recode.render.GpuModels;
import cursed.recode.scene.ModelJobs;
import cursed.recode.scene.QuadSort;
import cursed.recode.world.VertexAppend;
import java.nio.ByteBuffer;
import java.util.List;
import net.minecraft.client.renderer.StagedVertexBuffer;
import org.jspecify.annotations.Nullable;
import org.spongepowered.asm.mixin.Final;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.Unique;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.Redirect;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/**
 * Entity models' vertices made by the GPU (cursed.recode.scene.ModelJobs): builders of unsorted entity-format draws
 * carry this buffer's jobs; the upload copies all but the jobs' vertices and then hands the jobs to the frame thread.
 * Translucent quads ordered by the DLL (cursed.recode.scene.QuadSort). Also the dev timing of the upload (CURSED_DEV_BENCH).
 */
@Mixin(StagedVertexBuffer.class)
public abstract class StagedVertexBufferMixin {
    @Shadow
    @Final
    private ByteBufferBuilder stagingBuffer;
    @Shadow
    private @Nullable BufferBuilder lastVertexBuilder;
    @Unique
    private final ModelJobs cursed$jobs = new ModelJobs();

    /** A new builder for a draw (not every call: getVertexBuilder runs per quad). */
    @Inject(method = "getVertexBuilder", at = @At(value = "FIELD", target = "Lnet/minecraft/client/renderer/StagedVertexBuffer;lastVertexBuilder:Lcom/mojang/blaze3d/vertex/BufferBuilder;",
            opcode = org.objectweb.asm.Opcodes.PUTFIELD, shift = At.Shift.AFTER))
    private void cursed$jobs(StagedVertexBuffer.Draw draw, CallbackInfoReturnable<VertexConsumer> cir) {
        BufferBuilder builder = this.lastVertexBuilder;
        if (builder != null) {
            // Sorted draws read their vertices back on the CPU to order the quads; those keep the CPU's vertices.
            boolean gpu = draw.quadSorting == null && draw.format == DefaultVertexFormat.ENTITY && GpuModels.on();
            ((VertexAppend) builder).cursed$setJobs(gpu ? this.cursed$jobs : null);
        }
    }

    /** The builder's memory where the GPU copies from (cursed.recode.render.DirectStaging); made with the first upload. */
    @Unique
    private cursed.recode.render.@Nullable DirectStaging cursed$direct;

    /**
     * The upload with the builder in the GPU's memory: the vertices go as one copy command of the GPU's (a piece per
     * slice, the jobs' vertices left out as in cursed$put), the sorted indices are written after them in the same
     * memory and go the same way; then the builder moves on to another slot. Otherwise the game's upload, and the
     * builder moves into the GPU's memory after it.
     */
    @Inject(method = "uploadDrawsToBuffers", at = @At("HEAD"), cancellable = true)
    private void cursed$direct(GpuDevice device, List<StagedVertexBuffer.Draw> draws, GpuBuffer vertexGpuBuffer, @Nullable GpuBuffer indexGpuBuffer,
                               int vertexBufferSize, int indexBufferSize, CallbackInfo ci) {
        cursed.recode.render.DirectStaging direct = this.cursed$direct;
        if (direct == null || !direct.on()) {
            return;
        }
        ci.cancel();
        long indexAt = indexGpuBuffer != null && indexBufferSize > 0 ? direct.tail(indexBufferSize) : -1;
        long base = direct.address();
        for (StagedVertexBuffer.Draw draw : draws) {
            if (draw.isEmpty()) {
                continue;
            }
            long at = ((StagedDrawAccessor) draw).cursed$vertexOffset();
            for (ByteBufferBuilder.Result slice : draw.vertexBufferSlices) {
                ByteBuffer bytes = slice.byteBuffer();
                long from = org.lwjgl.system.MemoryUtil.memAddress(bytes) - base;
                int size = bytes.remaining();
                if (this.cursed$jobs.count > 0) {
                    this.cursed$jobs.pieces(from, size, at, direct);
                } else {
                    direct.piece(from, at, size);
                }
                at += size;
            }
        }
        direct.copy(vertexGpuBuffer);
        if (indexAt >= 0) {
            ByteBuffer indices = org.lwjgl.system.MemoryUtil.memByteBuffer(base + indexAt, indexBufferSize);
            for (StagedVertexBuffer.Draw draw : draws) {
                if (!draw.isEmpty() && draw.quadSorting != null) {
                    indices.position(((StagedDrawAccessor) draw).cursed$indexOffset());
                    com.mojang.renderpearl.api.pipeline.IndexType type = com.mojang.renderpearl.api.pipeline.IndexType.least(draw.vertexCount);
                    if (QuadSort.takes(draw)) {
                        QuadSort.write(draw, indices, type.bytes, QuadSort.CHECKING ? decodeSortingPoints(draw) : null);
                    } else {
                        new MeshData.SortState(decodeSortingPoints(draw), type).writeSortedIndexBuffer(indices, draw.quadSorting);
                    }
                }
            }
            direct.piece(indexAt, 0, indexBufferSize);
            direct.copy(indexGpuBuffer);
        }
        for (StagedVertexBuffer.Draw draw : draws) {
            if (!draw.isEmpty()) {
                draw.vertexBufferSlices.forEach(ByteBufferBuilder.Result::close);
                draw.vertexBufferSlices.clear();
            }
        }
        if (this.cursed$jobs.count > 0) {
            GpuModels.dispatch(this.cursed$jobs, vertexGpuBuffer);
            this.cursed$jobs.clear();
        }
        direct.next();
    }

    /** After the game's upload: the builder moves into the GPU's memory for the next one. */
    @Inject(method = "uploadDrawsToBuffers", at = @At("TAIL"))
    private void cursed$toDirect(GpuDevice device, List<StagedVertexBuffer.Draw> draws, GpuBuffer vertexGpuBuffer, @Nullable GpuBuffer indexGpuBuffer,
                                 int vertexBufferSize, int indexBufferSize, CallbackInfo ci) {
        if (this.cursed$direct == null) {
            this.cursed$direct = new cursed.recode.render.DirectStaging(this.stagingBuffer);
        }
        this.cursed$direct.next();
    }

    @Redirect(method = "uploadDrawsToBuffers", at = @At(value = "INVOKE", target = "Ljava/nio/ByteBuffer;put(Ljava/nio/ByteBuffer;)Ljava/nio/ByteBuffer;"))
    private ByteBuffer cursed$put(ByteBuffer dst, ByteBuffer src) {
        if (this.cursed$jobs.count == 0) {
            return dst.put(src);
        }
        return this.cursed$jobs.put(dst, src, this.stagingBuffer.pointer);
    }

    @Inject(method = "uploadDrawsToBuffers", at = @At("TAIL"))
    private void cursed$dispatch(GpuDevice device, List<StagedVertexBuffer.Draw> draws, GpuBuffer vertexGpuBuffer, @Nullable GpuBuffer indexGpuBuffer,
                                 int vertexBufferSize, int indexBufferSize, CallbackInfo ci) {
        if (this.cursed$jobs.count > 0) {
            GpuModels.dispatch(this.cursed$jobs, vertexGpuBuffer);
            this.cursed$jobs.clear();
        }
    }

    /** The draw whose quads the DLL orders next (cursed.recode.scene.QuadSort), between the two redirects below. */
    @Unique
    private StagedVertexBuffer.@Nullable Draw cursed$sorting;
    @Unique
    private @Nullable CompactVectorArray cursed$points;

    @Shadow
    private static CompactVectorArray decodeSortingPoints(StagedVertexBuffer.Draw draw) {
        throw new AssertionError();
    }

    /** No centres decoded in Java for a draw the DLL orders (it reads the vertices itself). */
    @Redirect(method = "uploadDrawsToBuffers", at = @At(value = "INVOKE",
            target = "Lnet/minecraft/client/renderer/StagedVertexBuffer;decodeSortingPoints(Lnet/minecraft/client/renderer/StagedVertexBuffer$Draw;)Lcom/mojang/blaze3d/vertex/CompactVectorArray;"))
    private CompactVectorArray cursed$points(StagedVertexBuffer.Draw draw) {
        if (QuadSort.takes(draw)) {
            this.cursed$sorting = draw;
            this.cursed$points = QuadSort.CHECKING ? decodeSortingPoints(draw) : null;
            return QuadSort.NONE;
        }
        this.cursed$sorting = null;
        return decodeSortingPoints(draw);
    }

    @Redirect(method = "uploadDrawsToBuffers", at = @At(value = "INVOKE",
            target = "Lcom/mojang/blaze3d/vertex/MeshData$SortState;writeSortedIndexBuffer(Ljava/nio/ByteBuffer;Lcom/mojang/blaze3d/vertex/VertexSorting;)V"))
    private void cursed$sort(MeshData.SortState state, ByteBuffer target, VertexSorting sorting) {
        StagedVertexBuffer.Draw draw = this.cursed$sorting;
        if (draw == null) {
            state.writeSortedIndexBuffer(target, sorting);
            return;
        }
        this.cursed$sorting = null;
        QuadSort.write(draw, target, state.indexType().bytes, this.cursed$points);
        this.cursed$points = null;
    }

    /**
     * A run of glyphs still waiting (cursed.recode.scene.NativeText) goes into its builder before the builder is
     * finished: the game asks for the next draw's builder (a text display's background, another render type) before
     * it hands that renderable over, and asking finishes the last one.
     */
    @Inject(method = "finishLastVertexBuilder", at = @At("HEAD"))
    private void cursed$glyphsFirst(CallbackInfo ci) {
        cursed.recode.scene.NativeText.flush();
    }

    @Inject(method = "endDraw", at = @At("HEAD"))
    private void cursed$endDraw(CallbackInfo ci) {
        this.cursed$jobs.clear();
    }

    @Inject(method = "upload", at = @At("HEAD"))
    private void cursed$start(CallbackInfo ci) {
        FeatureTimes.start("staged upload");
        if (cursed.recode.dev.Dev.on("CURSED_DEV_BENCH")) {
            for (StagedVertexBuffer.Draw d : this.draws) {
                FeatureTimes.count("vertices " + cursed.recode.dev.Formats.name(d.format) + (d.quadSorting != null ? " sorted" : ""), (long) d.vertexCount * d.format.getVertexSize());
            }
        }
    }

    @Shadow @Final private List<StagedVertexBuffer.Draw> draws;


    @Inject(method = "upload", at = @At("TAIL"))
    private void cursed$stop(CallbackInfo ci) {
        FeatureTimes.stop("staged upload");
    }
}
