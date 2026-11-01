package cursed.recode.mixin.client;

import cursed.recode.dev.Dev;
import cursed.recode.scene.GuiNodeIndex;
import java.util.List;
import net.minecraft.client.gui.navigation.ScreenRectangle;
import net.minecraft.client.renderer.state.gui.GuiRenderState;
import org.spongepowered.asm.mixin.Final;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/**
 * The layer search for a new GUI element (navigateToAboveHighestElementWithIntersectingBounds) over the layers' int
 * rectangles (GuiRenderStateNodeMixin): the same walk from the top layer down, the same answer. CURSED_DEV_NOGUIINDEX
 * leaves the game's own.
 */
@Mixin(GuiRenderState.class)
public abstract class GuiRenderStateMixin implements cursed.recode.scene.HudCache.GuiStrata {
    private static final boolean OFF = Dev.on("CURSED_DEV_NOGUIINDEX");
    /** CURSED_DEV_NOGLYPHRUNS: one GUI element per glyph, as the game makes them. */
    private static final boolean NO_RUNS = Dev.on("CURSED_DEV_NOGLYPHRUNS");
    /** CURSED_DEV_GUICHECK: the game's own search next to ours, every element; a different layer goes to the log. */
    private static final boolean CHECK = Dev.on("CURSED_DEV_GUICHECK");
    private static long checked;
    private static long differ;

    @Shadow
    @Final
    private List<GuiRenderState.Node> strata;
    @Shadow
    private GuiRenderState.Node current;
    @Shadow
    private int firstStratumAfterBlur;

    @Shadow
    public abstract void up();

    /** The rectangles of every layer of the strata after the blur's index (the HUD's upper strata: HudCache). */
    @Override
    public void cursed$coverAfterBlur(cursed.recode.scene.HudCache.Rects rects) {
        for (int k = this.firstStratumAfterBlur; k < this.strata.size(); k++) {
            for (GuiRenderState.Node node = this.strata.get(k); node != null; node = node.up) {
                ((GuiNodeIndex) (Object) node).cursed$each(rects);
            }
        }
    }

    /** A new frame of the GUI: the menus' item stage begins one too. */
    @Inject(method = "reset", at = @At("HEAD"))
    private void cursed$reset(CallbackInfo ci) {
        cursed.recode.menus.ItemStage.newFrame();
    }

    /** GUI glyphs go into runs (cursed.recode.scene.GuiGlyphRun). */
    @Inject(method = "addGlyphToCurrentLayer", at = @At("HEAD"), cancellable = true)
    private void cursed$glyph(net.minecraft.client.renderer.state.gui.GuiElementRenderState glyphState, CallbackInfo ci) {
        if (!NO_RUNS && glyphState instanceof net.minecraft.client.renderer.state.gui.GlyphRenderState glyph) {
            cursed.recode.scene.GuiGlyphRun.add(this.current, glyph);
            ci.cancel();
        }
    }

    @Inject(method = "navigateToAboveHighestElementWithIntersectingBounds", at = @At("HEAD"), cancellable = true)
    private void cursed$navigate(ScreenRectangle bounds, CallbackInfo ci) {
        if (OFF) {
            return;
        }
        int left = bounds.left();
        int top = bounds.top();
        int right = bounds.right();
        int bottom = bounds.bottom();
        GuiRenderState.Node node = this.strata.getLast();
        while (node.up != null) {
            node = node.up;
        }
        boolean found = false;
        while (!found) {
            found = ((GuiNodeIndex) (Object) node).cursed$intersects(left, top, right, bottom);
            if (node.parent == null) {
                break;
            }
            if (!found) {
                node = node.parent;
            }
        }
        if (CHECK) {
            this.cursed$check(bounds, node, found);
        }
        this.current = node;
        if (found) {
            this.up();
        }
        ci.cancel();
    }

    private void cursed$check(ScreenRectangle bounds, GuiRenderState.Node ours, boolean oursFound) {
        GuiRenderState.Node node = this.strata.getLast();
        while (node.up != null) {
            node = node.up;
        }
        boolean found = false;
        while (!found) {
            found = cursed$hits(bounds, node.elementStates) || cursed$hits(bounds, node.itemStates) || cursed$hits(bounds, node.textStates)
                    || cursed$hits(bounds, node.picturesInPictureStates);
            if (node.parent == null) {
                break;
            }
            if (!found) {
                node = node.parent;
            }
        }
        checked++;
        if (node != ours || found != oursFound) {
            differ++;
            if (differ < 20) {
                com.mojang.logging.LogUtils.getLogger().warn("[gui index] layer differs for {}", bounds);
            }
        }
        if (checked % 20000 == 0) {
            com.mojang.logging.LogUtils.getLogger().info("[gui index] checked {}, {} differ", checked, differ);
        }
    }

    private static boolean cursed$hits(ScreenRectangle bounds, java.util.List<? extends net.minecraft.client.renderer.state.gui.ScreenArea> states) {
        if (states != null) {
            for (net.minecraft.client.renderer.state.gui.ScreenArea a : states) {
                ScreenRectangle r = a.bounds();
                if (r != null && r.intersects(bounds)) {
                    return true;
                }
            }
        }
        return false;
    }
}
