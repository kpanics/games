package cursed.recode.motion;

import org.jspecify.annotations.Nullable;

/** A player's render state with Living Motion's pose for this frame (native motion.rs's numbers), null: the game's own. */
public interface MotionPose {
    float @Nullable [] cursed$motion();

    void cursed$motion(float @Nullable [] pose);
}
