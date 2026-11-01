package cursed.recode.mixin.client;

import com.mojang.blaze3d.vertex.PoseStack;
import cursed.recode.chams.ChamsHooks;
import net.minecraft.client.renderer.LevelRenderer;
import net.minecraft.client.renderer.SubmitNodeCollector;
import net.minecraft.client.renderer.state.level.LevelRenderState;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Chams (cursed.recode.chams.Chams): a frame's bodies begin. */
@Mixin(LevelRenderer.class)
public abstract class ChamsLevelMixin {
    @Inject(method = "submitEntities", at = @At("HEAD"))
    private void cursed$chamsFrame(PoseStack poseStack, LevelRenderState state, SubmitNodeCollector out, CallbackInfo ci) {
        ChamsHooks.frameStart();
    }
}
