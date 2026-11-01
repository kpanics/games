package cursed.recode.mixin.client;

import com.mojang.blaze3d.pipeline.RenderTarget;
import net.minecraft.client.renderer.SkyRenderer;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Redirect;

/** The sky goes where the level goes: the engine's smaller target while it upscales (cursed.recode.engine.Upscale). */
@Mixin(SkyRenderer.class)
public abstract class SkyRendererMixin {
    @Redirect(method = "render", at = @At(value = "FIELD", target = "Lnet/minecraft/client/renderer/SkyRenderer;renderTarget:Lcom/mojang/blaze3d/pipeline/RenderTarget;"))
    private RenderTarget cursed$target(SkyRenderer self) {
        RenderTarget t = cursed.recode.engine.Upscale.target();
        return t != null ? t : ((SkyRendererAccess) self).cursed$renderTarget();
    }
}
