package cursed.recode.fx;

import java.util.Map;
import net.minecraft.client.Minecraft;
import org.joml.Matrix4f;

/**
 * Aspect Ratio (the menu's module "aspect-ratio"; GameRendererMixin): the world drawn as on a screen of another width to
 * height and stretched over this one (narrower: what is seen wider), the hand too if set, the interface as it is. It
 * goes from one to another softly.
 */
public final class AspectRatio {
    private static Map<String, String> set = Map.of();
    private static long polled;
    private static long last;
    /** How much the world is stretched across now (1: not), gliding to what is set. */
    private static float factor = 1.0F;

    private AspectRatio() {
    }

    private static Map<String, String> settings() {
        long now = System.nanoTime();
        if (now - polled > 250_000_000L) {
            polled = now;
            set = Fx.settings("aspect-ratio");
        }
        return set;
    }

    private static float target(Map<String, String> v) {
        if (!Fx.on(v, "on", false)) {
            return 1.0F;
        }
        var w = Minecraft.getInstance().getWindow();
        if (w.getHeight() <= 0) {
            return 1.0F;
        }
        float screen = w.getWidth() / (float) w.getHeight();
        float ratio = switch (Fx.choice(v, "Ratio", "4:3")) {
            case "5:4" -> 5.0F / 4.0F;
            case "16:10" -> 16.0F / 10.0F;
            case "3:2" -> 3.0F / 2.0F;
            case "1:1" -> 1.0F;
            default -> 4.0F / 3.0F;
        };
        return screen / ratio;
    }

    /** The level's projection this frame (before the engine's jitter): stretched across. */
    public static void apply(Matrix4f projection) {
        Map<String, String> v = settings();
        long now = System.nanoTime();
        float dt = last == 0 ? 1.0F : Math.min((now - last) / 1e9F, 0.1F);
        last = now;
        float t = target(v);
        factor = Fx.on(v, "Smooth change", true) ? factor + (t - factor) * (1.0F - (float) Math.exp(-dt * 7.0)) : t;
        if (Math.abs(factor - t) < 1e-4F) {
            factor = t;
        }
        if (factor != 1.0F) {
            projection.scaleLocal(factor, 1.0F, 1.0F);
        }
    }

    /** The hand's projection's width (it is drawn as on a screen this narrower, when the hand stretches too). */
    public static float handWidth(float width) {
        return factor != 1.0F && Fx.on(settings(), "The hand too", true) ? Math.max(1.0F, width / factor) : width;
    }
}
