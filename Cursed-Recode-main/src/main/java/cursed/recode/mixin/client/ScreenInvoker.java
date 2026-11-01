package cursed.recode.mixin.client;

import net.minecraft.client.gui.components.events.GuiEventListener;
import net.minecraft.client.gui.screens.Screen;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Invoker;

/** A screen's widget taken away (Custom Inventory: the survival inventory's recipe book button). */
@Mixin(Screen.class)
public interface ScreenInvoker {
    @Invoker("removeWidget")
    void cursed$remove(GuiEventListener listener);
}
