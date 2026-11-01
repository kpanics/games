package cursed.recode.mixin.client;

import net.minecraft.client.Minecraft;
import net.minecraft.client.renderer.entity.EntityRenderer;
import net.minecraft.client.renderer.entity.state.EntityRenderState;
import net.minecraft.core.BlockPos;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.LightLayer;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.Redirect;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.Leashable;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/**
 * No dark disc under a creature in the open while the engine casts its real shadow (cursed.recode.engine.Engine,
 * the casters): two shadows would show. Where no sun or moon reaches it (caves, houses), the game's disc stays.
 * And the frustum check's leash holder, for a creature out of view, without looking up an entity of id 0 when it has
 * no holder to meet (Leashable.getLeashHolder does for every creature: 12.10, a sixth of the entities' check).
 */
@Mixin(EntityRenderer.class)
public abstract class EntityRendererMixin {
    @Redirect(method = "shouldRender", at = @At(value = "INVOKE", target = "Lnet/minecraft/world/entity/Leashable;getLeashHolder()Lnet/minecraft/world/entity/Entity;"))
    private Entity cursed$leashHolder(Leashable leashable) {
        Leashable.LeashData data = leashable.getLeashData();
        if (data == null) {
            return null;
        }
        // with no delayed holder to find, the game's lookup (of id 0) finds nothing and it returns the holder it has
        if (((LeashDataAccessor) (Object) data).cursed$delayedId() == 0) {
            return data.leashHolder;
        }
        return leashable.getLeashHolder();
    }

    @Inject(method = "extractShadow", at = @At("HEAD"), cancellable = true)
    private void cursed$noDisc(EntityRenderState state, Minecraft minecraft, Level level, CallbackInfo ci) {
        if (cursed.recode.engine.Engine.castsShadows()
                && level.getBrightness(LightLayer.SKY, BlockPos.containing(state.x, state.y + 0.5, state.z)) >= 13) {
            state.shadowPieces.clear();
            state.shadowRadius = 0.0F;
            ci.cancel();
        }
    }
}
