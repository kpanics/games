package cursed.recode.mixin.client;

import cursed.recode.launch.Transition;
import java.util.function.BooleanSupplier;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.screens.worldselection.WorldOpenFlows;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Redirect;

/**
 * Opening a world of one's own reads its data and datapacks while the game waits (WorldOpenFlows.loadWorldDataBlocking:
 * managedBlock, no frames) — some 0.9 s in which the way in (launch/Transition, «ink») stood still in the middle of
 * the valley going into its lines. While the way in plays, the wait draws frames, as the game's own wait for its
 * server does (Minecraft.doWorldLoad renders inside its waiting loop).
 */
@Mixin(WorldOpenFlows.class)
public abstract class WorldOpenFlowsMixin {
    @Redirect(method = "loadWorldDataBlocking", at = @At(value = "INVOKE", target = "Lnet/minecraft/client/Minecraft;managedBlock(Ljava/util/function/BooleanSupplier;)V"))
    private void cursed$framesWhileWaiting(Minecraft mc, BooleanSupplier done) {
        if (!Transition.entering()) {
            mc.managedBlock(done);
            return;
        }
        long[] last = {System.nanoTime()};
        boolean[] inFrame = {false};
        mc.managedBlock(() -> {
            if (done.getAsBoolean()) {
                return true;
            }
            // a frame each refresh or so (the menus' pace waits for the monitor's by itself)
            long now = System.nanoTime();
            if (!inFrame[0] && now - last[0] >= 4_000_000L) {
                inFrame[0] = true;
                try {
                    mc.renderFrame(false);
                } finally {
                    inFrame[0] = false;
                    last[0] = System.nanoTime();
                }
            }
            return done.getAsBoolean();
        });
    }
}
