package cursed.recode.mixin.client;

import net.minecraft.client.gui.components.EditBox;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Accessor;

/** The client's own chat: where the field's selection ends, the hint's ghost after its words. */
@Mixin(EditBox.class)
public interface EditBoxAccess {
    @Accessor("highlightPos")
    int cursed$highlight();

    @Accessor("suggestion")
    String cursed$suggestion();
}
