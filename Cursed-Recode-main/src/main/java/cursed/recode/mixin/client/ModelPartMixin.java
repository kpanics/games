package cursed.recode.mixin.client;

import com.mojang.blaze3d.vertex.PoseStack;
import com.mojang.blaze3d.vertex.VertexConsumer;
import cursed.recode.scene.NativeModels;
import cursed.recode.scene.PartAccess;
import java.util.List;
import java.util.Map;
import net.minecraft.client.model.geom.ModelPart;
import org.spongepowered.asm.mixin.Final;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.Unique;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Models' vertices are made natively (cursed.recode.scene.NativeModels). */
@Mixin(ModelPart.class)
public abstract class ModelPartMixin implements PartAccess {
    @Shadow
    @Final
    private List<ModelPart.Cube> cubes;
    @Shadow
    @Final
    private Map<String, ModelPart> children;
    @Unique
    private volatile int cursed$modelId = -1;
    @Unique
    private int cursed$vertexCount = -1;
    @Unique
    private int cursed$firstVertex = -1;
    @Unique
    private volatile Object cursed$flat;

    @Inject(method = "render(Lcom/mojang/blaze3d/vertex/PoseStack;Lcom/mojang/blaze3d/vertex/VertexConsumer;III)V", at = @At("HEAD"), cancellable = true)
    private void cursed$render(PoseStack poseStack, VertexConsumer buffer, int lightCoords, int overlayCoords, int color, CallbackInfo ci) {
        if (NativeModels.render((ModelPart) (Object) this, poseStack, buffer, lightCoords, overlayCoords, color)) {
            ci.cancel();
        }
    }

    @Override
    public List<ModelPart.Cube> cursed$cubes() {
        return this.cubes;
    }

    @Override
    public Map<String, ModelPart> cursed$children() {
        return this.children;
    }

    @Override
    public int cursed$modelId() {
        return this.cursed$modelId;
    }

    @Override
    public void cursed$setModelId(int id) {
        this.cursed$modelId = id;
    }

    @Override
    public int cursed$firstVertex() {
        return this.cursed$firstVertex;
    }

    @Override
    public void cursed$setFirstVertex(int first) {
        this.cursed$firstVertex = first;
    }

    @Override
    public Object cursed$flat() {
        return this.cursed$flat;
    }

    @Override
    public void cursed$setFlat(Object flat) {
        this.cursed$flat = flat;
    }

    @Override
    public int cursed$vertexCount() {
        int n = this.cursed$vertexCount;
        if (n < 0) {
            n = 0;
            for (ModelPart.Cube cube : this.cubes) {
                n += cube.polygons.length * 4;
            }
            this.cursed$vertexCount = n;
        }
        return n;
    }
}
