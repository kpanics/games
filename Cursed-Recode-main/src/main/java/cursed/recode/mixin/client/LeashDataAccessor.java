package cursed.recode.mixin.client;

import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Accessor;

/** The id of a leash holder not met yet (EntityRendererMixin: a creature without one needs no lookup). */
@Mixin(targets = "net.minecraft.world.entity.Leashable$LeashData")
public interface LeashDataAccessor {
    @Accessor("delayedLeashHolderId")
    int cursed$delayedId();
}
