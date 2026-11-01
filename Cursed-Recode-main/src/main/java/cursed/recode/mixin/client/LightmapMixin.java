package cursed.recode.mixin.client;

import net.minecraft.client.renderer.Lightmap;
import net.minecraft.client.renderer.state.LightmapRenderState;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** The game's light map, as it is about to be made: Full Bright makes it over (cursed.recode.fx.FullBright). */
@Mixin(Lightmap.class)
public abstract class LightmapMixin {
    @Inject(method = "render", at = @At("HEAD"))
    private void cursed$fullBright(LightmapRenderState state, CallbackInfo ci) {
        cursed.recode.fx.FullBright.apply(state);
        // No Render: the darkness's pulse of the light (never on FunTime: the menu says so)
        if (cursed.recode.fx.NoRender.on("Darkness pulse")) {
            state.darknessEffectScale = 0.0F;
        }
    }
}
