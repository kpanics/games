package cursed.recode.mixin.client;

import com.mojang.brigadier.suggestion.Suggestion;
import java.util.List;
import net.minecraft.client.gui.components.CommandSuggestions;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Accessor;

/** The client's own chat: the hints shown, the chosen one, the first in view. */
@Mixin(CommandSuggestions.SuggestionsList.class)
public interface SuggestionsListAccess {
    @Accessor("suggestionList")
    List<Suggestion> cursed$all();

    @Accessor("current")
    int cursed$current();

    @Accessor("offset")
    int cursed$offset();
}
