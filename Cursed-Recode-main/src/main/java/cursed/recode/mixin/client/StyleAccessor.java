package cursed.recode.mixin.client;

import net.minecraft.network.chat.FontDescription;
import net.minecraft.network.chat.Style;
import net.minecraft.network.chat.TextColor;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Accessor;

/** A style's own fields as set (null = taken from the parent), for name tags' keys (cursed.recode.scene.TextCache). */
@Mixin(Style.class)
public interface StyleAccessor {
    @Accessor("color")
    TextColor cursed$color();

    @Accessor("shadowColor")
    Integer cursed$shadowColor();

    @Accessor("bold")
    Boolean cursed$bold();

    @Accessor("italic")
    Boolean cursed$italic();

    @Accessor("underlined")
    Boolean cursed$underlined();

    @Accessor("strikethrough")
    Boolean cursed$strikethrough();

    @Accessor("obfuscated")
    Boolean cursed$obfuscated();

    @Accessor("font")
    FontDescription cursed$font();
}
