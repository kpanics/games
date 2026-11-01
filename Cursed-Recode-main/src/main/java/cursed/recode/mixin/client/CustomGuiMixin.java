package cursed.recode.mixin.client;

import cursed.recode.hud.HudScreen;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.screens.Screen;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.ModifyArgs;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;
import org.spongepowered.asm.mixin.injection.invoke.arg.Args;

/**
 * Custom Gui (cursed.recode.hud.HudScreen) around a screen's drawing: its course (the container's words and pictures are
 * the Custom Gui's while it lasts), the world's dimming, the panel under the game's picture as the screen grows in.
 */
@Mixin(Screen.class)
public abstract class CustomGuiMixin {
    @Inject(method = "extractRenderStateWithTooltipAndSubtitles", at = @At("HEAD"))
    private void cursed$begin(GuiGraphicsExtractor graphics, int mouseX, int mouseY, float a, CallbackInfo ci) {
        HudScreen.begin((Screen) (Object) this);
    }

    @Inject(method = "extractRenderStateWithTooltipAndSubtitles", at = @At("RETURN"))
    private void cursed$end(GuiGraphicsExtractor graphics, int mouseX, int mouseY, float a, CallbackInfo ci) {
        HudScreen.end(graphics);
    }

    /**
     * In a world the world behind every screen is the client's (the HUD's veil: greyed and darkened as the Theme says),
     * not the game's dark gradient, its blur or its in-world menu background; the subtitles still go on. Not under the
     * chat, the layout editor or the client's menu (its window does its own).
     */
    @Inject(method = "extractBackground", at = @At("HEAD"), cancellable = true)
    private void cursed$worldBehind(GuiGraphicsExtractor graphics, int mouseX, int mouseY, float a, CallbackInfo ci) {
        Screen self = (Screen) (Object) this;
        var mc = net.minecraft.client.Minecraft.getInstance();
        if (mc.level != null && cursed.recode.render.GuiFrame.active() && !(self instanceof net.minecraft.client.gui.screens.ChatScreen)
                && !(self instanceof cursed.recode.hud.HudEditor) && !(self instanceof cursed.recode.gui.ClickGui)
                && !cursed.recode.dev.Dev.on("CURSED_DEV_VANILLASCREENS")) {
            mc.gui.hud.extractDeferredSubtitles();
            HudScreen.afterBackground(self, graphics);
            ci.cancel();
        }
    }

    @Inject(method = "extractBackground", at = @At("RETURN"))
    private void cursed$panel(GuiGraphicsExtractor graphics, int mouseX, int mouseY, float a, CallbackInfo ci) {
        HudScreen.afterBackground((Screen) (Object) this, graphics);
    }

    @ModifyArgs(method = "extractTransparentBackground", at = @At(value = "INVOKE",
            target = "Lnet/minecraft/client/gui/GuiGraphicsExtractor;fillGradient(IIIIII)V"))
    private void cursed$dim(Args args) {
        args.set(4, HudScreen.dim(args.get(4), true));
        args.set(5, HudScreen.dim(args.get(5), false));
    }
}
