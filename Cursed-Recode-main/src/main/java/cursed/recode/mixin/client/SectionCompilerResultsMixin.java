package cursed.recode.mixin.client;

import cursed.recode.world.FacingHolder;
import net.minecraft.client.renderer.chunk.SectionCompiler;
import org.jspecify.annotations.Nullable;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Unique;

/** The native mesher's quads per facing, on the compile's results. */
@Mixin(SectionCompiler.Results.class)
public abstract class SectionCompilerResultsMixin implements FacingHolder {
    @Unique
    private int @Nullable [] cursed$facings;

    @Override
    public int @Nullable [] cursed$facings() {
        return this.cursed$facings;
    }

    @Override
    public void cursed$setFacings(int @Nullable [] facings) {
        this.cursed$facings = facings;
    }
}
