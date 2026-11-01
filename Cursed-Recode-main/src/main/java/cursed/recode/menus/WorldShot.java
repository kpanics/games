package cursed.recode.menus;

import com.mojang.blaze3d.pipeline.RenderTarget;
import com.mojang.blaze3d.platform.NativeImage;
import cursed.recode.CursedRecode;
import java.nio.file.Path;
import net.minecraft.client.Minecraft;
import net.minecraft.client.Screenshot;
import net.minecraft.util.Util;
import net.minecraft.world.level.storage.LevelResource;
import org.jspecify.annotations.Nullable;

/**
 * Where she left off: when the game is paused in a singleplayer world, the world as it is that frame (before the
 * GUI is drawn over it) goes into the world's folder as its picture for Singleplayer (cursed.recode.menus.WorldsPage).
 */
public final class WorldShot {
    public static final String FILE = "cursed_shot.png";
    /** The width it is kept at (16:9 at most). */
    private static final int WIDTH = 960;
    private static @Nullable Path pending;
    /** When it was asked (ns): taken a moment later, once the picture has settled (a turn of the camera as Esc was
     *  pressed left it smeared). */
    private static long askedAt;

    private WorldShot() {
    }

    /** The pause menu opened: this frame's world is taken (if it is a singleplayer world's). */
    static void ask() {
        Minecraft mc = Minecraft.getInstance();
        if (mc.getSingleplayerServer() != null && mc.level != null) {
            pending = mc.getSingleplayerServer().getWorldPath(LevelResource.ROOT).resolve(FILE);
            askedAt = System.nanoTime();
        }
    }

    /** The world is drawn into `target` and the GUI not yet (GameRendererMixin): taken now, if asked. */
    public static void take(RenderTarget target) {
        Path to = pending;
        if (to == null || System.nanoTime() - askedAt < 300_000_000L) {
            return;
        }
        Minecraft mc = Minecraft.getInstance();
        if (mc.getSingleplayerServer() == null || !to.startsWith(mc.getSingleplayerServer().getWorldPath(LevelResource.ROOT))) {
            // that world was left before its picture was taken
            pending = null;
            return;
        }
        pending = null;
        Screenshot.takeScreenshot(target, image -> Util.ioPool().execute(() -> {
            try (NativeImage src = image; NativeImage out = scaled(src)) {
                out.writeToFile(to);
            } catch (Exception e) {
                CursedRecode.LOGGER.warn("[menus] the world's picture", e);
            }
        }));
    }

    /** The picture at WIDTH wide, as 16:9 as it was (cropped to 16:9 when taller). */
    private static NativeImage scaled(NativeImage src) {
        int sw = src.getWidth(), sh = src.getHeight();
        int ch = Math.min(sh, sw * 9 / 16);
        int y0 = (sh - ch) / 2;
        int w = Math.min(WIDTH, sw), h = Math.max(1, ch * w / sw);
        NativeImage out = new NativeImage(w, h, false);
        src.resizeSubRectTo(0, y0, sw, ch, out);
        return out;
    }
}
