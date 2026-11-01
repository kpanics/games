package cursed.recode.mixin.client;

import cursed.recode.dev.Dev;
import java.util.Collection;
import java.util.function.Consumer;
import net.minecraft.client.renderer.texture.SpriteLoader;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Unique;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Redirect;

/**
 * An atlas's sprites get their mip levels in parallel: the game makes them one sprite after another in a single task
 * per atlas (the blocks atlas alone is over a thousand), while each sprite's levels depend on nothing else.
 * CURSED_DEV_SERIALMIPS keeps the game's order.
 */
@Mixin(SpriteLoader.class)
public abstract class SpriteLoaderMixin {
    @Unique
    private static final boolean OFF = Dev.on("CURSED_DEV_SERIALMIPS");

    @Redirect(method = "lambda$stitch$2", at = @At(value = "INVOKE", target = "Ljava/util/Collection;forEach(Ljava/util/function/Consumer;)V"))
    private static void cursed$parallelMips(Collection<Object> sprites, Consumer<Object> increaseMipLevel) {
        if (OFF) {
            sprites.forEach(increaseMipLevel);
        } else {
            sprites.parallelStream().forEach(increaseMipLevel);
        }
    }
}
