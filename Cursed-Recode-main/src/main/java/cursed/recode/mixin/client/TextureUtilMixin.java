package cursed.recode.mixin.client;

import com.mojang.blaze3d.platform.NativeImage;
import com.mojang.blaze3d.platform.TextureUtil;
import com.mojang.logging.LogUtils;
import cursed.recode.dev.Dev;
import cursed.recode.render.Native;
import cursed.recode.render.NativeTerrain;
import java.util.concurrent.atomic.AtomicLong;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Unique;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/**
 * TextureUtil.solidify in the DLL (native/src/scene/textures.rs): the same breadth-first fill, the same pixels, without
 * Java's per-pixel bounds checks — it was a fifth of the resource reload's work. CURSED_DEV_TEXCHECK runs the game's on
 * a copy and compares every image; CURSED_DEV_NOTEXNATIVE keeps the game's.
 */
@Mixin(TextureUtil.class)
public abstract class TextureUtilMixin {
    @Unique
    private static final boolean OFF = Dev.on("CURSED_DEV_NOTEXNATIVE");
    @Unique
    private static final boolean CHECK = Dev.on("CURSED_DEV_TEXCHECK");
    @Unique
    private static final AtomicLong checked = new AtomicLong();
    @Unique
    private static final AtomicLong differ = new AtomicLong();
    @Unique
    private static final ThreadLocal<Boolean> GAME = ThreadLocal.withInitial(() -> false);

    @Inject(method = "solidify", at = @At("HEAD"), cancellable = true)
    private static void cursed$solidify(NativeImage image, CallbackInfo ci) {
        if (OFF || GAME.get() || image.format() != NativeImage.Format.RGBA || !NativeTerrain.active()) {
            return;
        }
        NativeImage game = null;
        if (CHECK) {
            game = new NativeImage(image.getWidth(), image.getHeight(), false);
            game.copyFrom(image);
            GAME.set(true);
            try {
                TextureUtil.solidify(game);
            } finally {
                GAME.set(false);
            }
        }
        Native.textureSolidify(image.getPointer(), image.getWidth(), image.getHeight());
        ci.cancel();
        if (game != null) {
            boolean same = true;
            for (int y = 0; y < image.getHeight() && same; y++) {
                for (int x = 0; x < image.getWidth(); x++) {
                    if (image.getPixel(x, y) != game.getPixel(x, y)) {
                        same = false;
                        break;
                    }
                }
            }
            game.close();
            long n = checked.incrementAndGet();
            if (!same && differ.incrementAndGet() < 20) {
                LogUtils.getLogger().warn("[textures] solidify differs from the game on a {}x{} image", image.getWidth(), image.getHeight());
            }
            if (n % 500 == 0) {
                LogUtils.getLogger().info("[textures] solidify checked {} images, {} differ", n, differ.get());
            }
        }
    }
}
