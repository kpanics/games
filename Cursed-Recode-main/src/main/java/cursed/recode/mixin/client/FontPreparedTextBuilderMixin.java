package cursed.recode.mixin.client;

import cursed.recode.scene.TextCache;
import cursed.recode.scene.TextPlan;
import net.minecraft.client.gui.Font;
import net.minecraft.client.gui.font.glyphs.BakedGlyph;
import net.minecraft.network.chat.Style;
import org.jspecify.annotations.Nullable;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Unique;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/**
 * Text with obfuscated characters is not kept laid out (its glyphs change every time; cursed.recode.scene.TextCache).
 * A prepared text keeps its plans for TextFeatureRenderer (cursed.recode.scene.TextPlan), from its second use.
 */
@Mixin(targets = "net.minecraft.client.gui.Font$PreparedTextBuilder")
public abstract class FontPreparedTextBuilderMixin implements TextPlan.Holder {
    @Unique
    private int cursed$uses;
    @Unique
    private TextPlan @Nullable [] cursed$plans;

    @Inject(method = "accept(ILnet/minecraft/network/chat/Style;Lnet/minecraft/client/gui/font/glyphs/BakedGlyph;)Z", at = @At("HEAD"))
    private void cursed$obfuscated(int position, Style style, BakedGlyph glyph, CallbackInfoReturnable<Boolean> cir) {
        if (style.isObfuscated()) {
            TextCache.sawObfuscated = true;
        }
    }

    /** The effects dropped: the plans had them. */
    @Inject(method = "discardEffects", at = @At("HEAD"))
    private void cursed$discard(CallbackInfo ci) {
        this.cursed$plans = null;
    }

    @Override
    public @Nullable TextPlan cursed$plan(Font.DisplayMode mode) {
        if (this.cursed$uses < 2) {
            this.cursed$uses++;
            return null;
        }
        if (this.cursed$plans == null) {
            this.cursed$plans = new TextPlan[Font.DisplayMode.values().length];
        }
        TextPlan plan = this.cursed$plans[mode.ordinal()];
        if (plan == null) {
            plan = TextPlan.of((Font.PreparedText) this, mode);
            this.cursed$plans[mode.ordinal()] = plan;
        }
        return plan;
    }
}
