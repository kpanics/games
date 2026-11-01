package cursed.recode.mixin.client;

import cursed.recode.fx.ShulkerPreview;
import java.util.List;
import java.util.Optional;
import net.minecraft.client.gui.Font;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.screens.inventory.tooltip.ClientTooltipComponent;
import net.minecraft.client.gui.screens.inventory.tooltip.DefaultTooltipPositioner;
import net.minecraft.network.chat.Component;
import net.minecraft.resources.Identifier;
import net.minecraft.world.inventory.tooltip.TooltipComponent;
import org.jspecify.annotations.Nullable;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/**
 * Shulker Preview's grid in a tooltip (cursed.recode.fx.ShulkerPreview): the game makes a tooltip's image into its own
 * kinds only, so a box's grid is put in here instead, under the name as the game puts a bundle's.
 */
@Mixin(GuiGraphicsExtractor.class)
public abstract class ShulkerTooltipMixin {
    @Inject(method = "setTooltipForNextFrame(Lnet/minecraft/client/gui/Font;Ljava/util/List;Ljava/util/Optional;IILnet/minecraft/resources/Identifier;Z)V",
            at = @At("HEAD"), cancellable = true)
    private void cursed$grid(Font font, List<Component> texts, Optional<TooltipComponent> image, int xo, int yo, @Nullable Identifier style, boolean extra, CallbackInfo ci) {
        if (image.isPresent() && image.get() instanceof ShulkerPreview.Image grid) {
            List<ClientTooltipComponent> list = new java.util.ArrayList<>();
            for (Component t : texts) {
                list.add(ClientTooltipComponent.create(t.getVisualOrderText()));
            }
            list.add(list.isEmpty() ? 0 : 1, new ShulkerPreview.Client(grid));
            ((ShulkerTooltipAccess) this).cursed$tooltip(font, list, xo, yo, DefaultTooltipPositioner.INSTANCE, style, false, extra);
            ci.cancel();
        }
    }
}
