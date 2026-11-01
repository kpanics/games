package cursed.recode.mixin.client;

import cursed.recode.dev.Dev;
import cursed.recode.launch.LaunchScreen;
import cursed.recode.menus.Menus;
import net.minecraft.client.gui.Gui;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.gui.screens.TitleScreen;
import org.objectweb.asm.Opcodes;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.ModifyVariable;
import org.spongepowered.asm.mixin.injection.Redirect;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/**
 * The game's screens drawn by the menus (cursed.recode.menus): the screen's picture is theirs, and they hear when the
 * game changes its screen. The title screen the game makes for itself (no world, no screen asked) is ours too.
 */
@Mixin(Gui.class)
public abstract class MenusGuiMixin {
    private static final boolean VANILLA = Dev.on("CURSED_DEV_VANILLAMENU");

    @Shadow
    public abstract Screen screen();

    @Redirect(method = "extractRenderState", at = @At(value = "INVOKE",
            target = "Lnet/minecraft/client/gui/screens/Screen;extractRenderStateWithTooltipAndSubtitles(Lnet/minecraft/client/gui/GuiGraphicsExtractor;IIF)V"))
    private void cursed$menus(Screen screen, GuiGraphicsExtractor graphics, int mouseX, int mouseY, float a) {
        Menus.extract(screen, graphics, mouseX, mouseY, a);
    }

    /** The window closed in a world: the world and all over it go dark before it is left (LaunchScreen.quitVeil). */
    @Inject(method = "extractRenderState", at = @At("RETURN"))
    private void cursed$quitVeil(net.minecraft.client.DeltaTracker deltaTracker, boolean shouldRenderLevel, boolean resourcesLoaded, CallbackInfo ci,
                                 @com.llamalad7.mixinextras.sugar.Local GuiGraphicsExtractor graphics) {
        float v = LaunchScreen.quitVeil();
        if (v > 0.0F) {
            graphics.fill(0, 0, graphics.guiWidth(), graphics.guiHeight(), Math.round(v * 255.0F) << 24);
        }
        // the way out of a world (v12): its frame crumbling into the line, over everything
        cursed.recode.launch.Transition.draw(graphics);
    }

    @Inject(method = "setScreen", at = @At("HEAD"))
    private void cursed$changing(Screen screen, CallbackInfo ci) {
        Menus.changing(this.screen());
    }

    @Inject(method = "setScreen", at = @At("TAIL"))
    private void cursed$changed(Screen screen, CallbackInfo ci) {
        Menus.changed(this.screen());
    }

    /** setScreen(null) with no world makes the game's title screen itself: ours instead. */
    @ModifyVariable(method = "setScreen", argsOnly = true,
            at = @At(value = "FIELD", target = "Lnet/minecraft/client/gui/Gui;screen:Lnet/minecraft/client/gui/screens/Screen;", opcode = Opcodes.PUTFIELD))
    private Screen cursed$ourTitle(Screen screen) {
        return !VANILLA && screen != null && screen.getClass() == TitleScreen.class ? new LaunchScreen() : screen;
    }
}
