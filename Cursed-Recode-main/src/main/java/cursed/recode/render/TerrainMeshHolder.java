package cursed.recode.render;

/** A compiled section mesh of the game, with the id of its copy in the native terrain (0 = none). */
public interface TerrainMeshHolder {
    int cursed$mesh();

    /** Sets the id, and the drawn id of the section the mesh is the current one of (SectionMirror). */
    void cursed$setMesh(int id);

    /** The section this mesh was last made the current one of (a RenderSection, as SectionMirror), or null. */
    @org.jspecify.annotations.Nullable Object cursed$owner();

    void cursed$setOwner(Object section);

    /** Quads per facing, 3 layers x 7 (cursed.recode.world.FacingHolder), or null when the game compiled it. */
    int @org.jspecify.annotations.Nullable [] cursed$facings();
}
