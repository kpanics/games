package cursed.recode.world;

import org.jspecify.annotations.Nullable;

/**
 * Quads per facing of a compiled section's solid and cutout layers (3 layers x COUNT, world/mesher.rs FACINGS order), when
 * the native mesher made them: kept on SectionCompiler.Results and then on the CompiledSectionMesh, so the upload can
 * hand them to the terrain (vk/terrain.rs draws only what can face the camera).
 */
public interface FacingHolder {
    /** Facings per layer: the unculled, the six directions, the four upright diagonals (world/mesher.rs FACINGS). */
    int COUNT = 11;

    int @Nullable [] cursed$facings();

    void cursed$setFacings(int @Nullable [] facings);
}
