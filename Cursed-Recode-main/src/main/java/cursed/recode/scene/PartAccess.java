package cursed.recode.scene;

import java.util.List;
import java.util.Map;
import net.minecraft.client.model.geom.ModelPart;

/** What NativeModels needs of a ModelPart (mixin ModelPartMixin). */
public interface PartAccess {
    List<ModelPart.Cube> cursed$cubes();

    Map<String, ModelPart> cursed$children();

    /** The part's polygons in the DLL (scene/models.rs), -1 until handed over. */
    int cursed$modelId();

    void cursed$setModelId(int id);

    /** The part's first vertex in the GPU's mesh (set before the id). */
    int cursed$firstVertex();

    void cursed$setFirstVertex(int first);

    /** Vertices of the part's cubes (4 per polygon). */
    int cursed$vertexCount();

    /** The tree under this part laid out for the native walk (NativeModels.Flat), null until first drawn so. */
    Object cursed$flat();

    void cursed$setFlat(Object flat);
}
