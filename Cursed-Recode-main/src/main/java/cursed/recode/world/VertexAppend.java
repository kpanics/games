package cursed.recode.world;

/** Room for whole vertices at the end of a BufferBuilder, filled from outside (mixin BufferBuilderMixin). */
public interface VertexAppend {
    /** Reserves `count` vertices and returns where they go; the builder counts them as written. */
    long cursed$reserve(int count);

    /** The vertices written so far. */
    int cursed$vertices();

    /** Whether the builder writes DefaultVertexFormat.ENTITY (its fast path). */
    boolean cursed$entity();

    com.mojang.renderpearl.api.vertex.VertexFormat cursed$format();

    /** Where the GPU's share of this builder's models goes (a StagedVertexBuffer's), or null when it has none. */
    cursed.recode.scene.@org.jspecify.annotations.Nullable ModelJobs cursed$jobs();

    void cursed$setJobs(cursed.recode.scene.@org.jspecify.annotations.Nullable ModelJobs jobs);

    /** Where the builder's ByteBufferBuilder starts now (reserved addresses minus this are offsets that stay). */
    long cursed$base();
}
