package cursed.recode.mixin.client;

import com.mojang.blaze3d.vertex.QuadInstance;
import cursed.recode.scene.NativeItems;
import net.minecraft.client.renderer.feature.ItemFeatureRenderer;
import net.minecraft.client.renderer.feature.RenderTypeFeatureRenderer;
import org.spongepowered.asm.mixin.Final;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Items' quads are written natively (cursed.recode.scene.NativeItems). */
@Mixin(ItemFeatureRenderer.class)
public abstract class ItemFeatureRendererMixin {
    @Shadow
    @Final
    private QuadInstance quadInstance;

    @Inject(method = "prepareMainSubmit", at = @At("HEAD"), cancellable = true)
    private void cursed$prepare(ItemFeatureRenderer.Submit submit, CallbackInfo ci) {
        RenderTypeFeatureRenderer<?> self = (RenderTypeFeatureRenderer<?>) (Object) this;
        // an item of a chams' body (cursed.recode.chams.Chams): its types' twins
        int chams = cursed.recode.chams.Chams.itemKind(submit);
        java.util.function.Function<net.minecraft.client.renderer.rendertype.RenderType, com.mojang.blaze3d.vertex.VertexConsumer> builder = self::getVertexBuilder;
        if (chams != 0) {
            builder = type -> {
                net.minecraft.client.renderer.rendertype.RenderType twin = cursed.recode.chams.Chams.twin(type, chams);
                return self.getVertexBuilder(twin != null ? twin : type);
            };
        }
        cursed.recode.dev.FeatureTimes.start("items");
        boolean done = NativeItems.prepare(submit, this.quadInstance, builder);
        cursed.recode.dev.FeatureTimes.stop("items");
        if (!done && chams != 0) {
            // the game's own loop, with the twins
            this.quadInstance.setLightCoords(submit.lightCoords());
            this.quadInstance.setOverlayCoords(submit.overlayCoords());
            int[] tints = submit.tintLayers();
            for (net.minecraft.client.resources.model.geometry.BakedQuad quad : submit.quads()) {
                var material = quad.materialInfo();
                int i = material.tintIndex();
                this.quadInstance.setColor(material.isTinted() && i >= 0 && i < tints.length ? tints[i] : -1);
                builder.apply(material.itemRenderType()).putBakedQuad(submit.pose(), quad, this.quadInstance);
            }
            done = true;
        }
        if (done) {
            ci.cancel();
        }
    }
}
