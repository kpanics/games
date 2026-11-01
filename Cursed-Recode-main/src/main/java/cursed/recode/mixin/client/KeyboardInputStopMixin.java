package cursed.recode.mixin.client;

import net.minecraft.client.player.ClientInput;
import net.minecraft.client.player.KeyboardInput;
import net.minecraft.world.entity.player.Input;
import net.minecraft.world.phys.Vec2;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Auto Swap's stop for a swap from the inventory: the keys its timing says let go (walking, sprint, jump, sneak), as
 *  the game lets them go with the inventory open (cursed.recode.swap.AutoSwap.stopped) — the server is told they are
 *  not held, and with the walking ones the player stands. */
@Mixin(KeyboardInput.class)
public abstract class KeyboardInputStopMixin extends ClientInput {
    @Inject(method = "tick", at = @At("TAIL"))
    private void cursed$stop(CallbackInfo ci) {
        int keys = cursed.recode.swap.AutoSwap.stopped();
        if (keys == 0) {
            return;
        }
        boolean walk = (keys & 1) != 0;
        Input k = this.keyPresses;
        this.keyPresses = new Input(k.forward() && !walk, k.backward() && !walk, k.left() && !walk, k.right() && !walk, k.jump() && (keys & 4) == 0,
                k.shift() && (keys & 8) == 0, k.sprint() && (keys & 2) == 0);
        if (walk) {
            this.moveVector = Vec2.ZERO;
        }
    }
}
