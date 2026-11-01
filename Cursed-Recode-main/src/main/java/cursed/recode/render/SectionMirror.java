package cursed.recode.render;

import net.minecraft.client.renderer.chunk.SectionMesh;

/**
 * What the frame reads of every visible section, kept on the section itself (RenderSectionMixin): its mesh, whether the
 * mesh has block entities to draw, where it is, its dirty state. The game keeps the mesh behind an AtomicReference, the
 * origin in a BlockPos of its own, the dirty state in a rotating store, and walking the six hundred visible sections
 * three times a frame through them cost the game's thread some 70 µs in cache misses (12.10, JFR: getSectionMesh the
 * hottest method at 20%).
 */
public interface SectionMirror {
    SectionMesh cursed$plainMesh();

    /** The native terrain id of the current mesh when it has layers to draw, else 0 (kept by the mesh too: setMesh). */
    int cursed$drawId();

    void cursed$setDrawId(int id);

    boolean cursed$hasBlockEntities();

    int cursed$originX();

    int cursed$originY();

    int cursed$originZ();

    /** The section's dirty state as last looked up, and the tracker it came from (LevelExtractorMixin). */
    Object cursed$state();

    Object cursed$stateOwner();

    void cursed$setState(Object state, Object owner);
}
