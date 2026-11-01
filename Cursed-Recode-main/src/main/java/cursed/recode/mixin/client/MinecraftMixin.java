package cursed.recode.mixin.client;

import com.mojang.renderpearl.api.device.BackendCreationException;
import com.mojang.renderpearl.backend.vulkan.VulkanBackend;
import cursed.recode.dev.Dev;
import cursed.recode.dev.DevCommands;
import cursed.recode.dev.FrameStats;
import net.minecraft.client.Minecraft;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.Redirect;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

@Mixin(Minecraft.class)
public abstract class MinecraftMixin {
    /** The game would make a Vulkan instance of Mojang's just to see if Vulkan works; ours answers that itself. */
    @Redirect(
            method = "<init>",
            at = @At(value = "INVOKE", target = "Lcom/mojang/renderpearl/backend/vulkan/VulkanBackend;checkBackendAvailable()Lcom/mojang/renderpearl/api/device/BackendCreationException;"))
    private BackendCreationException cursed$noMojangVulkanCheck() {
        return Dev.mojang() ? VulkanBackend.checkBackendAvailable() : null;
    }

    @Inject(method = "renderFrame", at = @At("HEAD"))
    private void cursed$frame(boolean advanceGameTime, CallbackInfo ci) {
        FrameStats.frame();
        cursed.recode.dev.DevInput.frame((Minecraft) (Object) this);
    }

    @Inject(method = "tick", at = @At("TAIL"))
    private void cursed$tick(CallbackInfo ci) {
        DevCommands.tick((Minecraft) (Object) this);
        cursed.recode.via.Via.tick((Minecraft) (Object) this);
        cursed.recode.launch.Chrome.tick((Minecraft) (Object) this);
        cursed.recode.hud.Warmup.tick((Minecraft) (Object) this);
        cursed.recode.swap.AutoSwap.tick((Minecraft) (Object) this);
        cursed.recode.cosmetics.Cosmetics.tick((Minecraft) (Object) this);
        cursed.recode.swap.ClickPearl.tick((Minecraft) (Object) this);
        cursed.recode.alts.Alts.tick((Minecraft) (Object) this);
        cursed.recode.fx.ItemPhysic.tick((Minecraft) (Object) this);
        cursed.recode.play.Play.tick((Minecraft) (Object) this);
        cursed.recode.play.StreamerMode.tick((Minecraft) (Object) this);
        cursed.recode.fx.FakePlayers.tick((Minecraft) (Object) this);
        cursed.recode.fx.ShulkerPreview.tick((Minecraft) (Object) this);
    }

    /** Out of a world the menu's own music plays (her track, launch/sound.rs): none of the game's menu music. */
    @Inject(method = "getSituationalMusic", at = @At("HEAD"), cancellable = true)
    private void cursed$noMenuMusic(CallbackInfoReturnable<net.minecraft.sounds.Music> cir) {
        if (((Minecraft) (Object) this).player == null) {
            cir.setReturnValue(null);
        }
    }

    @org.spongepowered.asm.mixin.Unique
    private static boolean cursed$inWorld;

    /** Her track goes softly when a world is entered (the game's music is there) and comes back in the menu. */
    @Inject(method = "tick", at = @At("TAIL"))
    private void cursed$menuMusic(CallbackInfo ci) {
        boolean in = ((Minecraft) (Object) this).level != null;
        if (in != cursed$inWorld) {
            cursed$inWorld = in;
            cursed.recode.render.Native.launchMusic(in ? 2 : 3);
        }
    }

    /** The window is called Cursed Recode, wherever the game is (not «Minecraft* 26.3 - Singleplayer»). */
    @Inject(method = "createTitle", at = @At("HEAD"), cancellable = true)
    private void cursed$title(CallbackInfoReturnable<String> cir) {
        cir.setReturnValue("Cursed Recode");
    }
}
