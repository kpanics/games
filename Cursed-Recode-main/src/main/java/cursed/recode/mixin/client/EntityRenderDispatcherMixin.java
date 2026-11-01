package cursed.recode.mixin.client;

import cursed.recode.render.RendererSlot;
import java.util.Map;
import net.minecraft.client.entity.ClientMannequin;
import net.minecraft.client.player.AbstractClientPlayer;
import net.minecraft.client.renderer.entity.EntityRenderDispatcher;
import net.minecraft.client.renderer.entity.EntityRenderer;
import net.minecraft.server.packs.resources.ResourceManager;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EntityType;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.Unique;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/**
 * An entity's renderer from its type (cursed.recode.render.RendererSlot) instead of the renderers' map; players and
 * mannequins (a renderer per skin model) as the game does.
 */
@Mixin(EntityRenderDispatcher.class)
public abstract class EntityRenderDispatcherMixin {
    @Shadow
    private Map<EntityType<?>, EntityRenderer<?, ?>> renderers;

    /** Counts the renderers' remakes (1 the first): a type's kept renderer from an older one is looked up again. */
    @Unique
    private static int cursed$generation = 1;

    /** No Render (cursed.recode.fx.NoRender): falling sand and gravel not drawn (a lag machine's). */
    @Inject(method = "shouldRender", at = @At("HEAD"), cancellable = true)
    private <E extends Entity> void cursed$noFalling(E entity, net.minecraft.client.renderer.culling.Frustum frustum, double x, double y, double z, float partialTicks,
                                                      CallbackInfoReturnable<Boolean> cir) {
        if (entity instanceof net.minecraft.world.entity.item.FallingBlockEntity && cursed.recode.fx.NoRender.on("Falling blocks")) {
            cir.setReturnValue(false);
        }
    }

    @Inject(method = "onResourceManagerReload", at = @At("TAIL"))
    private void cursed$remade(ResourceManager resourceManager, CallbackInfo ci) {
        cursed$generation++;
        // the textures may be new: Chams' shadow finds their eyes again
        cursed.recode.chams.EyeMaps.reset();
    }

    @Inject(method = "getRenderer(Lnet/minecraft/world/entity/Entity;)Lnet/minecraft/client/renderer/entity/EntityRenderer;", at = @At("HEAD"), cancellable = true)
    private void cursed$kept(Entity entity, CallbackInfoReturnable<EntityRenderer<?, ?>> cir) {
        if (entity instanceof AbstractClientPlayer || entity instanceof ClientMannequin) {
            return;
        }
        RendererSlot slot = (RendererSlot) entity.getType();
        if (slot.cursed$generation() != cursed$generation) {
            slot.cursed$keep(this.renderers.get(entity.getType()), cursed$generation);
        }
        cir.setReturnValue((EntityRenderer<?, ?>) slot.cursed$renderer());
    }
}
