package cursed.recode.mixin.client;

import cursed.recode.launch.Chrome;
import net.minecraft.client.MouseHandler;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.ModifyVariable;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** The window's own title bar (cursed.recode.launch.Chrome): the cursor is where it is on the picture, under the bar. */
@Mixin(MouseHandler.class)
public abstract class MouseHandlerMixin {
    @Shadow
    private double ypos;

    @Shadow
    private boolean mouseGrabbed;

    @ModifyVariable(method = "onMove", at = @At("HEAD"), argsOnly = true, ordinal = 1)
    private double cursed$underBar(double ypos) {
        return ypos - Chrome.inset();
    }

    /** Dev (CURSED_DEV_NOMOUSE): the stand never takes the mouse — her cursor stays hers, the view stays put. */
    @Inject(method = {"grabMouse", "turnPlayer"}, at = @At("HEAD"), cancellable = true)
    private void cursed$devNoMouse(CallbackInfo ci) {
        if (NO_MOUSE) {
            ci.cancel();
        }
    }

    @org.spongepowered.asm.mixin.Unique
    private static final boolean NO_MOUSE = cursed.recode.dev.Dev.on("CURSED_DEV_NOMOUSE");

    /** The binds on the mouse's other buttons (the middle, the side ones): down in a world with no screen, up always. */
    @Inject(method = "onButton", at = @At("HEAD"))
    private void cursed$binds(long handle, net.minecraft.client.input.MouseButtonInfo info, int action, CallbackInfo ci) {
        var mc = net.minecraft.client.Minecraft.getInstance();
        int b = info.button();
        boolean left = b == com.mojang.blaze3d.platform.InputConstants.MOUSE_BUTTON_LEFT, right = b == com.mojang.blaze3d.platform.InputConstants.MOUSE_BUTTON_RIGHT;
        if (handle != mc.getWindow().handle()) {
            return;
        }
        if (left || right) {
            // the clicks a second (the HUD's {cps}, {rcps})
            if (action == 1 && mc.gui.screen() == null && mc.player != null) {
                cursed.recode.hud.HudVars.click(left);
            }
            return;
        }
        String name = b == com.mojang.blaze3d.platform.InputConstants.MOUSE_BUTTON_MIDDLE ? "Middle" : "Mouse " + b;
        if (action == 1 && mc.gui.screen() == null && mc.player != null && cursed.recode.render.GuiFrame.active()) {
            cursed.recode.render.Native.guiCall("key\t1\t" + name);
        } else if (action == 0) {
            cursed.recode.render.Native.guiCall("key\t0\t" + name);
        }
    }

    /** Custom Inventory: over a container's block whose clicks the game works out itself, the pointer in its own px. */
    @Inject(method = "getScaledXPos(Lcom/mojang/blaze3d/platform/Window;)D", at = @At("RETURN"), cancellable = true)
    private void cursed$packX(com.mojang.blaze3d.platform.Window window, org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable<Double> cir) {
        cir.setReturnValue(cursed.recode.hud.HudPack.gameX(cir.getReturnValueD(), this.xpos, this.ypos));
    }

    @Inject(method = "getScaledYPos(Lcom/mojang/blaze3d/platform/Window;)D", at = @At("RETURN"), cancellable = true)
    private void cursed$packY(com.mojang.blaze3d.platform.Window window, org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable<Double> cir) {
        cir.setReturnValue(cursed.recode.hud.HudPack.gameY(cir.getReturnValueD(), this.xpos, this.ypos));
    }

    @org.spongepowered.asm.mixin.Shadow
    private double xpos;

    @Inject(method = "resyncMousePosition", at = @At("TAIL"))
    private void cursed$resyncUnderBar(CallbackInfo ci) {
        if (!this.mouseGrabbed) {
            this.ypos -= Chrome.inset();
        }
    }
}
