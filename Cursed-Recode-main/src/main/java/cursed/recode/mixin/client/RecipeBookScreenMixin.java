package cursed.recode.mixin.client;

import cursed.recode.hud.HudInv;
import net.minecraft.client.gui.screens.inventory.AbstractRecipeBookScreen;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** Custom Inventory: where a stack let go is thrown (the stage in the middle, beyond the columns). */
@Mixin(AbstractRecipeBookScreen.class)
public abstract class RecipeBookScreenMixin {
    /** Custom Inventory's recipe book: its field takes the keys and the letters while it has them. */
    @Inject(method = "keyPressed", at = @At("HEAD"), cancellable = true)
    private void cursed$recipesKey(net.minecraft.client.input.KeyEvent event, CallbackInfoReturnable<Boolean> cir) {
        if (cursed.recode.hud.HudPack.recipesKey((AbstractRecipeBookScreen<?>) (Object) this, event)) {
            cir.setReturnValue(true);
        }
    }

    @Inject(method = "charTyped", at = @At("HEAD"), cancellable = true)
    private void cursed$recipesChar(net.minecraft.client.input.CharacterEvent event, CallbackInfoReturnable<Boolean> cir) {
        if (cursed.recode.hud.HudPack.recipesTyped((AbstractRecipeBookScreen<?>) (Object) this, event.codepoint())) {
            cir.setReturnValue(true);
        }
    }

    @Inject(method = "hasClickedOutside", at = @At("HEAD"), cancellable = true)
    private void cursed$outside(double mx, double my, int xo, int yo, CallbackInfoReturnable<Boolean> cir) {
        if (cursed.recode.hud.HudPack.active((AbstractRecipeBookScreen<?>) (Object) this)) {
            cir.setReturnValue(cursed.recode.hud.HudPack.outside(mx, my));
        } else if (HudInv.active((AbstractRecipeBookScreen<?>) (Object) this)) {
            cir.setReturnValue(HudInv.outside(mx, my));
        }
    }
}
