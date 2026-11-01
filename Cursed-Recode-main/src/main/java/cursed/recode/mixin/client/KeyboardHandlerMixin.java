package cursed.recode.mixin.client;

import cursed.recode.gui.ClickGui;
import net.minecraft.client.KeyboardHandler;
import net.minecraft.client.Minecraft;
import net.minecraft.client.input.KeyEvent;
import org.spongepowered.asm.mixin.Final;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** The client's menu key (Right Shift unless changed in its Settings), pressed in a world with no screen open; the binds' keys;
 *  the F3 keys a player has no use for doing nothing (cursed.recode.hud.HudF3.cut), F3+C held never crashing the game;
 *  F3+B and F3+G the switches of the client's Hitboxes and Chunk Grid. */
@Mixin(KeyboardHandler.class)
public abstract class KeyboardHandlerMixin {
    @Shadow @Final private Minecraft minecraft;
    @Shadow private long debugCrashKeyTime;

    @Shadow
    protected abstract void debugFeedbackTranslated(String pattern);

    @Inject(method = "handleDebugKeys", at = @At("HEAD"), cancellable = true)
    private void cursed$cut(KeyEvent event, CallbackInfoReturnable<Boolean> cir) {
        var o = this.minecraft.options;
        for (net.minecraft.client.KeyMapping k : o.keyMappings) {
            if (cursed.recode.hud.HudF3.cut(o, k) && k.matches(event)) {
                // taken, nothing done
                cir.setReturnValue(true);
                return;
            }
        }
        // F3+B and F3+G while the client draws them: its modules' switches, the same as the menu's (the game's own
        // entries would show what the module had hidden, or keep what it hid shown)
        if (this.minecraft.player != null && o.keyDebugShowHitboxes.matches(event) && cursed.recode.render.Hitboxes.working()) {
            this.debugFeedbackTranslated(cursed.recode.render.Hitboxes.flip() ? "debug.show_hitboxes.on" : "debug.show_hitboxes.off");
            cir.setReturnValue(true);
        } else if (this.minecraft.player != null && o.keyDebugShowChunkBorders.matches(event) && cursed.recode.fx.Fx.working()) {
            this.debugFeedbackTranslated(cursed.recode.fx.ChunkGrid.flip() ? "debug.chunk_boundaries.on" : "debug.chunk_boundaries.off");
            cir.setReturnValue(true);
        }
    }

    @Inject(method = "tick", at = @At("HEAD"))
    private void cursed$noCrash(CallbackInfo ci) {
        this.debugCrashKeyTime = -1L;
    }

    @Inject(method = "keyPress", at = @At("HEAD"), cancellable = true)
    private void cursed$menu(long handle, int action, KeyEvent event, CallbackInfo ci) {
        if (handle != this.minecraft.getWindow().handle()) {
            return;
        }
        if (action == 1 && ClickGui.opens(this.minecraft, event)) {
            ci.cancel();
            return;
        }
        // the binds (gui/cards.rs): a key down in a world with no screen open; a key up always (a key held while a
        // screen opened is let go then, and a held bind goes back)
        String name = ClickGui.keyName(event.key());
        if (action == 1 && this.minecraft.gui.screen() == null && this.minecraft.player != null && cursed.recode.render.GuiFrame.active()) {
            cursed.recode.render.Native.guiCall("key\t1\t" + name);
        } else if (action == 0) {
            cursed.recode.render.Native.guiCall("key\t0\t" + name);
        }
    }
}
