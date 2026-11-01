package cursed.recode.mixin.client;

import cursed.recode.fx.ItemPhysic;
import cursed.recode.fx.ItemPose;
import java.util.List;
import net.minecraft.client.renderer.entity.state.ItemEntityRenderState;
import org.jspecify.annotations.Nullable;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Unique;

/** A dropped item's render state carries Item Physic's copies for its frame, their shadows, its pickup's flight
 *  (cursed.recode.fx.ItemPhysic). */
@Mixin(ItemEntityRenderState.class)
public abstract class ItemEntityRenderStatePoseMixin implements ItemPose {
    @Unique
    private float @Nullable [] cursed$copies;
    @Unique
    private @Nullable List<ItemPhysic.Shade> cursed$shadows;
    @Unique
    private float cursed$flown;

    @Override
    public float @Nullable [] cursed$pose() {
        return this.cursed$copies;
    }

    @Override
    public void cursed$pose(float @Nullable [] pose) {
        this.cursed$copies = pose;
    }

    @Override
    public @Nullable List<ItemPhysic.Shade> cursed$shades() {
        return this.cursed$shadows;
    }

    @Override
    public void cursed$shades(@Nullable List<ItemPhysic.Shade> shades) {
        this.cursed$shadows = shades;
    }

    @Override
    public float cursed$flight() {
        return this.cursed$flown;
    }

    @Override
    public void cursed$flight(float flight) {
        this.cursed$flown = flight;
    }
}
