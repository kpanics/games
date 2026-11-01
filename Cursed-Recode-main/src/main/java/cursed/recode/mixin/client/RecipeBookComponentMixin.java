package cursed.recode.mixin.client;

import cursed.recode.hud.HudInv;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.screens.recipebook.RecipeBookComponent;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** Custom Inventory: the game's recipe book is not there (it neither draws nor takes clicks or keys). */
@Mixin(RecipeBookComponent.class)
public abstract class RecipeBookComponentMixin {
    /** Nor does it keep itself in step with the book's settings (it would open itself again every tick): the
     *  client's book (HudRecipes) does that. */
    @Inject(method = "tick", at = @At("HEAD"), cancellable = true)
    private void cursed$still(org.spongepowered.asm.mixin.injection.callback.CallbackInfo ci) {
        if (cursed.recode.hud.HudPack.active(Minecraft.getInstance().gui.screen())) {
            ci.cancel();
        }
    }

    @Inject(method = "isVisible", at = @At("HEAD"), cancellable = true)
    private void cursed$hidden(CallbackInfoReturnable<Boolean> cir) {
        var screen = Minecraft.getInstance().gui.screen();
        if (HudInv.active(screen) || cursed.recode.hud.HudPack.active(screen)) {
            cir.setReturnValue(false);
        }
    }
}
