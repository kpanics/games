package cursed.recode.mixin.client;

import com.mojang.blaze3d.platform.IconSet;
import com.mojang.blaze3d.platform.Monitor;
import com.mojang.blaze3d.platform.MonitorManager;
import cursed.recode.launch.AppIcon;
import cursed.recode.launch.LaunchScreen;
import net.minecraft.server.packs.PackMetadataResources;
import org.spongepowered.asm.mixin.Final;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.ModifyVariable;
import org.spongepowered.asm.mixin.injection.Redirect;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;
import cursed.recode.launch.Chrome;
import com.mojang.blaze3d.platform.Window;
import org.lwjgl.sdl.SDLVideo;

@Mixin(com.mojang.blaze3d.platform.Window.class)
public abstract class WindowMixin {
    @Shadow
    @Final
    private long handle;

    @Shadow
    private int width;

    @Shadow
    private int height;

    // ---- our own title bar (cursed.recode.launch.Chrome): the window the game knows is the picture under it ----

    @ModifyVariable(method = "onResize", at = @At("HEAD"), argsOnly = true, ordinal = 1)
    private int cursed$resizeUnderBar(int newHeight) {
        return Math.max(1, newHeight - Chrome.inset());
    }

    @ModifyVariable(method = "onFramebufferResize", at = @At("HEAD"), argsOnly = true, ordinal = 1)
    private int cursed$framebufferUnderBar(int newHeight) {
        return Math.max(1, newHeight - Chrome.inset());
    }

    @Inject(method = "queryFramebufferSize", at = @At("RETURN"), cancellable = true)
    private void cursed$queryUnderBar(CallbackInfoReturnable<Window.FramebufferSize> cir) {
        int inset = Chrome.inset();
        if (inset > 0) {
            Window.FramebufferSize s = cir.getReturnValue();
            cir.setReturnValue(new Window.FramebufferSize(s.width(), Math.max(1, s.height() - inset)));
        }
    }

    /** Back from fullscreen: the window put whole (the bar and the picture under it) by the bar's code, not SDL's. */
    @Redirect(method = "setWindowSizeAndPosition", at = @At(value = "INVOKE", target = "Lorg/lwjgl/sdl/SDLVideo;SDL_SetWindowSize(JII)Z"))
    private boolean cursed$sizeWithBar(long window, int w, int h) {
        return Chrome.inset() > 0 || SDLVideo.SDL_SetWindowSize(window, w, h);
    }

    @Redirect(method = "setWindowSizeAndPosition", at = @At(value = "INVOKE", target = "Lorg/lwjgl/sdl/SDLVideo;SDL_SetWindowPosition(JII)Z"))
    private boolean cursed$placeWithBar(long window, int x, int y) {
        int inset = Chrome.inset();
        if (inset <= 0) {
            return SDLVideo.SDL_SetWindowPosition(window, x, y);
        }
        long hwnd = org.lwjgl.sdl.SDLProperties.SDL_GetPointerProperty(SDLVideo.SDL_GetWindowProperties(window), SDLVideo.SDL_PROP_WINDOW_WIN32_HWND_POINTER, 0L);
        // (SDL's position is its client area's: the bar's corner; the picture is under the bar)
        cursed.recode.render.Native.chromePlace(hwnd, x, y + inset, this.width, this.height);
        return true;
    }

    /**
     * Development only: with CURSED_DEV_WINDOW_AT the backend already put the window where the test wants it, and the
     * game must keep that place instead of centring the window on the main monitor.
     */
    @Redirect(
            method = "<init>(Lcom/mojang/blaze3d/platform/WindowEventHandler;Lcom/mojang/blaze3d/platform/DisplayData;Ljava/lang/String;ZLjava/lang/String;Lcom/mojang/blaze3d/platform/MonitorManager;Lcom/mojang/renderpearl/api/device/GpuBackend;I)V",
            at = @At(value = "INVOKE", target = "Lcom/mojang/blaze3d/platform/MonitorManager;getMonitor(I)Lcom/mojang/blaze3d/platform/Monitor;"))
    private Monitor cursed$devPlace(MonitorManager manager, int id) {
        return System.getenv("CURSED_DEV_WINDOW_AT") != null || cursed.recode.launch.Twin.active() ? null : manager.getMonitor(id);
    }

    /** The window before the game chose the window's size (cursed.recode.launch.Twin): the game keeps it. */
    @Redirect(
            method = "<init>(Lcom/mojang/blaze3d/platform/WindowEventHandler;Lcom/mojang/blaze3d/platform/DisplayData;Ljava/lang/String;ZLjava/lang/String;Lcom/mojang/blaze3d/platform/MonitorManager;Lcom/mojang/renderpearl/api/device/GpuBackend;I)V",
            at = @At(value = "INVOKE", target = "Lcom/mojang/blaze3d/platform/Window;allowedWindowMinSize(II)I"))
    private int cursed$twinSize(int size, int min) {
        int[] p = cursed.recode.launch.Twin.active() ? cursed.recode.launch.Twin.place() : null;
        return Math.max(p == null ? size : min == 320 ? p[2] : p[3], min);
    }

    /** Her icon, not Minecraft's. */
    @Inject(method = "setIcon(Lnet/minecraft/server/packs/PackMetadataResources;Lcom/mojang/blaze3d/platform/IconSet;)V", at = @At("HEAD"), cancellable = true)
    private void cursed$icon(PackMetadataResources resources, IconSet iconSet, CallbackInfo ci) {
        if (AppIcon.set(this.handle)) {
            ci.cancel();
        }
    }

    /** The window's close button (and Alt+F4) in the main menu: the menu's own way out, as Quit held. */
    @Inject(method = "onQuitRequested", at = @At("HEAD"), cancellable = true)
    private void cursed$wayOut(CallbackInfo ci) {
        if (LaunchScreen.quitRequested()) {
            ci.cancel();
        }
    }
}
