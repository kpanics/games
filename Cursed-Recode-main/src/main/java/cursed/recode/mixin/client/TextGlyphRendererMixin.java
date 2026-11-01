package cursed.recode.mixin.client;

import com.mojang.blaze3d.vertex.VertexConsumer;
import cursed.recode.scene.NativeText;
import cursed.recode.scene.TextPlan;
import net.minecraft.client.gui.Font;
import net.minecraft.client.gui.font.TextRenderable;
import net.minecraft.client.renderer.feature.TextFeatureRenderer;
import net.minecraft.client.renderer.rendertype.RenderType;
import org.joml.Matrix4f;
import org.joml.Matrix4fc;
import org.spongepowered.asm.mixin.Final;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Redirect;

/**
 * Each text renderable of TextFeatureRenderer goes through NativeText (glyph runs natively, the rest the game's way);
 * the renderer draws text plans too (cursed.recode.scene.TextPlan).
 */
@Mixin(targets = "net.minecraft.client.renderer.feature.TextFeatureRenderer$GlyphRenderer")
public abstract class TextGlyphRendererMixin implements TextPlan.Target {
    @Shadow @Final private Matrix4f pose;
    @Shadow private int lightCoords;
    @Shadow private Font.DisplayMode displayMode;
    @Shadow @Final TextFeatureRenderer this$0;

    @Redirect(
            method = "acceptRenderable",
            at = @At(value = "INVOKE", target = "Lnet/minecraft/client/gui/font/TextRenderable;render(Lorg/joml/Matrix4fc;Lcom/mojang/blaze3d/vertex/VertexConsumer;IZ)V"))
    private void cursed$render(TextRenderable renderable, Matrix4fc pose, VertexConsumer consumer, int light, boolean flat) {
        NativeText.renderFeature(renderable, pose, consumer, light, flat);
    }

    @Override
    public Matrix4fc cursed$pose() {
        return this.pose;
    }

    @Override
    public int cursed$light() {
        return this.lightCoords;
    }

    @Override
    public Font.DisplayMode cursed$mode() {
        return this.displayMode;
    }

    @Override
    public VertexConsumer cursed$builder(RenderType type) {
        return this.this$0.getVertexBuilder(type);
    }
}
