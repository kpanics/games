package cursed.recode.mixin.client;

import cursed.recode.motion.MotionPose;
import net.minecraft.client.renderer.entity.state.AvatarRenderState;
import org.jspecify.annotations.Nullable;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Unique;

/** A player's render state carries Living Motion's pose for its frame (cursed.recode.motion.LivingMotion). */
@Mixin(AvatarRenderState.class)
public abstract class AvatarRenderStateMotionMixin implements MotionPose, cursed.recode.motion.StudioPose {
    @Unique
    private float @Nullable [] cursed$pose;

    @Unique
    private float[] cursed$studioPose;

    @Override public float[] cursed$studio() { return this.cursed$studioPose; }
    @Override public void cursed$studio(float[] pose) { this.cursed$studioPose = pose; }

    @Override
    public float @Nullable [] cursed$motion() {
        return this.cursed$pose;
    }

    @Override
    public void cursed$motion(float @Nullable [] pose) {
        this.cursed$pose = pose;
    }
}
