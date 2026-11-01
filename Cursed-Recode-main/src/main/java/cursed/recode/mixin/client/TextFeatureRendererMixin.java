package cursed.recode.mixin.client;

import cursed.recode.scene.NativeText;
import java.util.List;
import net.minecraft.client.renderer.feature.FeatureFrameContext;
import net.minecraft.client.renderer.feature.TextFeatureRenderer;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** The last run of glyphs goes out when the text group ends (cursed.recode.scene.NativeText). */
@Mixin(TextFeatureRenderer.class)
public abstract class TextFeatureRendererMixin {
    @Inject(method = "buildGroup", at = @At("HEAD"))
    private void cursed$start(FeatureFrameContext context, List<TextFeatureRenderer.Submit> submits, CallbackInfo ci) {
        cursed.recode.dev.FeatureTimes.start("text");
    }

    /** Each submit's pose: the run of glyphs ends with it (cursed.recode.scene.NativeText.renderFeature). */
    @org.spongepowered.asm.mixin.injection.Redirect(method = "buildGroup",
            at = @At(value = "INVOKE", target = "Lorg/joml/Matrix4f;set(Lorg/joml/Matrix4fc;)Lorg/joml/Matrix4f;"))
    private org.joml.Matrix4f cursed$pose(org.joml.Matrix4f pose, org.joml.Matrix4fc next) {
        NativeText.newPose();
        return pose.set(next);
    }

    /** A prepared text drawn from its plan when it has one (cursed.recode.scene.TextPlan). */
    @org.spongepowered.asm.mixin.injection.Redirect(method = "renderText",
            at = @At(value = "INVOKE", target = "Lnet/minecraft/client/gui/Font$PreparedText;visit(Lnet/minecraft/client/gui/Font$GlyphVisitor;)V"))
    private static void cursed$plan(net.minecraft.client.gui.Font.PreparedText text, net.minecraft.client.gui.Font.GlyphVisitor visitor) {
        cursed.recode.scene.TextPlan.visit(text, visitor);
    }

    @Inject(method = "buildGroup", at = @At("TAIL"))
    private void cursed$flush(FeatureFrameContext context, List<TextFeatureRenderer.Submit> submits, CallbackInfo ci) {
        NativeText.flush();
        cursed.recode.dev.FeatureTimes.stop("text");
    }
}
