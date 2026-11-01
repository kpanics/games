package cursed.recode.mixin.client;

import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.gui.screens.inventory.EnchantmentScreen;
import net.minecraft.client.gui.screens.inventory.LoomScreen;
import net.minecraft.client.gui.screens.inventory.StonecutterScreen;
import net.minecraft.client.input.MouseButtonEvent;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/**
 * Custom Inventory (cursed.recode.hud.HudPack): the screens that act on a click themselves before the container's
 * handling (an enchantment chosen, a recipe, a pattern) hear it only when the blocks' editor does not take it.
 */
@Mixin({EnchantmentScreen.class, StonecutterScreen.class, LoomScreen.class})
public abstract class PackClickMixin {
    @Inject(method = "mouseClicked", at = @At("HEAD"), cancellable = true)
    private void cursed$first(MouseButtonEvent event, boolean doubleClick, CallbackInfoReturnable<Boolean> cir) {
        if (cursed.recode.hud.HudPack.takeClick((Screen) (Object) this, event)) {
            cir.setReturnValue(true);
        }
    }
}
