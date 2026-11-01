package cursed.recode.mixin.client;

import com.mojang.blaze3d.platform.FramerateLimitTracker;
import cursed.recode.launch.LaunchScreen;
import cursed.recode.launch.Twin;
import net.minecraft.util.Util;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/**
 * The main menu is not held to 60 frames a second (the game's rule for menus outside a world): its swap chain shows
 * frames at the monitor's own rate there (CursedSurface), so nothing has to sleep between them.
 *
 * And the game is not "away from the keyboard" before anyone could touch it: its last input counted from 0 (the
 * computer's start, to its clock) made a first start (the game's default, "afk") throttle at once — and the game's
 * window, hidden under the window before the game where no input reaches it, never drew evenly enough to take over
 * (found 15.10 in a release's trial). Its last input is its start; while the window before the game plays, no limit.
 */
@Mixin(FramerateLimitTracker.class)
public abstract class FramerateLimitTrackerMixin {
    @Shadow
    private long latestInputTime;

    @Inject(method = "<init>", at = @At("TAIL"))
    private void cursed$inputAtStart(CallbackInfo ci) {
        this.latestInputTime = Util.getMillis();
    }

    @Inject(method = "getFramerateLimit", at = @At("HEAD"), cancellable = true)
    private void cursed$menuAtTheMonitorsRate(CallbackInfoReturnable<Integer> cir) {
        if (Twin.active() || LaunchScreen.paced() && !((FramerateLimitTracker) (Object) this).isHeavilyThrottled()) {
            cir.setReturnValue(260);
        }
    }
}
