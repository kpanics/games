package cursed.recode.mixin.client;

import java.util.List;
import net.minecraft.client.gui.components.CommandSuggestions;
import net.minecraft.util.FormattedCharSequence;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Accessor;

/** The client's own chat: the hints the game has for the field, or the command's usage. */
@Mixin(CommandSuggestions.class)
public interface CommandSuggestionsAccess {
    @Accessor("suggestions")
    CommandSuggestions.SuggestionsList cursed$list();

    @Accessor("commandUsage")
    List<FormattedCharSequence> cursed$usage();
}
