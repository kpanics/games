package cursed.recode.mixin.client;

import cursed.recode.fx.ShulkerPreview;
import net.minecraft.world.item.component.ItemContainerContents;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Shulker Preview: the game's lines of what a box holds ("Diamond x3", "and 4 more…") give way to its grid. */
@Mixin(ItemContainerContents.class)
public abstract class ShulkerLinesMixin {
    @Inject(method = "addToTooltip", at = @At("HEAD"), cancellable = true)
    private void cursed$noLines(CallbackInfo ci) {
        if (ShulkerPreview.replacesLines()) {
            ci.cancel();
        }
    }
}
