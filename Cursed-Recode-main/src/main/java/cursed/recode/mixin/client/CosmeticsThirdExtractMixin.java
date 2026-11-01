package cursed.recode.mixin.client;

import net.minecraft.client.renderer.extract.LevelExtractor;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Cosmetics from behind (cursed.recode.cosmetics.Cosmetics.thirdPose): while the level's bodies are read off the world
 *  (not a picture of the player in a menu). */
@Mixin(LevelExtractor.class)
public abstract class CosmeticsThirdExtractMixin {
    @Inject(method = "extractVisibleEntities", at = @At("HEAD"))
    private void cursed$begin(CallbackInfo ci) {
        cursed.recode.cosmetics.Cosmetics.extracting(true);
    }

    @Inject(method = "extractVisibleEntities", at = @At("TAIL"))
    private void cursed$end(CallbackInfo ci) {
        cursed.recode.cosmetics.Cosmetics.extracting(false);
    }
}
