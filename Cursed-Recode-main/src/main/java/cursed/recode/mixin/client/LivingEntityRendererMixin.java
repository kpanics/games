package cursed.recode.mixin.client;

import cursed.recode.scene.NameMemo;
import net.minecraft.client.renderer.entity.LivingEntityRenderer;
import net.minecraft.network.chat.Component;
import net.minecraft.world.entity.LivingEntity;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** isEntityUpsideDown with the name's string made once per name, not every frame (cursed.recode.scene.NameMemo). */
@Mixin(LivingEntityRenderer.class)
public abstract class LivingEntityRendererMixin {
    @Inject(method = "isEntityUpsideDown", at = @At("HEAD"), cancellable = true)
    private void cursed$upside(LivingEntity mob, CallbackInfoReturnable<Boolean> cir) {
        Component name = mob.getCustomName();
        if (name == null) {
            cir.setReturnValue(false);
            return;
        }
        NameMemo memo = (NameMemo) mob;
        if (memo.cursed$upsideName() != name) {
            String s = name.getString();
            memo.cursed$upside(name, "Dinnerbone".equals(s) || "Grumm".equals(s));
        }
        cir.setReturnValue(memo.cursed$upside());
    }
}
