package cursed.recode.swap;

import java.net.InetSocketAddress;
import java.net.SocketAddress;
import java.util.Locale;
import net.minecraft.client.Minecraft;
import net.minecraft.client.multiplayer.ServerData;

/**
 * The server played on as far as its own rules go (native gui/swap.rs): FunTime, ReallyWorld, HolyWorld, AresMine,
 * or "" for the rest — by the address typed, the one it answers on (AresMine's addresses too) and its brand. The menu
 * is told when it changes (it hides what the server bans). CURSED_DEV_SERVER names one for the stand.
 */
public final class Servers {
    public static final String FUNTIME = "FunTime";
    public static final String REALLYWORLD = "ReallyWorld";
    public static final String HOLYWORLD = "HolyWorld";
    public static final String ARESMINE = "AresMine";
    private static final String[][] NAMES = {
        {FUNTIME, "funtime.su", "funtime.me", "funtime.ru"},
        {REALLYWORLD, "reallyworld.ru", "reallyworld.me", "reallyworld.su"},
        {HOLYWORLD, "holyworld.ru", "holyworld.io", "holyworld.me", "hollyworld.ru"},
        {ARESMINE, "aresmine.ru", "aresmine.me"},
    };
    /** The addresses a server answers on, when a name not listed leads there (as the free Cursed's restrict.rs). */
    private static final String[][] IPS = {
        {ARESMINE, "46.8.201.12", "46.8.201.13", "46.8.201.16", "46.8.201.17"},
    };
    private static final String DEV = cursed.recode.dev.Dev.on("CURSED_DEV_SERVER") ? System.getenv("CURSED_DEV_SERVER") : null;
    /** What the menu was told last (null: nothing yet). */
    private static String told;

    private Servers() {
    }

    /** The server played on, as its rules know it ("" none of them: singleplayer too). */
    public static String now(Minecraft mc) {
        if (DEV != null) {
            return DEV;
        }
        if (mc.getConnection() == null || mc.isLocalServer()) {
            return "";
        }
        ServerData d = mc.getCurrentServer();
        String brand = mc.getConnection().serverBrand();
        String remote = "";
        SocketAddress a = mc.getConnection().getConnection().getRemoteAddress();
        if (a instanceof InetSocketAddress in) {
            remote = in.getHostString();
        }
        for (String[] rule : NAMES) {
            String name = rule[0];
            if (brand != null && brand.toLowerCase(Locale.ROOT).contains(name.toLowerCase(Locale.ROOT))) {
                return name;
            }
            for (int i = 1; i < rule.length; i++) {
                if ((d != null && under(d.ip, rule[i])) || under(remote, rule[i])) {
                    return name;
                }
            }
        }
        for (String[] rule : IPS) {
            for (int i = 1; i < rule.length; i++) {
                if (rule[i].equals(remote) || (d != null && under(d.ip, rule[i]))) {
                    return rule[0];
                }
            }
        }
        return "";
    }

    /** Each tick: the menu told when the server changed. */
    static void tick(Minecraft mc) {
        String s = now(mc);
        if (!s.equals(told)) {
            told = s;
            cursed.recode.render.Native.guiCall("server\t" + s);
        }
    }

    /** Whether `host` (perhaps with its port) is `domain` or a name under it. */
    static boolean under(String host, String domain) {
        if (host == null) {
            return false;
        }
        String h = host.trim().toLowerCase(Locale.ROOT);
        int colon = h.lastIndexOf(':');
        if (colon > 0 && h.indexOf(':') == colon) {
            h = h.substring(0, colon);
        }
        while (h.endsWith(".")) {
            h = h.substring(0, h.length() - 1);
        }
        return h.equals(domain) || h.endsWith("." + domain);
    }
}
