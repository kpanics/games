package cursed.recode.mixin.client;

import cursed.recode.scene.TextCache;
import net.minecraft.client.resources.language.LanguageManager;
import net.minecraft.server.packs.resources.ResourceManager;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** The language changed: name tags' visual order may go the other way (cursed.recode.scene.TextCache). */
@Mixin(LanguageManager.class)
public abstract class LanguageManagerMixin {
    @Inject(method = "onResourceManagerReload", at = @At("TAIL"))
    private void cursed$forget(ResourceManager resourceManager, CallbackInfo ci) {
        TextCache.clear();
    }
}
