package cursed.recode.mixin.client;

import com.mojang.blaze3d.systems.RenderSystem;
import com.mojang.renderpearl.api.buffers.GpuBuffer;
import com.mojang.renderpearl.api.commands.RenderPass;
import com.mojang.renderpearl.api.pipeline.IndexType;
import com.mojang.renderpearl.api.pipeline.RenderPipeline;
import cursed.recode.render.NativeTerrain;
import net.minecraft.client.renderer.chunk.ChunkSectionLayer;
import net.minecraft.client.renderer.chunk.ChunkSectionsToRender;
import org.jspecify.annotations.Nullable;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** A terrain layer is the game's pipeline and uniforms, then one native call that draws the frame's terrain of it. */
@Mixin(ChunkSectionsToRender.DrawIndirect.class)
public abstract class DrawIndirectMixin {
    @Inject(method = "render", at = @At("HEAD"), cancellable = true)
    private void cursed$nativeLayer(ChunkSectionLayer layer, RenderPass renderPass, @Nullable GpuBuffer defaultIndexBuffer, @Nullable IndexType defaultIndexType,
                                    @Nullable RenderPipeline override, @Nullable RenderPipeline overrideMultidraw, CallbackInfo ci) {
        if (!NativeTerrain.active()) {
            return;
        }
        renderPass.setPipeline(RenderSystem.getCompiledPipeline(overrideMultidraw != null ? overrideMultidraw : layer.pipeline(true)));
        NativeTerrain.drawLayer(layer.ordinal());
        ci.cancel();
    }
}
