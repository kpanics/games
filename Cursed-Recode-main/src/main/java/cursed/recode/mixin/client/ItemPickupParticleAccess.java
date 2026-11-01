package cursed.recode.mixin.client;

import net.minecraft.client.particle.ItemPickupParticle;
import net.minecraft.client.renderer.entity.state.EntityRenderState;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Accessor;

@Mixin(ItemPickupParticle.class)
public interface ItemPickupParticleAccess {
    @Accessor("life")
    int cursed$life();

    @Accessor("itemRenderState")
    EntityRenderState cursed$state();
}
