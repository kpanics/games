package cursed.recode.mixin.client;

import com.mojang.logging.LogUtils;
import cursed.recode.dev.Dev;
import cursed.recode.dev.FeatureTimes;
import cursed.recode.scene.WeatherBiomes;
import net.minecraft.client.multiplayer.ClientLevel;
import net.minecraft.client.renderer.WeatherEffectRenderer;
import net.minecraft.client.renderer.state.level.WeatherRenderState;
import net.minecraft.core.BlockPos;
import net.minecraft.world.level.biome.Biome;
import net.minecraft.world.phys.Vec3;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Unique;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.Redirect;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/**
 * The weather's columns ask cursed.recode.scene.WeatherBiomes for their precipitation (the same answer, without
 * BiomeManager's zoom every frame). CURSED_DEV_NOWEATHERCACHE asks the game; CURSED_DEV_WEATHERCHECK asks both.
 */
@Mixin(WeatherEffectRenderer.class)
public abstract class WeatherEffectRendererMixin {
    @Unique
    private static final boolean OFF = Dev.on("CURSED_DEV_NOWEATHERCACHE");
    @Unique
    private static final boolean CHECK = Dev.on("CURSED_DEV_WEATHERCHECK");
    @Unique
    private static long checked;
    @Unique
    private static long differ;

    /** No Render (cursed.recode.fx.NoRender): no rain or snow falling. */
    @org.spongepowered.asm.mixin.injection.Inject(method = "extractRenderState", at = @At("HEAD"), cancellable = true)
    private void cursed$noWeather(ClientLevel level, float partialTicks, net.minecraft.world.phys.Vec3 cameraPos, net.minecraft.client.renderer.state.level.WeatherRenderState renderState,
                                  org.spongepowered.asm.mixin.injection.callback.CallbackInfo ci) {
        if (cursed.recode.fx.NoRender.on("Rain and snow")) {
            renderState.reset();
            renderState.intensity = 0.0F;
            ci.cancel();
        }
    }

    @Redirect(method = "extractRenderState", at = @At(value = "INVOKE",
            target = "Lnet/minecraft/client/multiplayer/ClientLevel;getPrecipitationAt(Lnet/minecraft/core/BlockPos;)Lnet/minecraft/world/level/biome/Biome$Precipitation;"))
    private Biome.Precipitation cursed$precipitation(ClientLevel level, BlockPos pos) {
        if (OFF) {
            return level.getPrecipitationAt(pos);
        }
        Biome.Precipitation ours = WeatherBiomes.precipitation(level, pos);
        if (CHECK) {
            Biome.Precipitation game = level.getPrecipitationAt(pos);
            checked++;
            if (game != ours && differ++ < 20) {
                LogUtils.getLogger().warn("[weather] {} at {} where the game says {}", ours, pos, game);
            }
            if (checked % 100000 == 0) {
                LogUtils.getLogger().info("[weather] checked {} columns, {} differ", checked, differ);
            }
        }
        return ours;
    }

    @Inject(method = "extractRenderState", at = @At("HEAD"))
    private void cursed$start(ClientLevel level, float partialTicks, Vec3 cameraPos, WeatherRenderState renderState, CallbackInfo ci) {
        FeatureTimes.start("weather");
        WeatherBiomes.begin(level.getBiomeManager(), net.minecraft.util.Mth.floor(cameraPos.x), net.minecraft.util.Mth.floor(cameraPos.y),
                net.minecraft.util.Mth.floor(cameraPos.z), net.minecraft.client.Minecraft.getInstance().options.weatherRadius().get());
    }

    @Inject(method = "extractRenderState", at = @At("TAIL"))
    private void cursed$stop(ClientLevel level, float partialTicks, Vec3 cameraPos, WeatherRenderState renderState, CallbackInfo ci) {
        FeatureTimes.stop("weather");
    }
}
