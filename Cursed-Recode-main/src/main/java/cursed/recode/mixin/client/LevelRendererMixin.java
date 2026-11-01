package cursed.recode.mixin.client;

import com.mojang.blaze3d.systems.RenderSystem;
import com.mojang.renderpearl.api.buffers.GpuBufferSlice;
import com.mojang.renderpearl.api.textures.GpuTextureView;
import cursed.recode.render.NativeTerrain;
import it.unimi.dsi.fastutil.objects.ObjectArrayList;
import java.util.EnumMap;
import net.minecraft.client.renderer.LevelRenderer;
import net.minecraft.client.renderer.chunk.ChunkSectionLayer;
import net.minecraft.client.renderer.chunk.ChunkSectionsToRender;
import net.minecraft.client.renderer.chunk.SectionRenderDispatcher;
import net.minecraft.client.renderer.state.OptionsRenderState;
import net.minecraft.client.renderer.texture.TextureAtlas;
import net.minecraft.client.renderer.texture.TextureManager;
import net.minecraft.util.Util;
import org.joml.Matrix4fc;
import org.spongepowered.asm.mixin.Final;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.Unique;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/**
 * The frame's terrain is prepared by the native renderer: the game's per-frame walk over every visible section's
 * buffers (extractSectionDrawGroups, a fifth of the render thread) is replaced by one native call.
 */
@Mixin(LevelRenderer.class)
public abstract class LevelRendererMixin {
    @Shadow @Final private ObjectArrayList<SectionRenderDispatcher.RenderSection> visibleSections;
    @Shadow @Final private TextureManager textureManager;
    @Shadow @Final private OptionsRenderState optionsRenderState;

    @Inject(method = "prepareChunkRendersIndirect", at = @At("HEAD"), cancellable = true)
    private void cursed$nativeIndirect(Matrix4fc modelView, boolean respectTranslucentOrder, CallbackInfoReturnable<ChunkSectionsToRender> cir) {
        if (NativeTerrain.active()) {
            cir.setReturnValue(this.cursed$prepare(modelView, respectTranslucentOrder));
        }
    }

    @Inject(method = "prepareChunkRenders", at = @At("HEAD"), cancellable = true)
    private void cursed$nativeSeparate(Matrix4fc modelView, boolean respectTranslucentOrder, CallbackInfoReturnable<ChunkSectionsToRender> cir) {
        if (NativeTerrain.active()) {
            cir.setReturnValue(this.cursed$prepare(modelView, respectTranslucentOrder));
        }
    }

    @Unique
    private ChunkSectionsToRender cursed$prepare(Matrix4fc modelView, boolean respectTranslucentOrder) {
        NativeTerrain.build(this.visibleSections, Util.getMillis(), cursed.recode.fx.ChunkAnimator.length(Util.toMillis(this.optionsRenderState.chunkSectionFadeInTime)), respectTranslucentOrder, modelView);
        GpuTextureView atlas = this.textureManager.getTexture(TextureAtlas.LOCATION_BLOCKS).getTextureView();
        GpuBufferSlice transform = RenderSystem.getDynamicUniforms().writeTerrainTransform(modelView, atlas.getWidth(0), atlas.getHeight(0));
        return new ChunkSectionsToRender.DrawIndirect(transform, new EnumMap<>(ChunkSectionLayer.class), 0, transform);
    }
}
