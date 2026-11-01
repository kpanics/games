package cursed.recode.mixin.client;

import cursed.recode.dev.Dev;
import cursed.recode.launch.LaunchScreen;
import java.util.List;
import java.util.function.Function;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.Gui;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.gui.screens.TitleScreen;
import org.spongepowered.asm.mixin.Final;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.ModifyVariable;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** Wherever the game opens its title screen, ours opens instead (cursed.recode.launch). CURSED_DEV_VANILLAMENU keeps the game's. */
@Mixin(Gui.class)
public abstract class GuiScreenMixin {
    private static final boolean VANILLA = Dev.on("CURSED_DEV_VANILLAMENU");

    @Shadow
    @Final
    private Minecraft minecraft;

    @ModifyVariable(method = "setScreen", at = @At("HEAD"), argsOnly = true)
    private Screen cursed$ourMenu(Screen screen) {
        return !VANILLA && screen != null && screen.getClass() == TitleScreen.class ? new LaunchScreen() : screen;
    }

    /**
     * The game's accessibility screen before the title on a first start is not shown: the window before the game waits
     * for our menu over it (a player's first start stood there for good, found 15.10 in a release's trial), and Recode's
     * settings have its choices.
     */
    @Inject(method = "addInitialScreens", at = @At("HEAD"))
    private void cursed$noOnboarding(List<Function<Runnable, Screen>> screens, CallbackInfoReturnable<Boolean> cir) {
        if (this.minecraft.options.onboardAccessibility) {
            this.minecraft.options.onboardingAccessibilityFinished();
        }
    }
}
