package cursed.recode.mixin.client;

import cursed.recode.render.NativeTerrain;
import net.minecraft.client.renderer.chunk.SectionRenderDispatcher;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

@Mixin(SectionRenderDispatcher.class)
public abstract class SectionRenderDispatcherMixin {
    @Inject(method = "dispose", at = @At("HEAD"))
    private void cursed$dropTerrain(CallbackInfo ci) {
        NativeTerrain.reset();
    }
}
