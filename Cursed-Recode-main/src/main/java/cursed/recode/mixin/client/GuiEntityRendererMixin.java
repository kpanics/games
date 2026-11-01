package cursed.recode.mixin.client;

import com.mojang.blaze3d.vertex.PoseStack;
import cursed.recode.chams.ChamsHooks;
import net.minecraft.client.gui.render.pip.GuiEntityRenderer;
import net.minecraft.client.renderer.SubmitNodeCollector;
import net.minecraft.client.renderer.state.gui.pip.GuiEntityRenderState;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** A creature drawn into the screen (the HUD's target, the inventory's player) is never a chams' body (ChamsHooks.gui). */
@Mixin(GuiEntityRenderer.class)
public abstract class GuiEntityRendererMixin {
    @Inject(method = "renderToTexture(Lnet/minecraft/client/renderer/state/gui/pip/GuiEntityRenderState;Lcom/mojang/blaze3d/vertex/PoseStack;Lnet/minecraft/client/renderer/SubmitNodeCollector;)V",
            at = @At("HEAD"))
    private void cursed$guiStart(GuiEntityRenderState state, PoseStack pose, SubmitNodeCollector out, CallbackInfo ci) {
        ChamsHooks.guiStart();
    }

    @Inject(method = "renderToTexture(Lnet/minecraft/client/renderer/state/gui/pip/GuiEntityRenderState;Lcom/mojang/blaze3d/vertex/PoseStack;Lnet/minecraft/client/renderer/SubmitNodeCollector;)V",
            at = @At("RETURN"))
    private void cursed$guiDone(GuiEntityRenderState state, PoseStack pose, SubmitNodeCollector out, CallbackInfo ci) {
        ChamsHooks.guiDone();
    }
}
