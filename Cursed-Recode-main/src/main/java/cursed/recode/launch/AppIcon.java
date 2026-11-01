package cursed.recode.launch;

import com.mojang.blaze3d.platform.NativeImage;
import java.io.InputStream;
import java.util.ArrayList;
import java.util.List;
import org.lwjgl.sdl.SDLSurface;
import org.lwjgl.sdl.SDLVideo;
import org.lwjgl.sdl.SDL_Surface;

/**
 * Her icon on the game's window (the title bar, the taskbar, Alt+Tab) in place of Minecraft's (WindowMixin): her logo
 * with the rounded corners of Cursed's icon, at every size Windows asks for (assets/cursed-recode/icon).
 */
public final class AppIcon {
    private static final int[] SIZES = {256, 128, 64, 48, 32, 24, 16};

    private AppIcon() {
    }

    /** Sets it on the SDL window `handle`; false if none could be read. */
    public static boolean set(long handle) {
        List<NativeImage> images = new ArrayList<>();
        SDL_Surface main = null;
        try {
            for (int size : SIZES) {
                try (InputStream in = AppIcon.class.getResourceAsStream("/assets/cursed-recode/icon/recode_" + size + ".png")) {
                    if (in == null) {
                        continue;
                    }
                    NativeImage image = NativeImage.read(in);
                    images.add(image);
                    // the game's own way (Window.createIconSurface): RGBA as it lies in memory
                    SDL_Surface s = SDLSurface.SDL_CreateSurfaceFrom(image.getWidth(), image.getHeight(), 376840196, image.getPixelBytes(), image.getWidth() * 4);
                    if (s == null) {
                        continue;
                    }
                    if (main == null) {
                        main = s;
                    } else {
                        SDLSurface.SDL_AddSurfaceAlternateImage(main, s);
                        SDLSurface.SDL_DestroySurface(s);
                    }
                }
            }
            return main != null && SDLVideo.SDL_SetWindowIcon(handle, main);
        } catch (Exception e) {
            return false;
        } finally {
            if (main != null) {
                SDLSurface.SDL_DestroySurface(main);
            }
            images.forEach(NativeImage::close);
        }
    }
}
