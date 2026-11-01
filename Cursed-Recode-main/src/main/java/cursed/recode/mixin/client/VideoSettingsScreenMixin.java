package cursed.recode.mixin.client;

import java.util.Arrays;
import net.minecraft.client.OptionInstance;
import net.minecraft.client.Options;
import net.minecraft.client.gui.screens.options.VideoSettingsScreen;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

@Mixin(VideoSettingsScreen.class)
public abstract class VideoSettingsScreenMixin {
    /** No "Graphics API" choice: Cursed Recode draws only with its own Vulkan renderer. */
    @Inject(method = "displayOptions", at = @At("RETURN"), cancellable = true)
    private static void cursed$noGraphicsApiChoice(Options options, CallbackInfoReturnable<OptionInstance<?>[]> cir) {
        OptionInstance<?> api = options.preferredGraphicsBackend();
        OptionInstance<?>[] game = Arrays.stream(cir.getReturnValue()).filter(o -> o != api).toArray(OptionInstance[]::new);
        // The engine's quality and the world's size first (cursed.recode.engine.EngineSettings).
        OptionInstance<?>[] all = new OptionInstance<?>[game.length + 2];
        all[0] = cursed.recode.engine.EngineSettings.QUALITY;
        all[1] = cursed.recode.engine.EngineSettings.SCALE;
        System.arraycopy(game, 0, all, 2, game.length);
        cir.setReturnValue(all);
    }
}
