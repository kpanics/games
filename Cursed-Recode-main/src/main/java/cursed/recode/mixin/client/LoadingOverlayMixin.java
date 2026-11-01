package cursed.recode.mixin.client;

import cursed.recode.launch.LaunchScreen;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.screens.LoadingOverlay;
import net.minecraft.server.packs.resources.ReloadInstance;
import net.minecraft.util.ARGB;
import net.minecraft.util.Mth;
import net.minecraft.util.Util;
import org.spongepowered.asm.mixin.Final;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/**
 * No Mojang screen: the game's loading screen (at the start without the window before the game, and when resources
 * reload — F3+T, packs, the language) is black with one thin line of progress, in the menu's manner. At the start it
 * goes the moment loading is done instead of fading over two more seconds: the main menu (and the window before the
 * game waiting for it) need not wait for a fade nobody sees.
 */
@Mixin(LoadingOverlay.class)
public abstract class LoadingOverlayMixin {
    @Shadow
    @Final
    private Minecraft minecraft;
    @Shadow
    @Final
    private ReloadInstance reload;
    @Shadow
    @Final
    private boolean fadeIn;
    @Shadow
    private float currentProgress;
    @Shadow
    private long fadeInStart;
    @Shadow
    private long fadeOutStart;

    @Inject(method = "extractRenderState", at = @At("HEAD"), cancellable = true)
    private void cursed$ours(GuiGraphicsExtractor graphics, int mouseX, int mouseY, float a, CallbackInfo ci) {
        ci.cancel();
        // Custom Gui's pictures are made again from the resources as they are after it
        cursed.recode.hud.HudScreen.reloading();
        int w = graphics.guiWidth(), h = graphics.guiHeight();
        long now = Util.getMillis();
        if (this.fadeIn && this.fadeInStart == -1L) {
            this.fadeInStart = now;
        }
        float out = this.fadeOutStart > -1L ? (now - this.fadeOutStart) / 600.0F : -1.0F;
        float in = this.fadeInStart > -1L ? (now - this.fadeInStart) / 400.0F : -1.0F;
        float alpha;
        if (out >= 0.0F) {
            // done: what is under it shows through as the black goes
            if (this.minecraft.gui.screen() != null) {
                cursed.recode.menus.Menus.extract(this.minecraft.gui.screen(), graphics, 0, 0, a);
            }
            graphics.nextStratum();
            alpha = 1.0F - Mth.clamp(out, 0.0F, 1.0F);
        } else if (this.fadeIn) {
            if (this.minecraft.gui.screen() != null && in < 1.0F) {
                cursed.recode.menus.Menus.extract(this.minecraft.gui.screen(), graphics, mouseX, mouseY, a);
            }
            graphics.nextStratum();
            alpha = Mth.clamp(in, 0.0F, 1.0F);
        } else {
            alpha = 1.0F;
        }
        graphics.fill(0, 0, w, h, ARGB.color(Math.round(alpha * 255.0F), 0, 0, 0));
        // the progress: one thin line, a faint track and the part done bright
        this.currentProgress = Mth.clamp(this.currentProgress * 0.95F + this.reload.getActualProgress() * 0.05F, 0.0F, 1.0F);
        // (and the window before the game's card: its point runs along its line with it)
        cursed.recode.launch.Twin.load(this.reload.getActualProgress());
        float line = alpha * (out >= 0.0F ? 0.0F : 1.0F);
        if (line > 0.01F) {
            int x0 = Math.round(w * 0.38F), x1 = Math.round(w * 0.62F), y = Math.round(h * 0.86F);
            graphics.fill(x0, y, x1, y + 1, ARGB.color(Math.round(line * 40.0F), 255, 255, 255));
            graphics.fill(x0, y, x0 + Math.round((x1 - x0) * this.currentProgress), y + 1, ARGB.color(Math.round(line * 220.0F), 255, 255, 255));
        }
        if (out >= 1.0F) {
            this.minecraft.gui.setOverlay(null);
        }
    }

    /** At the start the main menu is under it once loading is done: it goes at once (the menu plays its own way in). */
    @Inject(method = "tick", at = @At("TAIL"))
    private void cursed$goAtOnce(CallbackInfo ci) {
        cursed.recode.launch.Twin.load(this.reload.getActualProgress());
        if (!this.fadeIn && this.fadeOutStart != -1L && this.minecraft.gui.screen() instanceof LaunchScreen) {
            this.minecraft.gui.setOverlay(null);
        }
    }
}
