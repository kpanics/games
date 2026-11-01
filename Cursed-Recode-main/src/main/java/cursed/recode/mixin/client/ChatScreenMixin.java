package cursed.recode.mixin.client;

import com.mojang.blaze3d.platform.InputConstants;
import cursed.recode.hud.HudLayer;
import cursed.recode.render.Native;
import net.minecraft.client.gui.screens.ChatScreen;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.input.CharacterEvent;
import net.minecraft.client.input.KeyEvent;
import net.minecraft.client.input.MouseButtonEvent;
import net.minecraft.network.chat.Component;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/**
 * The chat edits the HUD (gui/hud.rs): the left button drags an element, the right one opens its settings by it. What
 * the HUD does not take is the chat's as before.
 */
@Mixin(ChatScreen.class)
public abstract class ChatScreenMixin extends Screen {
    protected ChatScreenMixin(Component title) {
        super(title);
    }

    private float cursed$scale() {
        return (float) this.minecraft.getWindow().getGuiScale();
    }

    @Inject(method = "mouseClicked", at = @At("HEAD"), cancellable = true)
    private void cursed$hudPress(MouseButtonEvent event, boolean doubleClick, CallbackInfoReturnable<Boolean> cir) {
        float s = this.cursed$scale();
        if (Native.hudMouse((float) event.x() * s, (float) event.y() * s, HudLayer.button(event.button()), 1)) {
            cir.setReturnValue(true);
            return;
        }
        // the client's own chat: nothing of the game's chat under the pointer (it is not drawn)
        if (cursed.recode.hud.NChat.on()) {
            cir.setReturnValue(true);
            return;
        }
        // the Custom Chat: a right click on a message copies it
        if (HudLayer.button(event.button()) == 1 && cursed.recode.hud.HudChat.copyAt((float) event.x() * s, (float) event.y() * s)) {
            cir.setReturnValue(true);
        }
    }

    /** The client's own chat draws the field and the hints (the HUD); only what is open by an element stays here. */
    @Inject(method = "extractRenderState", at = @At("HEAD"), cancellable = true)
    private void cursed$own(net.minecraft.client.gui.GuiGraphicsExtractor graphics, int mouseX, int mouseY, float a, CallbackInfo ci) {
        if (cursed.recode.hud.NChat.on()) {
            HudLayer.overlay(graphics);
            ci.cancel();
        }
    }

    /** The Custom Chat draws the field's plate (the HUD, under it). */
    @org.spongepowered.asm.mixin.injection.Redirect(method = "extractRenderState", at = @At(value = "INVOKE", ordinal = 0,
            target = "Lnet/minecraft/client/gui/GuiGraphicsExtractor;fill(IIIII)V"))
    private void cursed$field(net.minecraft.client.gui.GuiGraphicsExtractor graphics, int x0, int y0, int x1, int y1, int color) {
        if (!cursed.recode.hud.HudChat.ownInput()) {
            graphics.fill(x0, y0, x1, y1, color);
        }
    }

    @Override
    public boolean mouseReleased(MouseButtonEvent event) {
        float s = this.cursed$scale();
        if (Native.hudMouse((float) event.x() * s, (float) event.y() * s, HudLayer.button(event.button()), 2)) {
            return true;
        }
        return super.mouseReleased(event);
    }

    @Override
    public boolean charTyped(CharacterEvent event) {
        if (Native.hudChar(event.codepoint())) {
            return true;
        }
        return super.charTyped(event);
    }

    @Inject(method = "mouseScrolled", at = @At("HEAD"), cancellable = true)
    private void cursed$hudScroll(double x, double y, double scrollX, double scrollY, CallbackInfoReturnable<Boolean> cir) {
        if (Native.hudScroll((float) scrollY)) {
            cir.setReturnValue(true);
        }
    }

    @Inject(method = "keyPressed", at = @At("HEAD"), cancellable = true)
    private void cursed$hudKey(KeyEvent event, CallbackInfoReturnable<Boolean> cir) {
        int k = event.key();
        boolean enter = k == InputConstants.KEY_RETURN || k == InputConstants.KEY_NUMPADENTER;
        ChatScreen self = (ChatScreen) (Object) this;
        // the client's own chat's search: Ctrl+F opens and closes it
        if (cursed.recode.hud.NChat.on() && k == InputConstants.KEY_F && event.hasControlDown()) {
            cursed.recode.hud.NChat.find(self, !cursed.recode.hud.NChat.finding());
            cir.setReturnValue(true);
            return;
        }
        if (Native.hudKey(event.isEscape(), enter, k == InputConstants.KEY_BACKSPACE)) {
            cir.setReturnValue(true);
            return;
        }
        // Server Adaptation's chat filter: a line it holds back stays in the field, unsent (Enter again sends it)
        if (enter && !cursed.recode.hud.NChat.finding() && cursed.recode.play.Adapt.heldBack(((ChatScreenAccess) self).cursed$input().getValue())) {
            cir.setReturnValue(true);
            return;
        }
        // the client's own chat in a conversation: the words go to them, the chat stays on it
        if (enter && cursed.recode.hud.NChat.say(self)) {
            cir.setReturnValue(true);
            return;
        }
        // while it searches the field is its words: Enter sends nothing, Esc closes the search (not the chat)
        if (cursed.recode.hud.NChat.finding() && (enter || event.isEscape())) {
            if (event.isEscape()) {
                cursed.recode.hud.NChat.find(self, false);
            }
            cir.setReturnValue(true);
        }
    }

    /** What is open by an element (its settings, the colour wheel), over the chat and the rest of the GUI. */
    @Inject(method = "extractRenderState", at = @At("TAIL"))
    private void cursed$hudOverlay(net.minecraft.client.gui.GuiGraphicsExtractor graphics, int mouseX, int mouseY, float a, CallbackInfo ci) {
        HudLayer.overlay(graphics);
    }

    @Inject(method = "removed", at = @At("HEAD"))
    private void cursed$hudClosed(CallbackInfo ci) {
        Native.hudClosed();
    }
}
