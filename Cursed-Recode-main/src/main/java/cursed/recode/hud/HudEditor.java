package cursed.recode.hud;

import com.mojang.blaze3d.platform.InputConstants;
import cursed.recode.render.Native;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.input.CharacterEvent;
import net.minecraft.client.input.KeyEvent;
import net.minecraft.client.input.MouseButtonEvent;
import net.minecraft.network.chat.Component;

/**
 * The HUD's layout editor (the menu's "Edit the layout"): every element shown, even those off or with nothing to show;
 * the left button moves one, the right one opens its settings by it (gui/hud.rs). Esc finishes.
 */
public final class HudEditor extends Screen {
    public HudEditor() {
        super(Component.literal("Interface"));
    }

    private float scale() {
        return (float) this.minecraft.getWindow().getGuiScale();
    }

    @Override
    protected void init() {
        // letters come only while the game takes text (SDL_StartTextInput): the settings' fields
        this.minecraft.textInputManager().startTextInput(this);
    }

    @Override
    public boolean isPauseScreen() {
        return false;
    }

    @Override
    public boolean shouldCloseOnEsc() {
        return false;
    }

    @Override
    public void extractBackground(final GuiGraphicsExtractor graphics, final int mouseX, final int mouseY, final float a) {
    }

    @Override
    public void extractRenderState(final GuiGraphicsExtractor graphics, final int mouseX, final int mouseY, final float a) {
        HudLayer.overlay(graphics);
    }

    @Override
    public boolean mouseClicked(final MouseButtonEvent event, final boolean doubleClick) {
        float s = this.scale();
        Native.hudMouse((float) event.x() * s, (float) event.y() * s, HudLayer.button(event.button()), 1);
        return true;
    }

    @Override
    public boolean mouseReleased(final MouseButtonEvent event) {
        float s = this.scale();
        Native.hudMouse((float) event.x() * s, (float) event.y() * s, HudLayer.button(event.button()), 2);
        return true;
    }

    @Override
    public boolean mouseScrolled(final double x, final double y, final double scrollX, final double scrollY) {
        Native.hudScroll((float) scrollY);
        return true;
    }

    @Override
    public boolean keyPressed(final KeyEvent event) {
        // a bind being recorded (a setting's card) takes every key
        if ("1".equals(Native.guiCall("hudkey\t1\t" + cursed.recode.gui.ClickGui.keyName(event.key())))) {
            return true;
        }
        int k = event.key();
        boolean enter = k == InputConstants.KEY_RETURN || k == InputConstants.KEY_NUMPADENTER;
        if (!Native.hudKey(event.isEscape(), enter, k == InputConstants.KEY_BACKSPACE) && event.isEscape()) {
            this.onClose();
        }
        return true;
    }

    @Override
    public boolean keyReleased(final KeyEvent event) {
        Native.guiCall("hudkey\t0\t" + cursed.recode.gui.ClickGui.keyName(event.key()));
        return true;
    }

    @Override
    public void tick() {
        cursed.recode.gui.GuiAsks.poll(this.minecraft);
    }

    @Override
    public boolean charTyped(final CharacterEvent event) {
        Native.hudChar(event.codepoint());
        return true;
    }

    @Override
    public void removed() {
        this.minecraft.textInputManager().stopTextInput(this);
        Native.hudClosed();
    }
}
