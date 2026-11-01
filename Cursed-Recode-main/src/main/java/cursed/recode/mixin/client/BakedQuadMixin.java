package cursed.recode.mixin.client;

import cursed.recode.scene.QuadAccess;
import net.minecraft.client.resources.model.geometry.BakedQuad;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Unique;

/** Where an item quad's vertices are on the GPU (cursed.recode.scene.NativeItems). */
@Mixin(BakedQuad.class)
public abstract class BakedQuadMixin implements QuadAccess {
    @Unique
    private int cursed$gpuVertex = -1;
    @Unique
    private volatile Object cursed$plan;

    @Override
    public Object cursed$plan() {
        return this.cursed$plan;
    }

    @Override
    public void cursed$setPlan(Object plan) {
        this.cursed$plan = plan;
    }

    @Override
    public int cursed$gpuVertex() {
        return this.cursed$gpuVertex;
    }

    @Override
    public void cursed$setGpuVertex(int vertex) {
        this.cursed$gpuVertex = vertex;
    }
}
