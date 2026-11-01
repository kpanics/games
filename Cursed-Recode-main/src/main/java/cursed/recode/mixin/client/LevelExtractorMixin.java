package cursed.recode.mixin.client;

import com.llamalad7.mixinextras.injector.wrapoperation.Operation;
import com.llamalad7.mixinextras.injector.wrapoperation.WrapOperation;
import com.llamalad7.mixinextras.sugar.Local;
import cursed.recode.render.SectionMirror;
import net.minecraft.client.SectionUpdateTracker;
import net.minecraft.client.renderer.chunk.CompiledSectionMesh;
import net.minecraft.client.renderer.chunk.SectionMesh;
import net.minecraft.client.renderer.chunk.SectionRenderDispatcher;
import net.minecraft.client.renderer.extract.LevelExtractor;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Redirect;

/**
 * The frame's two walks over the visible sections read what is kept on each section (cursed.recode.render.SectionMirror)
 * instead of following it into the game's stores: the dirty state from where it was last found (while it still is the
 * section's, of the same tracker), and the mesh only for sections with block entities.
 */
@Mixin(LevelExtractor.class)
public abstract class LevelExtractorMixin {
    @WrapOperation(method = "extract", at = @At(value = "INVOKE",
            target = "Lnet/minecraft/client/SectionUpdateTracker;getDirtyState(J)Lnet/minecraft/client/SectionUpdateTracker$SectionDirtyState;"))
    private SectionUpdateTracker.SectionDirtyState cursed$knownState(SectionUpdateTracker tracker, long node,
                                                                     Operation<SectionUpdateTracker.SectionDirtyState> original,
                                                                     @Local SectionRenderDispatcher.RenderSection section) {
        SectionMirror mirror = (SectionMirror) (Object) section;
        if (mirror.cursed$state() instanceof SectionUpdateTracker.SectionDirtyState state && mirror.cursed$stateOwner() == tracker
                && state.getSectionNode() == node) {
            return state;
        }
        SectionUpdateTracker.SectionDirtyState state = original.call(tracker, node);
        mirror.cursed$setState(state, tracker);
        return state;
    }

    @Redirect(method = "extractVisibleBlockEntities", at = @At(value = "INVOKE",
            target = "Lnet/minecraft/client/renderer/chunk/SectionRenderDispatcher$RenderSection;getSectionMesh()Lnet/minecraft/client/renderer/chunk/SectionMesh;"))
    private SectionMesh cursed$blockEntityMesh(SectionRenderDispatcher.RenderSection section) {
        SectionMirror mirror = (SectionMirror) (Object) section;
        return mirror.cursed$hasBlockEntities() ? mirror.cursed$plainMesh() : CompiledSectionMesh.EMPTY;
    }
}
