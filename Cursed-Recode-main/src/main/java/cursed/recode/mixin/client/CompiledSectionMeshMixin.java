package cursed.recode.mixin.client;

import cursed.recode.render.SectionMirror;
import cursed.recode.render.TerrainMeshHolder;
import cursed.recode.world.FacingHolder;
import net.minecraft.client.renderer.chunk.CompiledSectionMesh;
import net.minecraft.client.renderer.chunk.SectionCompiler;
import net.minecraft.client.renderer.chunk.TranslucencyPointOfView;
import org.jspecify.annotations.Nullable;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Unique;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

@Mixin(CompiledSectionMesh.class)
public abstract class CompiledSectionMeshMixin implements TerrainMeshHolder {
    @Unique
    private volatile int cursed$mesh;
    @Unique
    private int @Nullable [] cursed$facings;
    @Unique
    private volatile @Nullable Object cursed$owner;

    @Inject(method = "<init>(Lnet/minecraft/client/renderer/chunk/TranslucencyPointOfView;Lnet/minecraft/client/renderer/chunk/SectionCompiler$Results;J)V", at = @At("TAIL"))
    private void cursed$facings(TranslucencyPointOfView pointOfView, SectionCompiler.Results results, long startNs, CallbackInfo ci) {
        this.cursed$facings = ((FacingHolder) (Object) results).cursed$facings();
    }

    @Override
    public int @Nullable [] cursed$facings() {
        return this.cursed$facings;
    }

    @Override
    public int cursed$mesh() {
        return this.cursed$mesh;
    }

    @Override
    public void cursed$setMesh(int id) {
        this.cursed$mesh = id;
        // the section drawing this mesh draws it under its new id (a section made the owner after reads the id itself);
        // the section's lock orders this against its mesh changing (RenderSectionMixin.cursed$mirror)
        if (this.cursed$owner instanceof SectionMirror m) {
            synchronized (m) {
                if (m.cursed$plainMesh() == (Object) this) {
                    m.cursed$setDrawId(id);
                }
            }
        }
    }

    @Override
    public @Nullable Object cursed$owner() {
        return this.cursed$owner;
    }

    @Override
    public void cursed$setOwner(Object section) {
        this.cursed$owner = section;
    }
}
