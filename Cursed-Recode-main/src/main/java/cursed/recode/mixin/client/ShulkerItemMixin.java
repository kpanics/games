package cursed.recode.mixin.client;

import cursed.recode.fx.ShulkerPreview;
import java.util.Optional;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.inventory.tooltip.TooltipComponent;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** Shulker Preview: a box (or the ender chest, as last seen) has a tooltip image of its own — its grid. */
@Mixin(Item.class)
public abstract class ShulkerItemMixin {
    @Inject(method = "getTooltipImage", at = @At("HEAD"), cancellable = true)
    private void cursed$grid(ItemStack stack, CallbackInfoReturnable<Optional<TooltipComponent>> cir) {
        Optional<TooltipComponent> image = ShulkerPreview.image(stack);
        if (image.isPresent()) {
            cir.setReturnValue(image);
        }
    }
}
