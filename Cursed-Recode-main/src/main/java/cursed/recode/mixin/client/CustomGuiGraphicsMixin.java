package cursed.recode.mixin.client;

import cursed.recode.hud.HudScreen;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.resources.Identifier;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.ModifyVariable;

/** Custom Gui (cursed.recode.hud.HudScreen): a container's pictures without their grey, its dark words light. */
@Mixin(GuiGraphicsExtractor.class)
public abstract class CustomGuiGraphicsMixin {
    @ModifyVariable(method = "innerBlit(Lcom/mojang/renderpearl/api/pipeline/RenderPipeline;Lnet/minecraft/resources/Identifier;IIIIFFFFI)V",
            at = @At("HEAD"), argsOnly = true)
    private Identifier cursed$picture(Identifier id) {
        return HudScreen.drawing() ? HudScreen.picture(id) : id;
    }

    @org.spongepowered.asm.mixin.injection.Redirect(method = {
            "blitSprite(Lcom/mojang/renderpearl/api/pipeline/RenderPipeline;Lnet/minecraft/resources/Identifier;IIIII)V",
            "blitSprite(Lcom/mojang/renderpearl/api/pipeline/RenderPipeline;Lnet/minecraft/resources/Identifier;IIIIIIIII)V"},
            at = @At(value = "INVOKE", target = "Lnet/minecraft/client/renderer/texture/TextureAtlas;getSprite(Lnet/minecraft/resources/Identifier;)Lnet/minecraft/client/renderer/texture/TextureAtlasSprite;"))
    private net.minecraft.client.renderer.texture.TextureAtlasSprite cursed$sprite(net.minecraft.client.renderer.texture.TextureAtlas atlas, Identifier id) {
        return atlas.getSprite(HudScreen.drawing() ? HudScreen.sprite(id) : id);
    }

    @ModifyVariable(method = "text(Lnet/minecraft/client/gui/Font;Lnet/minecraft/util/FormattedCharSequence;IIIZ)V",
            at = @At("HEAD"), argsOnly = true, ordinal = 2)
    private int cursed$words(int color) {
        return HudScreen.words(color);
    }
}
