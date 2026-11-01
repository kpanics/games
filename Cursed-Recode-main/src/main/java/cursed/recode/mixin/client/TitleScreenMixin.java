package cursed.recode.mixin.client;

import cursed.recode.CursedRecode;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.screens.TitleScreen;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

@Mixin(TitleScreen.class)
public abstract class TitleScreenMixin {
    @Inject(method = "init", at = @At("TAIL"))
    private void cursed$onInit(CallbackInfo ci) {
        // Minecraft.instance стал доступен через access widener — заодно проверка, что он работает.
        CursedRecode.LOGGER.info("Главное меню открыто, миксин работает; окно {}x{}",
                Minecraft.instance.getWindow().getWidth(), Minecraft.instance.getWindow().getHeight());
        CursedRecode.LOGGER.info("[shader cache] {}", cursed.recode.render.ShaderCache.stats());
    }
}
