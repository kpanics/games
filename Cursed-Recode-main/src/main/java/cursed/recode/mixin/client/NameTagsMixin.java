package cursed.recode.mixin.client;

import cursed.recode.fx.NameTags;
import net.minecraft.client.renderer.entity.EntityRenderer;
import net.minecraft.client.renderer.entity.state.EntityRenderState;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.player.Player;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Name Tags (cursed.recode.fx.NameTags): a player's name the game would show, left out of the world and told to it
 *  (the score under the name stays the game's). */
@Mixin(EntityRenderer.class)
public abstract class NameTagsMixin {
    @Inject(method = "extractNameTags(Lnet/minecraft/world/entity/Entity;Lnet/minecraft/client/renderer/entity/state/EntityRenderState;FDD)V", at = @At("TAIL"))
    private void cursed$ours(Entity entity, EntityRenderState state, float partialTicks, double nameTagDistance, double belowNameDistance, CallbackInfo ci) {
        if (NameTags.on() && entity instanceof Player p && state.nameTag != null) {
            NameTags.saw(p, p.isDiscrete());
            state.nameTag = null;
        }
    }
}
