package cursed.recode.mixin.client;

import cursed.recode.scene.TextCache;
import net.minecraft.client.gui.font.FontManager;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Fonts reloaded: the kept layouts refer to old glyphs (cursed.recode.scene.TextCache). */
@Mixin(FontManager.class)
public abstract class TextReloadMixin {
    @Inject(method = "apply", at = @At("TAIL"))
    private void cursed$forget(CallbackInfo ci) {
        TextCache.clear();
    }
}
