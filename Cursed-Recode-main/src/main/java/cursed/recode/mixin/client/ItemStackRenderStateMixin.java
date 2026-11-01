package cursed.recode.mixin.client;

import cursed.recode.dev.Dev;
import cursed.recode.scene.ItemBoxes;
import net.minecraft.client.renderer.item.ItemStackRenderState;
import net.minecraft.world.item.ItemDisplayContext;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.Vec3;
import org.jspecify.annotations.Nullable;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** Items' model bounding boxes remembered across frames (cursed.recode.scene.ItemBoxes). */
@Mixin(ItemStackRenderState.class)
public abstract class ItemStackRenderStateMixin {
    private static final boolean OFF = Dev.on("CURSED_DEV_NOITEMBOXES");
    /** CURSED_DEV_ITEMBOXCHECK: every remembered box against the game's computation. */
    private static final boolean CHECK = Dev.on("CURSED_DEV_ITEMBOXCHECK");
    private static long checked;
    private static long differ;

    @Shadow
    ItemDisplayContext displayContext;
    @Shadow
    private int activeLayerCount;
    @Shadow
    private @Nullable AABB cachedModelBoundingBox;
    @Shadow
    private ItemStackRenderState.LayerRenderState[] layers;

    @Inject(method = "getModelBoundingBox", at = @At("HEAD"), cancellable = true)
    private void cursed$box(CallbackInfoReturnable<AABB> cir) {
        if (OFF || this.cachedModelBoundingBox != null || !net.minecraft.client.Minecraft.getInstance().isSameThread()) {
            return;
        }
        ItemBoxes.Key key = ItemBoxes.probe(this.activeLayerCount, this.displayContext.leftHand());
        if (key == null) {
            return;
        }
        for (int i = 0; i < this.activeLayerCount; i++) {
            ItemStackRenderState.LayerRenderState layer = this.layers[i];
            key.layer(i, layer.extents.get(), layer.itemTransform, layer.localTransform);
        }
        AABB box = ItemBoxes.get(key);
        if (box == null || CHECK) {
            // ItemStackRenderState.getModelBoundingBox, as the game computes it.
            AABB.Builder builder = new AABB.Builder();
            ((ItemStackRenderState) (Object) this).visitExtents(builder::include);
            AABB computed = builder.isDefined() ? builder.build() : AABB.ofSize(Vec3.ZERO, 0.0, 0.0, 0.0);
            if (box != null) {
                checked++;
                if (!box.equals(computed)) {
                    differ++;
                    com.mojang.logging.LogUtils.getLogger().warn("[item boxes] remembered {} but the game computes {}", box, computed);
                }
                if (checked % 20000 == 0) {
                    com.mojang.logging.LogUtils.getLogger().info("[item boxes] checked {}, {} differ", checked, differ);
                }
            } else {
                ItemBoxes.put(key, computed);
            }
            box = computed;
        }
        this.cachedModelBoundingBox = box;
        cir.setReturnValue(box);
    }
}
