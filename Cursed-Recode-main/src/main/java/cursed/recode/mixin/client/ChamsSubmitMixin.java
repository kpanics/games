package cursed.recode.mixin.client;

import com.mojang.blaze3d.vertex.PoseStack;
import cursed.recode.chams.Chams;
import net.minecraft.client.model.Model;
import net.minecraft.client.renderer.SubmitNodeCollection;
import net.minecraft.client.renderer.SubmitNodeCollector;
import net.minecraft.client.renderer.feature.ItemFeatureRenderer;
import net.minecraft.client.renderer.feature.phase.SimpleFeatureRenderPhase;
import net.minecraft.client.renderer.item.ItemStackRenderState;
import net.minecraft.client.renderer.rendertype.RenderType;
import net.minecraft.client.renderer.texture.UvMapping;
import net.minecraft.client.resources.model.geometry.ItemQuads;
import net.minecraft.world.item.ItemDisplayContext;
import org.spongepowered.asm.mixin.Final;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.Unique;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/**
 * Chams (cursed.recode.chams.Chams): while a body in the look is submitted, the models of the parts the player wants in
 * it go with their render types' twins (the enchantments' shine not at all), and its items are marked for theirs
 * (ItemFeatureRendererMixin). What is drawn in the look goes with the opaque world whatever the game's Improved
 * Transparency (its own phase needs pipelines of its own).
 */
@Mixin(SubmitNodeCollection.class)
public abstract class ChamsSubmitMixin {
    @Shadow
    @Final
    public SimpleFeatureRenderPhase solid;

    @Unique
    private static boolean cursed$again;

    @Inject(method = "submitModel", at = @At("HEAD"), cancellable = true)
    @SuppressWarnings({"unchecked", "rawtypes"})
    private void cursed$chams(Model model, Object state, PoseStack poseStack, RenderType renderType, int light, int overlay, int tint, UvMapping uvMapping,
                              int outline, CallbackInfo ci) {
        if (!cursed$again) {
            // a target's model: its boxes for Target ESP (they hide what it draws behind the body)
            cursed.recode.render.TargetEsp.model(model, state, poseStack);
            // a body in the shadow: its eyes found in its texture before its twin is made
            if (Chams.shadowBody() && Chams.firstModel()) {
                net.minecraft.resources.Identifier texture = Chams.texture(renderType);
                if (texture != null && !cursed.recode.chams.EyeMaps.known(texture)) {
                    cursed.recode.chams.EyeMaps.prepare(texture, model);
                }
            }
        }
        if (!cursed$again && Chams.boxedBody()) {
            // a body in an engine's look: its boxes to the engine; the liquid's polygons not drawn (the engine draws it) —
            // unless its boxes did not fit this frame: then its polygons in the liquid's matter
            boolean taken = cursed.recode.chams.ChamsLiquid.model(model, state, poseStack, light);
            if (taken && Chams.liquidBody()) {
                ci.cancel();
                return;
            }
        }
        if (cursed$again || !Chams.swapping(false)) {
            return;
        }
        RenderType twin = Chams.swap(renderType);
        if (twin == null) {
            ci.cancel();
            return;
        }
        if (twin == renderType) {
            return;
        }
        ci.cancel();
        cursed$again = true;
        try {
            ((SubmitNodeCollection) (Object) this).submitModel(model, state, poseStack, twin, light, overlay, tint, uvMapping, outline);
        } finally {
            cursed$again = false;
        }
    }

    /** What is drawn as quads of its own (an experience orb): with its type's twin as a model. */
    @Inject(method = "submitCustomGeometry", at = @At("HEAD"), cancellable = true)
    private void cursed$chamsCustom(PoseStack poseStack, RenderType renderType, SubmitNodeCollector.CustomGeometryRenderer renderer, CallbackInfo ci) {
        if (cursed$again || !Chams.swapping(false)) {
            return;
        }
        RenderType twin = Chams.swap(renderType);
        if (twin == null) {
            ci.cancel();
            return;
        }
        if (twin == renderType) {
            return;
        }
        ci.cancel();
        cursed$again = true;
        try {
            ((SubmitNodeCollection) (Object) this).submitCustomGeometry(poseStack, twin, renderer);
        } finally {
            cursed$again = false;
        }
    }

    @Inject(method = "submitItem", at = @At("HEAD"), cancellable = true)
    private void cursed$chamsItem(PoseStack poseStack, ItemDisplayContext context, int light, int overlay, int outline, int[] tints, ItemQuads quads,
                                  ItemStackRenderState.FoilType foil, CallbackInfo ci) {
        if (!Chams.swapping(true)) {
            return;
        }
        ci.cancel();
        if (quads.isEmpty()) {
            return;
        }
        // all of it with the opaque world, without the shine: the twins' matter is the item's now
        ItemFeatureRenderer.Submit submit = new ItemFeatureRenderer.Submit(poseStack.last().copy(), context, light, overlay, 0, tints, quads.all(),
                ItemStackRenderState.FoilType.NONE);
        Chams.markItem(submit, Chams.kind());
        this.solid.submit(submit);
    }
}
