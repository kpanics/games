package cursed.recode.mixin.client;

import com.mojang.blaze3d.vertex.VertexSorting;
import cursed.recode.world.NativeMesher;
import net.minecraft.client.color.block.BlockColors;
import net.minecraft.client.renderer.SectionBufferBuilderPack;
import net.minecraft.client.renderer.block.BlockStateModelSet;
import net.minecraft.client.renderer.block.FluidStateModelSet;
import net.minecraft.client.renderer.chunk.RenderSectionRegion;
import net.minecraft.client.renderer.chunk.SectionCompiler;
import net.minecraft.core.SectionPos;
import org.spongepowered.asm.mixin.Final;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** Sections are meshed in the DLL (cursed.recode.world.NativeMesher); the game's compile stays for what it can't do. */
@Mixin(SectionCompiler.class)
public abstract class SectionCompilerMixin {
    @Shadow
    @Final
    private boolean ambientOcclusion;
    @Shadow
    @Final
    private boolean cutoutLeaves;
    @Shadow
    @Final
    private BlockStateModelSet blockModelSet;
    @Shadow
    @Final
    private FluidStateModelSet fluidModelSet;
    @Shadow
    @Final
    private BlockColors blockColors;

    @Inject(method = "<init>", at = @At("TAIL"))
    private void cursed$tables(boolean ambientOcclusion, boolean cutoutLeaves, BlockStateModelSet blockModelSet, FluidStateModelSet fluidModelSet,
                               BlockColors blockColors, CallbackInfo ci) {
        NativeMesher.prepare(blockModelSet, blockColors, fluidModelSet);
    }

    @Inject(method = "compile", at = @At("HEAD"), cancellable = true)
    private void cursed$compile(SectionPos sectionPos, RenderSectionRegion region, VertexSorting vertexSorting, SectionBufferBuilderPack builders,
                                CallbackInfoReturnable<SectionCompiler.Results> cir) {
        SectionCompiler.Results results = NativeMesher.compile((SectionCompiler) (Object) this, this.ambientOcclusion, this.cutoutLeaves,
                this.blockModelSet, this.fluidModelSet, this.blockColors, sectionPos, region, vertexSorting, builders);
        if (results != null) {
            cir.setReturnValue(results);
        }
    }
}
