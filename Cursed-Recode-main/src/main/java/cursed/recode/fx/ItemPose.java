package cursed.recode.fx;

import java.util.List;
import org.jspecify.annotations.Nullable;

/** A dropped item's render state with Item Physic's copies for its frame (native itemphysic.rs's numbers), null: the
 *  game's own way; each copy's own shadow; how far its pickup's flight has gone (0: not flying). */
public interface ItemPose {
    float @Nullable [] cursed$pose();

    void cursed$pose(float @Nullable [] pose);

    @Nullable List<ItemPhysic.Shade> cursed$shades();

    void cursed$shades(@Nullable List<ItemPhysic.Shade> shades);

    float cursed$flight();

    void cursed$flight(float flight);
}
