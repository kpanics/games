package cursed.recode.mixin.client;

import cursed.recode.menus.Menus;
import net.minecraft.client.MouseHandler;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.input.MouseButtonEvent;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Redirect;

/** The mouse on a screen the menus draw goes to them (cursed.recode.menus.Menus); on any other, to the screen. */
@Mixin(MouseHandler.class)
public abstract class MenusInputMixin {
    @Redirect(method = "onButton", at = @At(value = "INVOKE", target = "Lnet/minecraft/client/gui/screens/Screen;mouseClicked(Lnet/minecraft/client/input/MouseButtonEvent;Z)Z"))
    private boolean cursed$clicked(Screen screen, MouseButtonEvent event, boolean doubleClick) {
        return Menus.mouseClicked(screen, event, doubleClick);
    }

    @Redirect(method = "onButton", at = @At(value = "INVOKE", target = "Lnet/minecraft/client/gui/screens/Screen;mouseReleased(Lnet/minecraft/client/input/MouseButtonEvent;)Z"))
    private boolean cursed$released(Screen screen, MouseButtonEvent event) {
        return Menus.mouseReleased(screen, event);
    }

    @Redirect(method = "onScroll", at = @At(value = "INVOKE", target = "Lnet/minecraft/client/gui/screens/Screen;mouseScrolled(DDDD)Z"))
    private boolean cursed$scrolled(Screen screen, double x, double y, double scrollX, double scrollY) {
        return Menus.mouseScrolled(screen, x, y, scrollX, scrollY);
    }

    @Redirect(method = "handleAccumulatedMovement", at = @At(value = "INVOKE", target = "Lnet/minecraft/client/gui/screens/Screen;mouseMoved(DD)V"))
    private void cursed$moved(Screen screen, double x, double y) {
        Menus.mouseMoved(screen, x, y);
    }

    @Redirect(method = "handleAccumulatedMovement", at = @At(value = "INVOKE",
            target = "Lnet/minecraft/client/gui/screens/Screen;mouseDragged(Lnet/minecraft/client/input/MouseButtonEvent;DD)Z"))
    private boolean cursed$dragged(Screen screen, MouseButtonEvent event, double dx, double dy) {
        return Menus.mouseDragged(screen, event, dx, dy);
    }
}
