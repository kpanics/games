package cursed.recode.mixin.client;

import com.mojang.blaze3d.vertex.PoseStack;
import net.minecraft.client.renderer.LevelRenderer;
import net.minecraft.client.renderer.SubmitNodeCollector;
import net.minecraft.client.renderer.state.level.LevelRenderState;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Cosmetics from behind (cursed.recode.cosmetics.Cosmetics.third): while the level's bodies are given (not a picture of
 *  the player in a menu). */
@Mixin(LevelRenderer.class)
public abstract class CosmeticsThirdLevelMixin {
    @Inject(method = "submitEntities", at = @At("HEAD"))
    private void cursed$begin(PoseStack poseStack, LevelRenderState state, SubmitNodeCollector out, CallbackInfo ci) {
        cursed.recode.cosmetics.Cosmetics.level(true);
    }

    @Inject(method = "submitEntities", at = @At("TAIL"))
    private void cursed$end(PoseStack poseStack, LevelRenderState state, SubmitNodeCollector out, CallbackInfo ci) {
        cursed.recode.cosmetics.Cosmetics.level(false);
    }
}
