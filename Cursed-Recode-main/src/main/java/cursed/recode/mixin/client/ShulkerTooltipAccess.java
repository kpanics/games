package cursed.recode.mixin.client;

import java.util.List;
import net.minecraft.client.gui.Font;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.screens.inventory.tooltip.ClientTooltipComponent;
import net.minecraft.client.gui.screens.inventory.tooltip.ClientTooltipPositioner;
import net.minecraft.resources.Identifier;
import org.jspecify.annotations.Nullable;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Invoker;

/** The game's own way a tooltip's lines are kept for the next frame (ShulkerTooltipMixin hands its grid to it). */
@Mixin(GuiGraphicsExtractor.class)
public interface ShulkerTooltipAccess {
    @Invoker("setTooltipForNextFrameInternal")
    void cursed$tooltip(Font font, List<ClientTooltipComponent> lines, int xo, int yo, ClientTooltipPositioner positioner, @Nullable Identifier style,
                        boolean replaceExisting, boolean extraSpaceAfterFirstLine);
}
