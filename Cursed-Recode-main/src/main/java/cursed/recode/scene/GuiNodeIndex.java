package cursed.recode.scene;

/** A GUI layer's rectangles (mixin GuiRenderStateNodeMixin). */
public interface GuiNodeIndex {
    /** Whether any element of the layer intersects the rectangle, as ScreenRectangle.intersects decides. */
    boolean cursed$intersects(int left, int top, int right, int bottom);

    /** Each of the layer's rectangles (HudCache: where the HUD has something). */
    void cursed$each(HudCache.Rects rects);
}
