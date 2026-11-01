package cursed.recode.menus;

import com.mojang.blaze3d.platform.NativeImage;
import cursed.recode.render.Native;
import java.io.InputStream;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Set;

/** Pictures for the menus' pages (native/src/gui/pics.rs): the game's images as RGBA rows, under a key. */
final class Pics {
    /** Keys whose file could not be read (not asked again); keys being read now. */
    private static final Set<String> FAILED = java.util.concurrent.ConcurrentHashMap.newKeySet();
    private static final Set<String> READING = java.util.concurrent.ConcurrentHashMap.newKeySet();

    private Pics() {
    }

    static void put(String key, NativeImage img) {
        int w = img.getWidth(), h = img.getHeight();
        int[] abgr = img.getPixelsABGR();
        byte[] b = new byte[w * h * 4];
        for (int i = 0; i < abgr.length; i++) {
            int p = abgr[i];
            b[i * 4] = (byte) p;
            b[i * 4 + 1] = (byte) (p >> 8);
            b[i * 4 + 2] = (byte) (p >> 16);
            b[i * 4 + 3] = (byte) (p >>> 24);
        }
        Native.menusPic(key, w, h, b);
    }

    /**
     * The picture in `file` under `key`: true once the menus have it. It is read aside (a world's shot is a large
     * picture: read here it held the frame up and every animation jumped after it); false until then, the page shows it
     * when it comes.
     */
    static boolean file(String key, Path file) {
        if (Native.menusPicHas(key)) {
            return true;
        }
        if (FAILED.contains(key) || file == null || !READING.add(key)) {
            return false;
        }
        net.minecraft.util.Util.backgroundExecutor().execute(() -> {
            try (InputStream in = Files.newInputStream(file); NativeImage img = NativeImage.read(in)) {
                put(key, img);
            } catch (Exception e) {
                FAILED.add(key);
            } finally {
                READING.remove(key);
            }
        });
        return false;
    }

    /** A picture read anew (it changed on disk). */
    static void forget(String key) {
        FAILED.remove(key);
    }
}
