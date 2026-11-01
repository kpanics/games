package cursed.recode.hud;

import cursed.recode.mixin.client.BossHealthOverlayAccess;
import cursed.recode.mixin.client.HudAccess;
import java.util.ArrayList;
import java.util.Iterator;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.UUID;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.Font;
import net.minecraft.client.gui.components.LerpingBossEvent;
import net.minecraft.network.chat.Component;
import net.minecraft.util.ARGB;
import net.minecraft.util.Mth;
import org.joml.Matrix4f;
import org.jspecify.annotations.Nullable;

/**
 * Other Custom Things (Interface; native gui/hudother.rs): the boss bars, the title and subtitle, the action bar and the
 * name tags over things (holograms). The HUD writes their words in its own letters (HudText: a server's glyphs and
 * colours stay), places them (elements of the HUD, dragged in the editor), draws the cards and the bars and moves them:
 * a boss bar slides in and out and its fill eases with a trail, a title comes in on its card, the action bar fades on
 * its pill. Here only what the game has. Name tags: their plate's colour and opacity, a shadow, a size, and they fade
 * with distance instead of popping out.
 */
public final class HudOther {
    private HudOther() {
    }

    /** A boss bar as shown: the last of it stays while it slides out. */
    private static final class Bar {
        Component name = Component.empty();
        float progress;
        int color, overlay;
        long seen, goneAt;
        float alpha;
    }

    private static final Map<UUID, Bar> BARS = new LinkedHashMap<>();
    private static final List<Bar> SHOWN = new ArrayList<>();

    private static boolean on, bars, titles, action, holograms;
    private static float barX, barY, barStep, barK = 1.0F, barAlpha;
    private static boolean barNames = true;
    private static float titleX, titleY, titleK = 1.0F, titleAlpha, titleSpeed = 1.0F;
    private static int titleAnim;
    private static float actX, actY, actK = 1.0F, actAlpha;
    private static int tagBg;
    private static float tagOpacity = 0.25F, tagK = 1.0F, tagFadeFrom = 40.0F;
    private static int tagColour;
    private static boolean tagShadow, tagFade = true;

    /** The title this frame: its words, how seen, how long it is in (s); the action bar's. */
    private static @Nullable Component title, subtitle, act;
    private static float tAlpha, tPhase, aAlpha;
    private static boolean aRainbow;
    private static float aHue;
    private static @Nullable Component lastAct;
    private static long actAt;

    public static boolean bars() {
        return on && bars;
    }

    public static boolean titles() {
        return on && titles;
    }

    public static boolean action() {
        return on && action;
    }

    /** From the HUD's state (Native.hudState: 40 on). */
    static void state(float[] s) {
        on = s[40] > 0.5F;
        bars = s[41] > 0.5F;
        barX = s[42];
        barY = s[43];
        barStep = s[44];
        barK = s[46] > 0.0F ? s[46] : 1.0F;
        barAlpha = s[47];
        barNames = s[48] > 0.5F;
        titles = s[50] > 0.5F;
        titleX = s[51];
        titleY = s[52];
        titleK = s[53] > 0.0F ? s[53] : 1.0F;
        titleAnim = (int) s[54];
        titleAlpha = s[55];
        titleSpeed = Math.max(0.25F, s[56]);
        action = s[57] > 0.5F;
        actX = s[58];
        actY = s[59];
        actK = s[60] > 0.0F ? s[60] : 1.0F;
        actAlpha = s[61];
        holograms = s[62] > 0.5F;
        tagBg = (int) s[63];
        tagOpacity = s[64];
        tagColour = (int) s[65];
        tagShadow = s[66] > 0.5F;
        tagFade = s[67] > 0.5F;
        tagK = s[68] > 0.0F ? s[68] : 1.0F;
        tagFadeFrom = s[69];
    }

    private static float ease(float x) {
        float t = Mth.clamp(x, 0.0F, 1.0F);
        return 1.0F - (1.0F - t) * (1.0F - t) * (1.0F - t);
    }

    /**
     * The boss bars, the title and the action bar for the HUD (gui/hudother.rs Other::parse): their numbers, and their
     * words' runs as text (each bar's name, the title and subtitle, the action bar's).
     */
    static void append(Minecraft mc, float partial, List<Double> n, StringBuilder text) {
        long now = System.nanoTime();
        Font font = mc.font;
        float gs = (float) mc.getWindow().getGuiScale();
        // the boss bars: each slides in when it comes, the last of it slides out
        var events = ((BossHealthOverlayAccess) mc.gui.hud.getBossOverlay()).cursed$events();
        for (Bar b : BARS.values()) {
            b.alpha = -1.0F;
        }
        for (var e : events.entrySet()) {
            LerpingBossEvent ev = e.getValue();
            Bar b = BARS.computeIfAbsent(e.getKey(), k -> {
                Bar nb = new Bar();
                nb.seen = now;
                return nb;
            });
            b.name = ev.getName();
            b.progress = ev.getProgress();
            b.color = ev.getColor().ordinal();
            b.overlay = ev.getOverlay().ordinal();
            b.goneAt = 0L;
            b.alpha = 0.0F;
        }
        SHOWN.clear();
        for (Iterator<Bar> it = BARS.values().iterator(); it.hasNext(); ) {
            Bar b = it.next();
            if (b.alpha < 0.0F && b.goneAt == 0L) {
                b.goneAt = now;
            }
            float in = ease((now - b.seen) / 1e9F / 0.35F);
            float out = b.goneAt == 0L ? 1.0F : 1.0F - (now - b.goneAt) / 1e9F / 0.35F;
            if (out <= 0.0F) {
                it.remove();
                continue;
            }
            b.alpha = in * ease(out);
            if (SHOWN.size() < 8) {
                SHOWN.add(b);
            }
        }
        n.add((double) SHOWN.size());
        n.add((double) gs);
        for (int i = 0; i < SHOWN.size(); i++) {
            Bar b = SHOWN.get(i);
            n.add((double) b.progress);
            n.add((double) b.color);
            n.add((double) b.overlay);
            n.add((double) font.width(b.name) * gs);
            n.add((double) b.alpha);
            n.add((double) (System.identityHashCode(b) & 0xffffff));
            text.append(HudText.encode(HudText.BAR + i, b.name)).append('\n');
        }
        // the title: how seen as the game has it, how long in
        HudAccess h = (HudAccess) mc.gui.hud;
        title = h.cursed$title();
        subtitle = h.cursed$subtitle();
        int tt = h.cursed$titleTime();
        tAlpha = 0.0F;
        if (title != null && tt > 0) {
            float f = tt - partial;
            int fi = Math.max(1, h.cursed$fadeIn()), st = h.cursed$stay(), fo = Math.max(1, h.cursed$fadeOut());
            float a = 1.0F;
            if (tt > fo + st) {
                a = (fi + st + fo - f) / fi;
            }
            if (tt <= fo) {
                a = f / fo;
            }
            tAlpha = Mth.clamp(a, 0.0F, 1.0F);
            tPhase = (fi + st + fo - f) / 20.0F;
        }
        n.add(tAlpha > 0.0F ? 1.0 : 0.0);
        n.add((double) tAlpha);
        n.add((double) tPhase);
        n.add(title == null ? 0.0 : (double) font.width(title) * gs);
        n.add(subtitle == null ? 0.0 : (double) font.width(subtitle) * gs);
        n.add(subtitle != null ? 1.0 : 0.0);
        if (tAlpha > 0.0F) {
            text.append(HudText.encode(HudText.TITLE, title)).append('\n');
            text.append(HudText.encode(HudText.SUBTITLE, subtitle)).append('\n');
        }
        // the action bar: it fades in when it changes, out in its last second
        act = h.cursed$action();
        int at = h.cursed$actionTime();
        aAlpha = 0.0F;
        if (act != null && at > 0) {
            if (act != lastAct) {
                if (lastAct == null || !act.getString().equals(lastAct.getString())) {
                    actAt = now;
                }
                lastAct = act;
            }
            float f = at - partial;
            aAlpha = Mth.clamp(f / 20.0F, 0.0F, 1.0F) * ease((now - actAt) / 1e9F / 0.25F);
            aRainbow = h.cursed$actionRainbow();
            aHue = f / 50.0F;
        } else {
            lastAct = null;
        }
        n.add(aAlpha > 0.0F ? 1.0 : 0.0);
        n.add((double) aAlpha);
        n.add(act == null ? 0.0 : (double) font.width(act) * gs);
        n.add((double) (now - actAt) / 1e9);
        // a record's "now playing": its colours go round
        n.add(aAlpha > 0.0F && aRainbow ? 1.0 : 0.0);
        if (aAlpha > 0.0F) {
            text.append(HudText.encode(HudText.ACTION, act)).append('\n');
        }
    }

    // ------------------------------------------------------------------------------------------ name tags

    /** This name tag's fade with distance (set as it is submitted). */
    private static float tagAlpha = 1.0F;

    public static boolean holograms() {
        return on && holograms;
    }

    /** A name tag begins (its pose relative to the camera): how far it is fades it. */
    public static void tag(Matrix4f pose) {
        if (!holograms() || !tagFade) {
            tagAlpha = 1.0F;
            return;
        }
        float d = (float) Math.sqrt(pose.m30() * pose.m30() + pose.m31() * pose.m31() + pose.m32() * pose.m32());
        float from = Math.min(tagFadeFrom, 63.0F);
        float k = Mth.clamp((d - from) / Math.max(1.0F, 64.0F - from), 0.0F, 1.0F);
        tagAlpha = 1.0F - k * k * (3.0F - 2.0F * k);
    }

    /** The plate's opacity (the game's as it is, none, or the client's). */
    public static float opacity(float game) {
        if (!holograms()) {
            return game;
        }
        return switch (tagBg) {
            case 1 -> 0.0F;
            case 2 -> tagOpacity;
            default -> game;
        };
    }

    /** The plate's colour (its alpha kept) and how far away it is. */
    public static int plate(int argb) {
        if (!holograms()) {
            return argb;
        }
        int a = ARGB.alpha(argb);
        int rgb = tagBg == 2 ? tagColour & 0xFFFFFF : argb & 0xFFFFFF;
        return ARGB.color(Math.round(a * tagAlpha), rgb);
    }

    /** The words' colour, faded with distance. */
    public static int words(int argb) {
        if (!holograms() || tagAlpha >= 0.999F) {
            return argb;
        }
        return ARGB.color(Math.max(5, Math.round(ARGB.alpha(argb) * tagAlpha)), argb & 0xFFFFFF);
    }

    public static boolean shadow(boolean game) {
        return holograms() ? tagShadow || game : game;
    }

    public static float scale() {
        return holograms() ? tagK : 1.0F;
    }
}
