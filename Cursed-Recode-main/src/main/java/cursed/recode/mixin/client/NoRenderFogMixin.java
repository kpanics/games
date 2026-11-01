package cursed.recode.mixin.client;

import cursed.recode.fx.NoRender;
import net.minecraft.client.renderer.fog.environment.BlindnessFogEnvironment;
import net.minecraft.client.renderer.fog.environment.DarknessFogEnvironment;
import net.minecraft.client.renderer.fog.environment.MobEffectFogEnvironment;
import net.minecraft.world.level.material.FogType;
import net.minecraft.world.entity.Entity;
import org.jspecify.annotations.Nullable;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** No Render (cursed.recode.fx.NoRender): the blindness's fog, the darkness's (never on FunTime: the menu says so). */
@Mixin(MobEffectFogEnvironment.class)
public abstract class NoRenderFogMixin {
    @Inject(method = "isApplicable", at = @At("HEAD"), cancellable = true)
    private void cursed$effect(@Nullable FogType fogType, Entity entity, CallbackInfoReturnable<Boolean> cir) {
        Object self = this;
        if (self instanceof BlindnessFogEnvironment && NoRender.on("Blindness fog") || self instanceof DarknessFogEnvironment && NoRender.on("Darkness pulse")) {
            cir.setReturnValue(false);
        }
    }
}
