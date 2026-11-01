package cursed.recode.mixin.client;

import cursed.recode.scene.HudCache;
import net.minecraft.client.DeltaTracker;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.Gui;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** The frame's GUI begins: whether its HUD comes from the HUD's texture (cursed.recode.scene.HudCache). */
@Mixin(Gui.class)
public abstract class GuiMixin {
    @Inject(method = "extractRenderState", at = @At("HEAD"))
    private void cursed$hudCache(DeltaTracker deltaTracker, boolean shouldRenderLevel, boolean resourcesLoaded, CallbackInfo ci) {
        HudCache.begin(Minecraft.getInstance(), shouldRenderLevel, resourcesLoaded);
    }
}
