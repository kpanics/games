package cursed.recode.mixin.client;

import com.mojang.blaze3d.vertex.BufferBuilder;
import com.mojang.blaze3d.vertex.ByteBufferBuilder;
import cursed.recode.world.VertexAppend;
import org.spongepowered.asm.mixin.Final;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.Unique;

/** Whole vertices from the native mesher, appended after what the builder already has (fluids). */
@Mixin(BufferBuilder.class)
public abstract class BufferBuilderMixin implements VertexAppend {
    @Shadow
    @Final
    private ByteBufferBuilder buffer;
    @Shadow
    private int vertices;
    @Shadow
    @Final
    private int vertexSize;
    @Shadow
    @Final
    private boolean entityFormat;
    @Shadow
    @Final
    private com.mojang.renderpearl.api.vertex.VertexFormat format;

    @Unique
    private cursed.recode.scene.ModelJobs cursed$jobs;

    @Shadow
    private void ensureBuilding() {
    }

    @Shadow
    private void endLastVertex() {
    }

    @Override
    public long cursed$reserve(int count) {
        this.ensureBuilding();
        this.endLastVertex();
        if (this.vertices + count > 16777215) {
            throw new IllegalStateException("Trying to write too many vertices (>16777215) into BufferBuilder");
        }
        this.vertices += count;
        return this.buffer.reserve(count * this.vertexSize);
    }

    @Override
    public int cursed$vertices() {
        return this.vertices;
    }

    @Override
    public boolean cursed$entity() {
        return this.entityFormat;
    }

    @Override
    public com.mojang.renderpearl.api.vertex.VertexFormat cursed$format() {
        return this.format;
    }

    @Override
    public cursed.recode.scene.ModelJobs cursed$jobs() {
        return this.cursed$jobs;
    }

    @Override
    public void cursed$setJobs(cursed.recode.scene.ModelJobs jobs) {
        this.cursed$jobs = jobs;
    }

    @Override
    public long cursed$base() {
        return this.buffer.pointer;
    }
}
