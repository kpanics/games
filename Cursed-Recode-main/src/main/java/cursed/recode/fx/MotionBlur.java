package cursed.recode.fx;

import java.util.Map;

/**
 * Motion Blur (the menu's module "motion-blur"): the world smeared along the way it moved on the screen since the last
 * frame, so a turn runs smoothly instead of jumping from frame to frame — each pixel found where it was then from its
 * depth and last frame's matrix (native/src/engine/fx.rs: BLUR). As long as at sixty frames a second whatever the frame
 * rate; the hand and the interface sharp; only the turn if set (the camera's going left out).
 */
final class MotionBlur {
    private static Map<String, String> set = Map.of();
    private static long polled;
    private static float shown;

    private MotionBlur() {
    }

    private static Map<String, String> settings() {
        long now = System.nanoTime();
        if (now - polled > 250_000_000L) {
            polled = now;
            set = Fx.settings("motion-blur");
        }
        return set;
    }

    static void draw(float dt) {
        Map<String, String> v = settings();
        float step = dt / 0.3F;
        shown = Fx.on(v, "on", false) ? Math.min(1.0F, shown + step) : Math.max(0.0F, shown - step);
        if (shown <= 0.0F) {
            return;
        }
        int samples = switch (Fx.choice(v, "Quality", "Medium")) {
            case "Low" -> 8;
            case "High" -> 24;
            default -> 14;
        };
        Fx.blur(Fx.number(v, "Strength", 55.0F) / 100.0F, samples, Fx.on(v, "Only when turning", true), Fx.number(v, "Longest smear", 6.0F) / 100.0F, shown);
    }
}
