package cursed.recode.mixin.client;

import cursed.recode.fx.NoRender;
import net.minecraft.client.Camera;
import net.minecraft.client.DeltaTracker;
import net.minecraft.client.multiplayer.ClientLevel;
import net.minecraft.client.renderer.fog.FogData;
import net.minecraft.client.renderer.fog.environment.LavaFogEnvironment;
import net.minecraft.client.renderer.fog.environment.WaterFogEnvironment;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** No Render (cursed.recode.fx.NoRender): under water and in lava as far as the eye goes (FunTime allows it: LavaClearWater). */
@Mixin({WaterFogEnvironment.class, LavaFogEnvironment.class})
public abstract class NoRenderLiquidFogMixin {
    @Inject(method = "setupFog", at = @At("TAIL"))
    private void cursed$clear(FogData fog, Camera camera, ClientLevel level, float renderDistance, DeltaTracker deltaTracker, CallbackInfo ci) {
        Object self = this;
        boolean water = self instanceof WaterFogEnvironment;
        if (water ? NoRender.on("Clear water") : NoRender.on("Clear lava")) {
            fog.environmentalStart = renderDistance * (water ? 0.6F : 0.3F);
            fog.environmentalEnd = renderDistance;
            fog.skyEnd = renderDistance;
            fog.cloudEnd = renderDistance;
        }
    }
}
