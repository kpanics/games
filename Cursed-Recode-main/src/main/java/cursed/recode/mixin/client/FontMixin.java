package cursed.recode.mixin.client;

import cursed.recode.scene.TextCache;
import net.minecraft.client.gui.Font;
import net.minecraft.util.FormattedCharSequence;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** A sequence's glyphs prepared once for the same place and colours (cursed.recode.scene.TextCache). */
@Mixin(Font.class)
public abstract class FontMixin {
    @Inject(method = "prepareText(Lnet/minecraft/util/FormattedCharSequence;FFIZZI)Lnet/minecraft/client/gui/Font$PreparedText;", at = @At("HEAD"), cancellable = true)
    private void cursed$kept(FormattedCharSequence text, float x, float y, int color, boolean shadow, boolean includeEmpty, int background,
                             CallbackInfoReturnable<Font.PreparedText> cir) {
        if (TextCache.on()) {
            Font.PreparedText kept = TextCache.prepared((Font) (Object) this, text, x, y, color, shadow, includeEmpty, background);
            if (kept != null) {
                cir.setReturnValue(kept);
                return;
            }
        }
        TextCache.sawObfuscated = false;
    }

    @Inject(method = "prepareText(Lnet/minecraft/util/FormattedCharSequence;FFIZZI)Lnet/minecraft/client/gui/Font$PreparedText;", at = @At("RETURN"))
    private void cursed$keep(FormattedCharSequence text, float x, float y, int color, boolean shadow, boolean includeEmpty, int background,
                             CallbackInfoReturnable<Font.PreparedText> cir) {
        if (TextCache.on() && !TextCache.sawObfuscated) {
            Font.PreparedText made = cir.getReturnValue();
            if (TextCache.prepared((Font) (Object) this, text, x, y, color, shadow, includeEmpty, background) != made) {
                TextCache.remember((Font) (Object) this, text, x, y, color, shadow, includeEmpty, background, made);
            }
        }
    }
}
