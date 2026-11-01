package cursed.recode.fx;

import java.util.Map;
import net.minecraft.client.renderer.state.LightmapRenderState;
import org.joml.Vector3f;

/**
 * Full Bright (the menu's module "full-bright"; LightmapMixin): no more darkness, coming in and going out softly. The
 * game's light map is made over as it is updated — by its gamma pushed past the game's own (the light's differences
 * kept: the shade still darker than the open), or by a night vision's even light — and the Darkness of the deep dark
 * and a boss's darkening are taken off.
 */
public final class FullBright {
    private static final Vector3f WHITE = new Vector3f(1.0F, 1.0F, 1.0F);
    private static Map<String, String> set = Map.of();
    private static long polled;
    private static long last;
    /** How far in it is (0..1). */
    private static float in;
    /** The game's own values, as it last made them (it does so once a tick; between, the frames make them over again). */
    private static float brightness;
    private static float nightVision;
    private static org.joml.Vector3fc nightVisionColor = WHITE;
    private static float darkness;
    private static float boss;

    private FullBright() {
    }

    private static Map<String, String> settings() {
        long now = System.nanoTime();
        if (now - polled > 250_000_000L) {
            polled = now;
            set = Fx.settings("full-bright");
        }
        return set;
    }

    private static void restore(LightmapRenderState s) {
        s.brightness = brightness;
        s.nightVisionEffectIntensity = nightVision;
        s.nightVisionColor = nightVisionColor;
        s.darknessEffectScale = darkness;
        s.bossOverlayWorldDarkening = boss;
    }

    /** The light map about to be made (Lightmap.render): made over by how far in it is. */
    public static void apply(LightmapRenderState s) {
        if (s.needsUpdate) {
            brightness = s.brightness;
            nightVision = s.nightVisionEffectIntensity;
            nightVisionColor = s.nightVisionColor;
            darkness = s.darknessEffectScale;
            boss = s.bossOverlayWorldDarkening;
        }
        Map<String, String> v = settings();
        long now = System.nanoTime();
        float dt = last == 0 ? 0.0F : Math.min((now - last) / 1e9F, 0.1F);
        last = now;
        boolean on = Fx.on(v, "on", false);
        float fade = Fx.number(v, "Fade", 0.6F);
        float step = fade <= 0.01F ? 1.0F : dt / fade;
        float was = in;
        in = on ? Math.min(1.0F, in + step) : Math.max(0.0F, in - step);
        if (in <= 0.0F) {
            if (was > 0.0F) {
                // once more as the game has it
                restore(s);
                s.needsUpdate = true;
            }
            return;
        }
        restore(s);
        float k = in * in * (3.0F - 2.0F * in) * Fx.number(v, "Strength", 100.0F) / 100.0F;
        if (Fx.choice(v, "Mode", "Gamma").equals("Night vision")) {
            s.nightVisionEffectIntensity = Math.max(nightVision, k);
            s.nightVisionColor = WHITE;
        } else {
            // past the game's own gamma (its slider ends at 1): the dark lifted, the light's differences kept
            s.brightness = brightness + (1.0F + 4.0F * k - brightness) * k;
            s.nightVisionEffectIntensity = Math.max(nightVision, 0.12F * k);
        }
        if (Fx.on(v, "No Darkness effect", true)) {
            s.darknessEffectScale = darkness * (1.0F - in);
            s.bossOverlayWorldDarkening = boss * (1.0F - in);
        }
        s.needsUpdate = true;
    }
}
