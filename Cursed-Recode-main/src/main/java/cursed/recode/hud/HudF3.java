package cursed.recode.hud;

import net.minecraft.client.KeyMapping;
import net.minecraft.client.Options;

/**
 * Custom F3 (Interface → Custom F3, gui/hudf3.rs): its cards show while F3 is on, the game's own words of F3 are not
 * drawn then (DebugScreenOverlayMixin); and the F3 keys a player has no use for are cut, whatever the module
 * (KeyboardHandlerMixin, SettingsPage: not in the keys' list either) — the profiler, the charts, the dumps, the
 * copied commands, the crash.
 */
public final class HudF3 {
    /** The module on (the HUD's state, HudLayer). */
    public static volatile boolean active;

    private HudF3() {
    }

    /** The F3 keys cut: what they do is for the game's makers, not its players. */
    public static boolean cut(Options o, KeyMapping k) {
        return k == o.keyDebugCopyRecreateCommand || k == o.keyDebugDumpDynamicTextures || k == o.keyDebugProfiling || k == o.keyDebugDumpVersion
                || k == o.keyDebugPofilingChart || k == o.keyDebugFpsCharts || k == o.keyDebugNetworkCharts || k == o.keyDebugLightmapTexture
                || k == o.keyDebugSwitchTranslucencyMode || k == o.keyDebugCrash || (active && k == o.keyDebugDebugOptions);
    }
}
