package cursed.recode.mixin.client;

import net.minecraft.client.multiplayer.MultiPlayerGameMode;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Invoker;

/** The held slot told to the server now (not at the next tick): Auto Swap picks a slot, acts with it and goes back
 *  in one tick (cursed.recode.swap.Moves). */
@Mixin(MultiPlayerGameMode.class)
public interface MultiPlayerGameModeAccess {
    @Invoker("ensureHasSentCarriedItem")
    void cursed$sendCarried();
}
