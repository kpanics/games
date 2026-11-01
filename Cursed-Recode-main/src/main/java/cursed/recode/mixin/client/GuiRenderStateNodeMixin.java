package cursed.recode.mixin.client;

import cursed.recode.scene.GuiNodeIndex;
import net.minecraft.client.gui.navigation.ScreenRectangle;
import net.minecraft.client.renderer.state.gui.GuiElementRenderState;
import net.minecraft.client.renderer.state.gui.GuiItemRenderState;
import net.minecraft.client.renderer.state.gui.GuiTextRenderState;
import net.minecraft.client.renderer.state.gui.pip.PictureInPictureRenderState;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Unique;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/**
 * A GUI layer's element rectangles as plain ints next to the game's lists (the ones GuiRenderState.hasIntersection
 * reads: elements, items, texts, pictures — not glyphs), with their bounding box: the search for a new element's layer
 * reads these instead of asking every element for its bounds.
 */
@Mixin(targets = "net.minecraft.client.renderer.state.gui.GuiRenderState$Node")
public abstract class GuiRenderStateNodeMixin implements GuiNodeIndex {
    @Unique
    private int[] cursed$rects = new int[64];
    @Unique
    private int cursed$count;
    @Unique
    private int cursed$minLeft = Integer.MAX_VALUE;
    @Unique
    private int cursed$minTop = Integer.MAX_VALUE;
    @Unique
    private int cursed$maxRight = Integer.MIN_VALUE;
    @Unique
    private int cursed$maxBottom = Integer.MIN_VALUE;

    @Unique
    private void cursed$add(ScreenRectangle r) {
        if (r == null) {
            return;
        }
        int n = this.cursed$count * 4;
        if (n + 4 > this.cursed$rects.length) {
            this.cursed$rects = java.util.Arrays.copyOf(this.cursed$rects, this.cursed$rects.length * 2);
        }
        int left = r.left();
        int top = r.top();
        int right = r.right();
        int bottom = r.bottom();
        this.cursed$rects[n] = left;
        this.cursed$rects[n + 1] = top;
        this.cursed$rects[n + 2] = right;
        this.cursed$rects[n + 3] = bottom;
        this.cursed$count++;
        this.cursed$minLeft = Math.min(this.cursed$minLeft, left);
        this.cursed$minTop = Math.min(this.cursed$minTop, top);
        this.cursed$maxRight = Math.max(this.cursed$maxRight, right);
        this.cursed$maxBottom = Math.max(this.cursed$maxBottom, bottom);
    }

    @Inject(method = "addItem", at = @At("TAIL"))
    private void cursed$item(GuiItemRenderState state, CallbackInfo ci) {
        this.cursed$add(state.bounds());
    }

    @Inject(method = "addText", at = @At("TAIL"))
    private void cursed$text(GuiTextRenderState state, CallbackInfo ci) {
        this.cursed$add(state.bounds());
    }

    @Inject(method = "addPicturesInPictureState", at = @At("TAIL"))
    private void cursed$picture(PictureInPictureRenderState state, CallbackInfo ci) {
        this.cursed$add(state.bounds());
    }

    @Inject(method = "addGuiElement", at = @At("TAIL"))
    private void cursed$element(GuiElementRenderState state, CallbackInfo ci) {
        this.cursed$add(state.bounds());
    }

    @Override
    public void cursed$each(cursed.recode.scene.HudCache.Rects rects) {
        int[] r = this.cursed$rects;
        for (int i = 0, n = this.cursed$count * 4; i < n; i += 4) {
            rects.accept(r[i], r[i + 1], r[i + 2], r[i + 3]);
        }
    }

    /** ScreenRectangle.intersects of any of this layer's rectangles with (left, top, right, bottom). */
    @Override
    public boolean cursed$intersects(int left, int top, int right, int bottom) {
        if (this.cursed$count == 0) {
            return false;
        }
        // Outside the box around them all: none of them can intersect (exactly as the rectangles themselves would say).
        if (!(this.cursed$minLeft < right && this.cursed$maxRight > left && this.cursed$minTop < bottom && this.cursed$maxBottom > top)) {
            return false;
        }
        int[] r = this.cursed$rects;
        for (int i = 0, n = this.cursed$count * 4; i < n; i += 4) {
            if (r[i] < right && r[i + 2] > left && r[i + 1] < bottom && r[i + 3] > top) {
                return true;
            }
        }
        return false;
    }
}
