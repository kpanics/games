package cursed.recode.mixin.client;

import cursed.recode.hud.HudInv;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.screens.inventory.InventoryScreen;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/**
 * Custom Inventory (cursed.recode.hud.HudInv): the survival inventory drawn by the client. Its background (the game's
 * picture, the player in the box, the effects beside it) and its contents (labels, slots, the carried stack, tooltips,
 * the recipe book) are the client's; its recipe book's button goes.
 */
@Mixin(InventoryScreen.class)
public abstract class InventoryScreenMixin {
    @Inject(method = "extractBackground", at = @At("HEAD"), cancellable = true)
    private void cursed$background(GuiGraphicsExtractor graphics, int mouseX, int mouseY, float a, CallbackInfo ci) {
        InventoryScreen self = (InventoryScreen) (Object) this;
        if (HudInv.active(self)) {
            HudInv.background(self, graphics, mouseX, mouseY, a);
            ci.cancel();
        }
    }

    @Inject(method = "extractRenderState", at = @At("HEAD"), cancellable = true)
    private void cursed$contents(GuiGraphicsExtractor graphics, int mouseX, int mouseY, float a, CallbackInfo ci) {
        InventoryScreen self = (InventoryScreen) (Object) this;
        if (HudInv.active(self)) {
            HudInv.contents(self, graphics, mouseX, mouseY, a);
            ci.cancel();
        }
    }

    @Inject(method = "init", at = @At("TAIL"))
    private void cursed$init(CallbackInfo ci) {
        HudInv.init((InventoryScreen) (Object) this);
        cursed.recode.hud.HudPack.init((InventoryScreen) (Object) this);
    }
}
