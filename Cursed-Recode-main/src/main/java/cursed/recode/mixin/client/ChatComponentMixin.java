package cursed.recode.mixin.client;

import com.llamalad7.mixinextras.sugar.Local;
import cursed.recode.hud.HudChat;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.components.ChatComponent;
import net.minecraft.client.multiplayer.chat.GuiMessage;
import net.minecraft.client.multiplayer.chat.GuiMessageSource;
import net.minecraft.client.multiplayer.chat.GuiMessageTag;
import net.minecraft.network.chat.Component;
import net.minecraft.network.chat.MessageSignature;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Constant;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.ModifyArg;
import org.spongepowered.asm.mixin.injection.ModifyConstant;
import org.spongepowered.asm.mixin.injection.Redirect;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/**
 * The Custom Chat (cursed.recode.hud.HudChat): the game still writes the messages (a server's glyphs and colours, its
 * clicks and hovers as they were), but where the chat is, how wide and big, how its lines come and go and what is
 * behind them are the client's — the HUD draws the glass, the times and the highlights under the words.
 */
@Mixin(ChatComponent.class)
public abstract class ChatComponentMixin {
    private static final String DRAW = "extractRenderState(Lnet/minecraft/client/gui/GuiGraphicsExtractor;Lnet/minecraft/client/gui/Font;IIILnet/minecraft/client/gui/components/ChatComponent$DisplayMode;Z)V";
    private static final String INNER = "Lnet/minecraft/client/gui/components/ChatComponent;extractRenderState(Lnet/minecraft/client/gui/components/ChatComponent$ChatGraphicsAccess;IILnet/minecraft/client/gui/components/ChatComponent$DisplayMode;)V";

    /** The lines drawn through the Custom Chat: moved, faded, a head before a message, no plates of the game's. */
    @ModifyArg(method = DRAW, at = @At(value = "INVOKE", target = INNER), index = 0)
    private ChatComponent.ChatGraphicsAccess cursed$draw(ChatComponent.ChatGraphicsAccess access, @Local(argsOnly = true) GuiGraphicsExtractor graphics) {
        return HudChat.wrap(access, graphics);
    }

    /** Clicks find the words where they are drawn. */
    @ModifyArg(method = "captureClickableText", at = @At(value = "INVOKE", target = INNER), index = 0)
    private ChatComponent.ChatGraphicsAccess cursed$clicks(ChatComponent.ChatGraphicsAccess access) {
        return HudChat.wrap(access, null);
    }

    /** How long a line stays is the Custom Chat's: to the game every line is new (it fades them itself). */
    @ModifyArg(method = DRAW, at = @At(value = "INVOKE", target = INNER), index = 2)
    private int cursed$ticks(int ticks) {
        return HudChat.active() ? -1_000_000 : ticks;
    }

    /** While the lines slide, one more comes in at the top (it fades in and out there instead of popping). */
    @Redirect(method = "forEachLine", at = @At(value = "INVOKE", target = "Lnet/minecraft/client/gui/components/ChatComponent;getLinesPerPage()I"))
    private int cursed$lines(ChatComponent chat) {
        return chat.getLinesPerPage() + HudChat.extraLines();
    }

    @Inject(method = "getWidth()I", at = @At("HEAD"), cancellable = true)
    private void cursed$width(CallbackInfoReturnable<Integer> cir) {
        int w = HudChat.width();
        if (w > 0) {
            cir.setReturnValue(w);
        }
    }

    @Inject(method = "getHeight()I", at = @At("HEAD"), cancellable = true)
    private void cursed$height(CallbackInfoReturnable<Integer> cir) {
        int h = HudChat.height((ChatComponent) (Object) this);
        if (h > 0) {
            cir.setReturnValue(h);
        }
    }

    @Inject(method = "getScale()D", at = @At("HEAD"), cancellable = true)
    private void cursed$scale(CallbackInfoReturnable<Double> cir) {
        double s = HudChat.scale();
        if (s > 0.0) {
            cir.setReturnValue(s);
        }
    }

    /** The client's own chat draws instead of the game's (cursed.recode.hud.NChat). */
    @Inject(method = DRAW, at = @At("HEAD"), cancellable = true)
    private void cursed$own(GuiGraphicsExtractor graphics, net.minecraft.client.gui.Font font, int ticks, int mouseX, int mouseY,
                            ChatComponent.DisplayMode mode, boolean insertions, CallbackInfo ci) {
        if (cursed.recode.hud.NChat.on()) {
            ci.cancel();
        }
    }

    /** The same message again: the one before counts it (×2, ×3…) instead of another line. */
    @Inject(method = "addMessage", at = @At("HEAD"), cancellable = true)
    private void cursed$add(Component contents, MessageSignature signature, GuiMessageSource source, GuiMessageTag tag, CallbackInfo ci) {
        // every message to the client's own chat too (it counts repeats itself); to the acting modules (Auto Accept)
        cursed.recode.hud.NChat.add(contents, source);
        cursed.recode.play.Play.chat(contents, source);
        if (HudChat.repeat((ChatComponent) (Object) this, contents, signature, source, tag)) {
            ci.cancel();
            return;
        }
        HudChat.adding = true;
    }

    @Inject(method = "addMessage", at = @At("RETURN"))
    private void cursed$added(Component contents, MessageSignature signature, GuiMessageSource source, GuiMessageTag tag, CallbackInfo ci) {
        HudChat.adding = false;
    }

    @Inject(method = "addMessageToDisplayQueue", at = @At("HEAD"))
    private void cursed$queue(GuiMessage message, CallbackInfo ci) {
        HudChat.queueing((ChatComponent) (Object) this, message);
    }

    @Inject(method = "addMessageToDisplayQueue", at = @At("RETURN"))
    private void cursed$queued(GuiMessage message, CallbackInfo ci) {
        HudChat.queued((ChatComponent) (Object) this, message);
    }

    /** As many messages kept as the Custom Chat says (the game: 100). */
    @ModifyConstant(method = {"addMessageToDisplayQueue", "addMessageToQueue"}, constant = @Constant(intValue = 100))
    private int cursed$history(int n) {
        return HudChat.history(n);
    }

    @Inject(method = "scrollChat", at = @At("HEAD"))
    private void cursed$scrolling(int dir, CallbackInfo ci) {
        HudChat.scrolling((ChatComponent) (Object) this);
    }

    @Inject(method = "scrollChat", at = @At("RETURN"))
    private void cursed$scrolled(int dir, CallbackInfo ci) {
        HudChat.scrolled((ChatComponent) (Object) this);
    }

    @Inject(method = "clearMessages", at = @At("HEAD"))
    private void cursed$cleared(boolean history, CallbackInfo ci) {
        HudChat.cleared();
        cursed.recode.hud.NChat.cleared();
    }
}
