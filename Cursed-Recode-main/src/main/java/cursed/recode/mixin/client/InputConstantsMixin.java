package cursed.recode.mixin.client;

import com.mojang.blaze3d.platform.InputConstants;
import cursed.recode.launch.Chrome;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.ModifyArg;

/** The window's own title bar (cursed.recode.launch.Chrome): the cursor put on the picture's point, under the bar. */
@Mixin(InputConstants.class)
public abstract class InputConstantsMixin {
    @ModifyArg(method = {"grabMouse", "releaseMouse"}, at = @At(value = "INVOKE", target = "Lorg/lwjgl/sdl/SDLMouse;SDL_WarpMouseInWindow(JFF)V"), index = 2)
    private static float cursed$warpUnderBar(float y) {
        return y + Chrome.inset();
    }
}
