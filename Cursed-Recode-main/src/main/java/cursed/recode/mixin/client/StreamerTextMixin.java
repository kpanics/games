package cursed.recode.mixin.client;

import cursed.recode.play.StreamerMode;
import net.minecraft.network.chat.Style;
import net.minecraft.util.FormattedCharSink;
import net.minecraft.util.StringDecomposer;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** Streamer Mode (cursed.recode.play.StreamerMode): the game's words go through its list as they are laid out to be
 *  drawn — the tab's, the name tags', the titles', every screen's. */
@Mixin(StringDecomposer.class)
public abstract class StreamerTextMixin {
    private static final ThreadLocal<Boolean> CURSED$IN = ThreadLocal.withInitial(() -> false);

    @Inject(method = "iterate(Ljava/lang/String;Lnet/minecraft/network/chat/Style;Lnet/minecraft/util/FormattedCharSink;)Z", at = @At("HEAD"), cancellable = true)
    private static void cursed$iterate(String string, Style style, FormattedCharSink output, CallbackInfoReturnable<Boolean> cir) {
        if (!StreamerMode.on() || CURSED$IN.get()) {
            return;
        }
        String m = StreamerMode.mask(string);
        if (m != string && !m.equals(string)) {
            CURSED$IN.set(true);
            try {
                cir.setReturnValue(StringDecomposer.iterate(m, style, output));
            } finally {
                CURSED$IN.set(false);
            }
        }
    }

    @Inject(method = "iterateFormatted(Ljava/lang/String;ILnet/minecraft/network/chat/Style;Lnet/minecraft/network/chat/Style;Lnet/minecraft/util/FormattedCharSink;)Z",
            at = @At("HEAD"), cancellable = true)
    private static void cursed$formatted(String string, int offset, Style current, Style reset, FormattedCharSink output, CallbackInfoReturnable<Boolean> cir) {
        if (offset != 0 || !StreamerMode.on() || CURSED$IN.get()) {
            return;
        }
        String m = StreamerMode.mask(string);
        if (m != string && !m.equals(string)) {
            CURSED$IN.set(true);
            try {
                cir.setReturnValue(StringDecomposer.iterateFormatted(m, 0, current, reset, output));
            } finally {
                CURSED$IN.set(false);
            }
        }
    }
}
