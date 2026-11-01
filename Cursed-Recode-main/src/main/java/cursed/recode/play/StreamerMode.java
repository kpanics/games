package cursed.recode.play;

import cursed.recode.render.Native;
import java.util.Locale;
import net.minecraft.client.Minecraft;
import net.minecraft.client.multiplayer.PlayerInfo;

/**
 * Streamer Mode (native gui/streamer.rs decides): each second the facts told — the player's nick, the server's address,
 * the nicks on it — and the list of what is put for what taken back; the game's words go through it as they are drawn
 * (StreamerTextMixin), the player's own skin is the default one on their screen while it says so (StreamerSkinMixin).
 */
public final class StreamerMode {
    private static volatile String[] from = new String[0];
    private static volatile String[] to = new String[0];
    private static volatile boolean[] host = new boolean[0];
    private static volatile boolean skin;
    private static int ticks;

    private StreamerMode() {
    }

    /** Each game tick (MinecraftMixin): the facts each second. */
    public static void tick(Minecraft mc) {
        if (++ticks % 20 != 0) {
            return;
        }
        String own = mc.getUser().getName();
        var server = mc.getCurrentServer();
        StringBuilder names = new StringBuilder();
        var conn = mc.getConnection();
        if (conn != null) {
            int n = 0;
            for (PlayerInfo pi : conn.getOnlinePlayers()) {
                if (n++ >= 300) {
                    break;
                }
                if (!names.isEmpty()) {
                    names.append(',');
                }
                names.append(pi.getProfile().name());
            }
        }
        String said = Native.guiCall("streamer\tfacts\t" + own + "\u001f" + (server == null ? "" : server.ip) + "\u001f" + names);
        if (said == null || said.isEmpty()) {
            from = new String[0];
            to = new String[0];
            host = new boolean[0];
            skin = false;
            return;
        }
        String[] lines = said.split("\n");
        int n = Math.max(0, lines.length - 1);
        String[] f = new String[n];
        String[] t = new String[n];
        boolean[] h = new boolean[n];
        int k = 0;
        boolean sk = false;
        for (String l : lines) {
            if (l.startsWith("skin ")) {
                sk = l.endsWith("1");
                continue;
            }
            String[] p = l.split("\u001f", 3);
            if (p.length == 3 && k < n) {
                h[k] = p[0].equals("h");
                f[k] = p[1];
                t[k] = p[2];
                k++;
            }
        }
        from = java.util.Arrays.copyOf(f, k);
        to = java.util.Arrays.copyOf(t, k);
        host = java.util.Arrays.copyOf(h, k);
        skin = sk;
    }

    public static boolean hidesSkin() {
        return skin;
    }

    public static boolean on() {
        return from.length > 0;
    }

    private static boolean hostChar(char c) {
        return (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || (c >= '0' && c <= '9') || c == '.' || c == '-';
    }

    /** A word as drawn: what is hidden put aside (as native gui/mask.rs does). */
    public static String mask(String s) {
        String[] f = from;
        if (f.length == 0 || s == null || s.isEmpty()) {
            return s;
        }
        String[] t = to;
        boolean[] h = host;
        String out = s;
        for (int i = 0; i < f.length && i < t.length && i < h.length; i++) {
            String end = f[i];
            if (end.isEmpty()) {
                continue;
            }
            if (!h[i]) {
                if (out.contains(end)) {
                    out = out.replace(end, t[i]);
                }
                continue;
            }
            String low = out.toLowerCase(Locale.ROOT);
            if (!low.contains(end) || low.length() != out.length()) {
                continue;
            }
            StringBuilder res = new StringBuilder(out.length());
            int at = 0;
            int found;
            while ((found = low.indexOf(end, at)) >= 0) {
                int a = found;
                while (a > at && hostChar(out.charAt(a - 1))) {
                    a--;
                }
                int b = found + end.length();
                if (b < out.length() && out.charAt(b) == ':') {
                    int c = b + 1;
                    while (c < out.length() && Character.isDigit(out.charAt(c))) {
                        c++;
                    }
                    if (c > b + 1) {
                        b = c;
                    }
                }
                res.append(out, at, a).append(t[i]);
                at = b;
            }
            res.append(out, at, out.length());
            out = res.toString();
        }
        return out;
    }
}
