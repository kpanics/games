package cursed.recode.mixin.client;

import com.mojang.renderpearl.api.device.GpuBackend;
import cursed.recode.dev.Dev;
import cursed.recode.render.CursedBackend;
import net.minecraft.client.PreferredGraphicsApi;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** Whatever the options say, the game tries one backend: ours (Vulkan through cursed_recode.dll). */
@Mixin(PreferredGraphicsApi.class)
public abstract class PreferredGraphicsApiMixin {
    @Inject(method = "getBackendsToTry", at = @At("HEAD"), cancellable = true)
    private void cursed$onlyOurs(CallbackInfoReturnable<GpuBackend[]> cir) {
        if (Dev.mojang()) {
            return;
        }
        cir.setReturnValue(new GpuBackend[] {new CursedBackend()});
    }
}
