package cursed.recode.scene;

/** What NativeItems keeps on a BakedQuad (mixin BakedQuadMixin). */
public interface QuadAccess {
    /** The quad's first vertex in the GPU's mesh (native/src/scene/models.rs GPU_MESH), -1 until handed over. */
    int cursed$gpuVertex();

    void cursed$setGpuVertex(int vertex);

    /** The plan of the item model whose quad list starts with this quad (NativeItems.Plan), per foil; null until made. */
    Object cursed$plan();

    void cursed$setPlan(Object plan);
}
