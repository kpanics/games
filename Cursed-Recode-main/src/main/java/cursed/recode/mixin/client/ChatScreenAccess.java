package cursed.recode.mixin.client;

import net.minecraft.client.gui.components.CommandSuggestions;
import net.minecraft.client.gui.components.EditBox;
import net.minecraft.client.gui.screens.ChatScreen;
import net.minecraft.network.chat.Style;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Accessor;
import org.spongepowered.asm.mixin.gen.Invoker;

/** The client's own chat (cursed.recode.hud.NChat): the chat screen's field and hints, and its clicking of words. */
@Mixin(ChatScreen.class)
public interface ChatScreenAccess {
    @Accessor("input")
    EditBox cursed$input();

    @Accessor("commandSuggestions")
    CommandSuggestions cursed$suggestions();

    @Invoker("handleComponentClicked")
    boolean cursed$clicked(Style style, boolean insertions);
}
