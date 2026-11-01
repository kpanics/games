package cursed.recode.mixin.client;

import cursed.recode.menus.Menus;
import net.minecraft.client.KeyboardHandler;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.input.CharacterEvent;
import net.minecraft.client.input.KeyEvent;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Redirect;

/** The keys on a screen the menus draw go to them (cursed.recode.menus.Menus); on any other, to the screen. */
@Mixin(KeyboardHandler.class)
public abstract class MenusKeysMixin {
    @Redirect(method = "keyPress", at = @At(value = "INVOKE", target = "Lnet/minecraft/client/gui/screens/Screen;keyPressed(Lnet/minecraft/client/input/KeyEvent;)Z"))
    private boolean cursed$pressed(Screen screen, KeyEvent event) {
        return Menus.keyPressed(screen, event);
    }

    @Redirect(method = "keyPress", at = @At(value = "INVOKE", target = "Lnet/minecraft/client/gui/screens/Screen;keyReleased(Lnet/minecraft/client/input/KeyEvent;)Z"))
    private boolean cursed$released(Screen screen, KeyEvent event) {
        return Menus.keyReleased(screen, event);
    }

    @Redirect(method = "charTyped", at = @At(value = "INVOKE", target = "Lnet/minecraft/client/gui/screens/Screen;charTyped(Lnet/minecraft/client/input/CharacterEvent;)Z"))
    private boolean cursed$typed(Screen screen, CharacterEvent event) {
        return Menus.charTyped(screen, event);
    }
}
