package cursed.recode.mixin.client;

import com.mojang.renderpearl.api.commands.RenderPass;
import com.mojang.renderpearl.api.textures.GpuTextureView;
import cursed.recode.render.CheapOit;
import cursed.recode.render.NativeTerrain;
import com.mojang.renderpearl.api.textures.GpuSampler;
import net.minecraft.client.renderer.chunk.ChunkSectionLayerGroup;
import net.minecraft.client.renderer.texture.TextureAtlas;
import net.minecraft.client.renderer.texture.TextureManager;
import net.minecraft.client.CloudStatus;
import net.minecraft.client.renderer.CloudRenderer;
import net.minecraft.client.renderer.GameRenderer;
import net.minecraft.client.renderer.LevelRenderer;
import net.minecraft.client.renderer.chunk.ChunkSectionsToRender;
import net.minecraft.client.renderer.feature.FeatureRenderDispatcher;
import net.minecraft.client.renderer.oit.OitRenderPassProvider;
import net.minecraft.client.renderer.oit.OitStage;
import net.minecraft.client.renderer.state.OptionsRenderState;
import net.minecraft.client.renderer.state.level.LevelRenderState;
import net.minecraft.util.ARGB;
import org.spongepowered.asm.mixin.Unique;
import org.spongepowered.asm.mixin.Final;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.Redirect;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/**
 * With Improved Transparency (cursed.recode.render.CheapOit) the clouds are drawn as without it, at the end of the
 * opaque pass, depth and all, before the translucent passes: water and glass in front of them still lie over them, as
 * the game's own transparency has it; only from above the clouds is water under one drawn over it. Their three passes
 * through the order-independent transparency cost some 120 µs at 1440p (12.10); drawn so, some 20.
 * And the transparency's first stage, its depth bounds, is not run: nothing reads them (CheapOit), and its three passes
 * with nothing in them still cost the GPU a wait each. The far water goes the classic way here too, before the clouds
 * (vk/terrain.rs far_water: two transparency passes over the far ocean cost some 20 µs at 1440p, drawn sorted some 5).
 */
@Mixin(LevelRenderer.class)
public abstract class LevelRendererCloudsMixin {
    @Shadow
    @Final
    private GameRenderer gameRenderer;
    @Shadow
    @Final
    private LevelRenderState levelRenderState;
    @Shadow
    @Final
    private OptionsRenderState optionsRenderState;
    @Shadow
    @Final
    private CloudRenderer cloudRenderer;
    @Shadow
    @Final
    private TextureManager textureManager;
    @Shadow
    private GpuSampler chunkLayerSampler;

    @Inject(method = "executeSolid", at = @At("TAIL"))
    private void cursed$cloudsBeforeTransparency(ChunkSectionsToRender sections, FeatureRenderDispatcher.PreparedFrame featureFrame, RenderPass renderPass,
                                                 CallbackInfo ci) {
        if (CheapOit.ON && CheapOit.FAR_WATER && this.gameRenderer.useImprovedTransparency() && NativeTerrain.active()) {
            NativeTerrain.farTranslucent = true;
            try {
                sections.renderGroup(ChunkSectionLayerGroup.TRANSLUCENT, renderPass, this.chunkLayerSampler,
                        this.textureManager.getTexture(TextureAtlas.LOCATION_BLOCKS).getTextureView(), false);
            } finally {
                NativeTerrain.farTranslucent = false;
            }
        }
        if (CheapOit.ON && this.gameRenderer.useImprovedTransparency() && this.optionsRenderState.cloudStatus != CloudStatus.OFF
                && ARGB.alpha(this.levelRenderState.cloudColor) > 0) {
            this.cloudRenderer.render(this.optionsRenderState.cloudStatus, renderPass);
        }
    }

    @Unique
    private static final OitStage[] CURSED$STAGES = {OitStage.TRANSMITTANCE, OitStage.ACCUMULATE};

    @Redirect(method = "executeOit", at = @At(value = "INVOKE", target = "Lnet/minecraft/client/renderer/oit/OitStage;values()[Lnet/minecraft/client/renderer/oit/OitStage;"))
    private OitStage[] cursed$stages() {
        return CheapOit.ON ? CURSED$STAGES.clone() : OitStage.values();
    }

    @Redirect(method = "executeOit", at = @At(value = "INVOKE",
            target = "Lnet/minecraft/client/renderer/CloudRenderer;renderOit(Lnet/minecraft/client/CloudStatus;Lnet/minecraft/client/renderer/oit/OitStage;Lcom/mojang/renderpearl/api/textures/GpuTextureView;Lnet/minecraft/client/renderer/oit/OitRenderPassProvider$Parameters;)V"))
    private void cursed$noOitClouds(CloudRenderer clouds, CloudStatus status, OitStage stage, GpuTextureView depth, OitRenderPassProvider.Parameters params) {
        if (!CheapOit.ON) {
            clouds.renderOit(status, stage, depth, params);
        }
    }
}
