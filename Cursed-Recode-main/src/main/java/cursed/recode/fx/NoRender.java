package cursed.recode.fx;

import java.util.Map;

/**
 * No Render (the menu's module "no-render"): what of the game's is left out, each thing its own switch. The menu says
 * which are on (a quarter of a second at most behind); one a server forbids (FunTime: the blindness's fog, the darkness's
 * pulse) is told off there by the menu itself. The game's places that ask: ScreenEffectRendererMixin, GameRendererMixin,
 * HudMixin, NoRenderFogMixin, LightmapMixin, WeatherEffectRendererMixin, ParticleEngineMixin, EntityRenderDispatcherMixin.
 */
public final class NoRender {
    private static Map<String, String> set = Map.of();
    private static long polled;

    private NoRender() {
    }

    /** Whether `label` is left out now (the module on, its switch on). */
    public static boolean on(String label) {
        long now = System.nanoTime();
        if (now - polled > 250_000_000L) {
            polled = now;
            set = Fx.settings("no-render");
        }
        return "1".equals(set.get("on")) && "1".equals(set.get(label));
    }
}
