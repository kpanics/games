package cursed.recode.mixin.client;

import cursed.recode.fx.FreeLook;
import net.minecraft.client.Minecraft;
import net.minecraft.client.MouseHandler;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Free Look (cursed.recode.fx.FreeLook): while it looks, the wheel brings the camera nearer or farther. */
@Mixin(MouseHandler.class)
public abstract class FreeLookScrollMixin {
    @Inject(method = "onScroll", at = @At("HEAD"), cancellable = true)
    private void cursed$distance(long handle, double xoffset, double yoffset, CallbackInfo ci) {
        Minecraft mc = Minecraft.getInstance();
        if (handle == mc.getWindow().handle() && mc.gui.screen() == null && FreeLook.scroll(yoffset)) {
            ci.cancel();
        }
    }
}
