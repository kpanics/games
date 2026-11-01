package cursed.recode.motion;

/** A double's pose as the native studio computed it: body, head, arms, legs; radians, three axes each. */
public interface StudioPose {
    float[] cursed$studio();
    void cursed$studio(float[] pose);
}
