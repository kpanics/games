package cursed.recode.scene;

import net.minecraft.network.chat.Component;
import org.jspecify.annotations.Nullable;

/**
 * A living entity's custom name as last looked at, and whether it reads Dinnerbone or Grumm
 * (LivingEntityRenderer.isEntityUpsideDown made a string of the name every frame). The name is kept as the object
 * it is: a new name is a new object.
 */
public interface NameMemo {
    @Nullable Component cursed$upsideName();

    boolean cursed$upside();

    void cursed$upside(Component name, boolean upside);
}
