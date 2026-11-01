package cursed.recode.mixin.client;

import cursed.recode.render.Native;
import net.minecraft.client.resources.sounds.SoundInstance;
import net.minecraft.client.sounds.SoundEngine;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Unique;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.ModifyVariable;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** No Sound (native gui/nosound.rs): a sound starting is asked about — left out (not started), quieter, or as it is. */
@Mixin(SoundEngine.class)
public abstract class NoSoundMixin {
    /** How loud the sound starting now may be (1 as it is). */
    @Unique
    private float cursed$loud = 1.0F;

    @Inject(method = "play", at = @At("HEAD"), cancellable = true)
    private void cursed$ask(SoundInstance instance, CallbackInfoReturnable<SoundEngine.PlayResult> cir) {
        this.cursed$loud = 1.0F;
        String said = Native.guiCall("nosound\t" + instance.getIdentifier());
        if (said == null || said.isEmpty()) {
            return;
        }
        try {
            this.cursed$loud = Float.parseFloat(said.trim());
        } catch (NumberFormatException e) {
            return;
        }
        if (this.cursed$loud <= 0.001F) {
            cir.setReturnValue(SoundEngine.PlayResult.NOT_STARTED);
        }
    }

    @ModifyVariable(method = "play", at = @At(value = "STORE", ordinal = 0), ordinal = 0)
    private float cursed$quieter(float volume) {
        float k = this.cursed$loud;
        this.cursed$loud = 1.0F;
        return volume * k;
    }
}
