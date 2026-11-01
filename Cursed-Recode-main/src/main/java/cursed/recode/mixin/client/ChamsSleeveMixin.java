package cursed.recode.mixin.client;

import cursed.recode.chams.ChamsHooks;
import net.minecraft.client.renderer.entity.player.AvatarRenderer;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.ModifyVariable;

/** Chams (cursed.recode.chams.Chams): the first person's arm in ink without its sleeve, as the body (ChamsHooks.bodyStart). */
@Mixin(AvatarRenderer.class)
public abstract class ChamsSleeveMixin {
    @ModifyVariable(method = {"renderRightHand", "renderLeftHand"}, at = @At("HEAD"), argsOnly = true, ordinal = 0)
    private boolean cursed$chamsSleeve(boolean visible) {
        return ChamsHooks.sleeve(visible);
    }
}
