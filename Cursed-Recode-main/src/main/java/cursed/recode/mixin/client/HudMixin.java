package cursed.recode.mixin.client;

import cursed.recode.scene.HudCache;
import net.minecraft.client.DeltaTracker;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.Font;
import net.minecraft.client.gui.Hud;
import net.minecraft.client.gui.contextualbar.ContextualBar;
import net.minecraft.client.gui.contextualbar.ExperienceBar;
import net.minecraft.client.gui.contextualbar.JumpableVehicleBar;
import cursed.recode.hud.HudBody;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.Redirect;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/**
 * The HUD past the crosshair (cursed.recode.scene.HudCache): on a frame that draws it, the strata from here on are the
 * texture's (the render state's blur index marks where; the HUD's frames have no blur); on a frame that keeps it, the
 * texture's pieces go here and nothing more is extracted: not the HUD's rest, nor the saving indicator or F3 after it.
 * And no vignette at all (12.10, her word: "nobody likes it"; a full-screen draw, a tenth of the GPU's frame in the
 * shade at 1440p): nor the world border's red warning, which is drawn as it.
 */
@Mixin(Hud.class)
public abstract class HudMixin {
    @Inject(method = "extractRenderState", cancellable = true,
            at = @At(value = "INVOKE", target = "Lnet/minecraft/client/gui/GuiGraphicsExtractor;nextStratum()V", shift = At.Shift.AFTER))
    private void cursed$split(GuiGraphicsExtractor graphics, DeltaTracker deltaTracker, CallbackInfo ci) {
        if (HudCache.active()) {
            var state = Minecraft.getInstance().gameRenderer.gameRenderState().guiRenderState;
            if (HudCache.kept()) {
                HudCache.place(state);
                ci.cancel();
            } else {
                HudCache.split(state);
            }
        }
    }

    /** The client's HUD (cursed.recode.hud.HudLayer), in the live stratum with the crosshair: never kept in the cache. */
    @Inject(method = "extractRenderState", at = @At(value = "INVOKE", target = "Lnet/minecraft/client/gui/GuiGraphicsExtractor;nextStratum()V"))
    private void cursed$hud(GuiGraphicsExtractor graphics, DeltaTracker deltaTracker, CallbackInfo ci) {
        cursed.recode.hud.HudLayer.extract(graphics, deltaTracker.getGameTimeDeltaPartialTick(false));
        cursed.recode.menus.TabList.extract(graphics);
        cursed.recode.menus.Menus.leftovers(graphics);
    }

    /** CS's crosshair (the HUD's: native/src/cosmetics/crosshair.rs) while a CS knife is in the hands, and none in a
     *  rifle's scope (CS's own lens there): not the game's. */
    @Inject(method = "extractCrosshair", at = @At("HEAD"), cancellable = true)
    private void cursed$crosshair(GuiGraphicsExtractor graphics, DeltaTracker deltaTracker, CallbackInfo ci) {
        if (cursed.recode.cosmetics.Cosmetics.crosshair()) {
            ci.cancel();
        }
    }

    /** The player list is the menus' (cursed.recode.menus.TabList, drawn with the HUD above). */
    @Inject(method = "extractTabList", at = @At("HEAD"), cancellable = true)
    private void cursed$tabList(GuiGraphicsExtractor graphics, DeltaTracker deltaTracker, CallbackInfo ci) {
        if (!cursed.recode.dev.Dev.on("CURSED_DEV_VANILLASCREENS")) {
            ci.cancel();
        }
    }

    // ---- The Custom Hotbar (Interface; HudBody): each part of the game's hotbar and bars the client draws itself is left
    // out; one switched off is the game's again, lifted to rest over the client's hotbar.

    /** Up by the lift (nothing pushed when none: `pushed` says which). */
    private static boolean cursed$up(GuiGraphicsExtractor graphics) {
        float lift = HudBody.lift(graphics.guiHeight());
        if (lift <= 0.0F) {
            return false;
        }
        graphics.pose().pushMatrix();
        graphics.pose().translate(0.0F, -lift);
        return true;
    }

    private boolean cursed$healthUp;
    private boolean cursed$mountUp;
    private boolean cursed$nameUp;

    @Inject(method = "extractItemHotbar", at = @At("HEAD"), cancellable = true)
    private void cursed$slots(GuiGraphicsExtractor graphics, DeltaTracker deltaTracker, CallbackInfo ci) {
        if (HudBody.drawn(HudBody.HOTBAR)) {
            ci.cancel();
        }
    }

    @Inject(method = "extractPlayerHealth", at = @At("HEAD"))
    private void cursed$healthIn(GuiGraphicsExtractor graphics, CallbackInfo ci) {
        cursed$healthUp = cursed$up(graphics);
    }

    @Inject(method = "extractPlayerHealth", at = @At("RETURN"))
    private void cursed$healthOut(GuiGraphicsExtractor graphics, CallbackInfo ci) {
        if (cursed$healthUp) {
            graphics.pose().popMatrix();
            cursed$healthUp = false;
        }
    }

    @Inject(method = "extractHearts", at = @At("HEAD"), cancellable = true)
    private void cursed$hearts(CallbackInfo ci) {
        if (HudBody.drawn(HudBody.HEALTH)) {
            ci.cancel();
        }
    }

    @Inject(method = "extractArmor", at = @At("HEAD"), cancellable = true)
    private static void cursed$armour(CallbackInfo ci) {
        if (HudBody.drawn(HudBody.ARMOUR)) {
            ci.cancel();
        }
    }

    @Inject(method = "extractFood", at = @At("HEAD"), cancellable = true)
    private void cursed$food(CallbackInfo ci) {
        if (HudBody.drawn(HudBody.FOOD)) {
            ci.cancel();
        }
    }

    @Inject(method = "extractAirBubbles", at = @At("HEAD"), cancellable = true)
    private void cursed$air(CallbackInfo ci) {
        if (HudBody.drawn(HudBody.AIR)) {
            ci.cancel();
        }
    }

    @Inject(method = "extractVehicleHealth", at = @At("HEAD"), cancellable = true)
    private void cursed$mountIn(GuiGraphicsExtractor graphics, CallbackInfo ci) {
        if (HudBody.drawn(HudBody.MOUNT)) {
            ci.cancel();
            return;
        }
        cursed$mountUp = cursed$up(graphics);
    }

    @Inject(method = "extractVehicleHealth", at = @At("RETURN"))
    private void cursed$mountOut(GuiGraphicsExtractor graphics, CallbackInfo ci) {
        if (cursed$mountUp) {
            graphics.pose().popMatrix();
            cursed$mountUp = false;
        }
    }

    /** The bar under the hearts (experience, the mount's jump, the locator): the client's own of it, or the game's lifted. */
    private static void cursed$bar(ContextualBar bar, GuiGraphicsExtractor graphics, DeltaTracker deltaTracker, boolean background) {
        if (bar instanceof ExperienceBar && HudBody.drawn(HudBody.XP) || bar instanceof JumpableVehicleBar && HudBody.drawn(HudBody.JUMP)) {
            return;
        }
        boolean up = cursed$up(graphics);
        if (background) {
            bar.extractBackground(graphics, deltaTracker);
        } else {
            bar.extractRenderState(graphics, deltaTracker);
        }
        if (up) {
            graphics.pose().popMatrix();
        }
    }

    @Redirect(method = "extractHotbarAndDecorations", at = @At(value = "INVOKE", target = "Lnet/minecraft/client/gui/contextualbar/ContextualBar;extractBackground(Lnet/minecraft/client/gui/GuiGraphicsExtractor;Lnet/minecraft/client/DeltaTracker;)V"))
    private void cursed$barBack(ContextualBar bar, GuiGraphicsExtractor graphics, DeltaTracker deltaTracker) {
        cursed$bar(bar, graphics, deltaTracker, true);
    }

    @Redirect(method = "extractHotbarAndDecorations", at = @At(value = "INVOKE", target = "Lnet/minecraft/client/gui/contextualbar/ContextualBar;extractRenderState(Lnet/minecraft/client/gui/GuiGraphicsExtractor;Lnet/minecraft/client/DeltaTracker;)V"))
    private void cursed$barFront(ContextualBar bar, GuiGraphicsExtractor graphics, DeltaTracker deltaTracker) {
        cursed$bar(bar, graphics, deltaTracker, false);
    }

    @Redirect(method = "extractHotbarAndDecorations", at = @At(value = "INVOKE", target = "Lnet/minecraft/client/gui/contextualbar/ContextualBar;extractExperienceLevel(Lnet/minecraft/client/gui/GuiGraphicsExtractor;Lnet/minecraft/client/gui/Font;I)V"))
    private void cursed$level(GuiGraphicsExtractor graphics, Font font, int level) {
        if (HudBody.drawn(HudBody.LEVEL)) {
            return;
        }
        boolean up = cursed$up(graphics);
        ContextualBar.extractExperienceLevel(graphics, font, level);
        if (up) {
            graphics.pose().popMatrix();
        }
    }

    @Inject(method = "extractSelectedItemName", at = @At("HEAD"), cancellable = true)
    private void cursed$nameIn(GuiGraphicsExtractor graphics, CallbackInfo ci) {
        if (HudBody.drawn(HudBody.NAME)) {
            ci.cancel();
            return;
        }
        cursed$nameUp = cursed$up(graphics);
    }

    @Inject(method = "extractSelectedItemName", at = @At("RETURN"))
    private void cursed$nameOut(GuiGraphicsExtractor graphics, CallbackInfo ci) {
        if (cursed$nameUp) {
            graphics.pose().popMatrix();
            cursed$nameUp = false;
        }
    }

    /** Other Custom Things (cursed.recode.hud.HudOther): the boss bars, the title, the action bar are the HUD's. */
    @Inject(method = "extractBossOverlay", at = @At("HEAD"), cancellable = true)
    private void cursed$bossBars(GuiGraphicsExtractor graphics, DeltaTracker deltaTracker, CallbackInfo ci) {
        if (cursed.recode.hud.HudOther.bars()) {
            ci.cancel();
        }
    }

    @Inject(method = "extractTitle", at = @At("HEAD"), cancellable = true)
    private void cursed$title(GuiGraphicsExtractor graphics, DeltaTracker deltaTracker, CallbackInfo ci) {
        if (cursed.recode.hud.HudOther.titles()) {
            ci.cancel();
        }
    }

    @Inject(method = "extractOverlayMessage", at = @At("HEAD"), cancellable = true)
    private void cursed$actionBar(GuiGraphicsExtractor graphics, DeltaTracker deltaTracker, CallbackInfo ci) {
        if (cursed.recode.hud.HudOther.action()) {
            ci.cancel();
        }
    }

    /** The Custom Scoreboard (cursed.recode.hud.HudBoard): the sidebar is the HUD's. */
    @Inject(method = "extractScoreboardSidebar", at = @At("HEAD"), cancellable = true)
    private void cursed$board(GuiGraphicsExtractor graphics, DeltaTracker deltaTracker, CallbackInfo ci) {
        if (cursed.recode.hud.HudBoard.active()) {
            ci.cancel();
        }
    }

    /** Its Effects element shows them: the game's own icons are left out (if the HUD says so). */
    @Inject(method = "extractEffects", at = @At("HEAD"), cancellable = true)
    private void cursed$effects(GuiGraphicsExtractor graphics, DeltaTracker deltaTracker, CallbackInfo ci) {
        if (cursed.recode.hud.HudLayer.hideEffects()) {
            ci.cancel();
        }
    }

    @Inject(method = "extractSavingIndicator", at = @At("HEAD"), cancellable = true)
    private void cursed$savingKept(GuiGraphicsExtractor graphics, DeltaTracker deltaTracker, CallbackInfo ci) {
        // "Saving world" is the menus' (cursed.recode.menus.Toasts, with the toasts), kept HUD or not
        boolean ours = !cursed.recode.dev.Dev.on("CURSED_DEV_VANILLASCREENS");
        if (ours) {
            Hud hud = (Hud) (Object) this;
            var mc = Minecraft.getInstance();
            float v = net.minecraft.util.Mth.clamp(net.minecraft.util.Mth.lerp(deltaTracker.getRealtimeDeltaTicks(), hud.lastAutosaveIndicatorValue, hud.autosaveIndicatorValue), 0.0F, 1.0F);
            cursed.recode.menus.Toasts.saving = cursed.recode.dev.Dev.on("CURSED_DEV_SAVING") ? 1.0F : mc.options.showAutosaveIndicator().get() ? v : 0.0F;
        }
        if (ours || HudCache.kept()) {
            ci.cancel();
        }
    }

    @Inject(method = "extractDebugOverlay", at = @At("HEAD"), cancellable = true)
    private void cursed$debugKept(GuiGraphicsExtractor graphics, CallbackInfo ci) {
        if (HudCache.kept()) {
            ci.cancel();
        }
    }

    /** No Render: a portal's and nausea's green over the view (cursed.recode.fx.NoRender). */
    @Inject(method = "extractPortalOverlay", at = @At("HEAD"), cancellable = true)
    private void cursed$noPortal(CallbackInfo ci) {
        if (cursed.recode.fx.NoRender.on("Nausea and portal")) {
            ci.cancel();
        }
    }

    @Inject(method = "extractConfusionOverlay", at = @At("HEAD"), cancellable = true)
    private void cursed$noConfusion(CallbackInfo ci) {
        if (cursed.recode.fx.NoRender.on("Nausea and portal")) {
            ci.cancel();
        }
    }

    /** No Render: a carved pumpkin's blur and powder snow's frost over the view. */
    @Redirect(method = "extractCameraOverlays", at = @At(value = "INVOKE",
            target = "Lnet/minecraft/client/gui/Hud;extractTextureOverlay(Lnet/minecraft/client/gui/GuiGraphicsExtractor;Lnet/minecraft/resources/Identifier;F)V"))
    private void cursed$overlay(Hud hud, GuiGraphicsExtractor graphics, net.minecraft.resources.Identifier texture, float alpha) {
        String path = texture.getPath();
        if (path.contains("pumpkinblur") && cursed.recode.fx.NoRender.on("Pumpkin") || path.contains("powder_snow") && cursed.recode.fx.NoRender.on("Powder snow")) {
            return;
        }
        ((HudAccess) hud).cursed$textureOverlay(graphics, texture, alpha);
    }

    /** No vignette (the class's note). */
    @Inject(method = "extractVignette", at = @At("HEAD"), cancellable = true)
    private void cursed$vignette(GuiGraphicsExtractor graphics, net.minecraft.world.entity.Entity camera, CallbackInfo ci) {
        ci.cancel();
    }
}
