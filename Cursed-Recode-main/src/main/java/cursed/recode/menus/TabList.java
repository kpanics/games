package cursed.recode.menus;

import com.google.gson.JsonArray;
import com.google.gson.JsonObject;
import cursed.recode.launch.LaunchScreen;
import cursed.recode.render.Native;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.List;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.Font;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.multiplayer.PlayerInfo;
import net.minecraft.client.multiplayer.ServerData;
import net.minecraft.network.chat.Component;
import net.minecraft.network.chat.numbers.StyledFormat;
import net.minecraft.util.FormattedCharSequence;
import net.minecraft.world.entity.player.PlayerSkin;
import net.minecraft.world.level.GameType;
import net.minecraft.world.scores.DisplaySlot;
import net.minecraft.world.scores.Objective;
import net.minecraft.world.scores.ReadOnlyScoreInfo;
import net.minecraft.world.scores.ScoreHolder;
import net.minecraft.world.scores.Scoreboard;
import net.minecraft.world.scores.criteria.ObjectiveCriteria;

/**
 * The player list (held Tab) as the menus' page "tab" over the game: a glass plate with the players in columns, each
 * with the face, the name, the ping as bars and the score; the server's header and footer above and under. The names,
 * the header and the footer are the server's text — colours, prefixes and the server's own glyphs — so the game draws
 * them (with its font, at whole pixels) where the page says; the rest is the page's.
 */
public final class TabList {
    private static int id;
    private static boolean open;
    private static long closedAt;
    private static long sentAt;
    private static String sent = "";
    private static final List<FormattedCharSequence> TEXTS = new ArrayList<>();
    private static final List<PlayerSkin> FACES = new ArrayList<>();
    private static final float[] SAID = new float[64];
    private static final Comparator<PlayerInfo> ORDER = Comparator.<PlayerInfo>comparingInt(p -> -p.getTabListOrder())
            .thenComparingInt(p -> p.getGameMode() == GameType.SPECTATOR ? 1 : 0)
            .thenComparing(p -> p.getTeam() != null ? p.getTeam().getName() : "")
            .thenComparing(p -> p.getProfile().name(), String::compareToIgnoreCase);

    private TabList() {
    }

    /** Is the page up or still sinking (its frames are drawn here, not by Menus.leftovers). */
    public static boolean active() {
        return open || System.nanoTime() - closedAt < 700_000_000L;
    }

    private static int text(Font font, FormattedCharSequence seq, JsonArray into) {
        into.add(TEXTS.size());
        into.add(font.width(seq));
        TEXTS.add(seq);
        return TEXTS.size() - 1;
    }

    /** CURSED_DEV_TAB="6-12,30": the list shown from 6 to 12 s in the world (the stand takes no keys), with 30 made-up players. */
    private static final String[] DEV = cursed.recode.dev.Dev.on("CURSED_DEV_TAB") ? System.getenv("CURSED_DEV_TAB").split(",") : null;
    private static long devT0;
    private static List<PlayerInfo> fake;

    private static List<PlayerInfo> fakes() {
        if (DEV == null || DEV.length < 2) {
            return List.of();
        }
        if (fake == null) {
            fake = new ArrayList<>();
            var tl = Minecraft.getInstance().gui.hud.getTabList();
            tl.setHeader(Component.literal("§6§lCURSED§r §fTEST SERVER\n§7play.example.net"));
            tl.setFooter(Component.literal("§7Shop: §ecursedclient.xyz §8| §7Discord: §9/cursed"));
            String[] names = {"anoxis", "Steve_PvP", "Alex", "suicid", "DarkNed", "kotik228", "xXProXx", "Notch", "jeb_", "Dinnerbone", "Grumm", "Herobrine"};
            int n = Integer.parseInt(DEV[1].trim());
            for (int i = 0; i < n; i++) {
                String name = names[i % names.length] + (i >= names.length ? String.valueOf(i) : "");
                var p = new PlayerInfo(new com.mojang.authlib.GameProfile(java.util.UUID.nameUUIDFromBytes(name.getBytes()), name), false);
                p.setLatency(i * 97 % 1200 - (i % 9 == 0 ? 1300 : 0));
                fake.add(p);
            }
        }
        return fake;
    }

    private static boolean devHeld() {
        if (DEV == null) {
            return false;
        }
        if (devT0 == 0) {
            devT0 = System.nanoTime();
        }
        float t = (System.nanoTime() - devT0) / 1e9F;
        String[] ab = DEV[0].split("-");
        return t >= Float.parseFloat(ab[0]) && t < Float.parseFloat(ab[1]);
    }

    private static JsonObject data(Minecraft mc, Scoreboard scoreboard, Objective objective) {
        TEXTS.clear();
        FACES.clear();
        Font font = mc.font;
        var tab = mc.gui.hud.getTabList();
        JsonObject o = new JsonObject();
        List<PlayerInfo> all = new ArrayList<>(mc.player.connection.getListedOnlinePlayers());
        all.addAll(fakes());
        List<PlayerInfo> players = all.stream().sorted(ORDER).limit(80L).toList();
        JsonArray rows = new JsonArray();
        for (PlayerInfo p : players) {
            JsonObject r = new JsonObject();
            JsonArray name = new JsonArray();
            text(font, tab.getNameForDisplay(p).getVisualOrderText(), name);
            r.add("name", name);
            r.addProperty("face", FACES.size());
            FACES.add(p.getSkin());
            r.addProperty("ping", p.getLatency());
            r.addProperty("spectator", p.getGameMode() == GameType.SPECTATOR);
            r.addProperty("me", p.getProfile().id().equals(mc.player.getUUID()));
            // (who of ours it is: the native side marks Recode's and Cursed's players by it)
            r.addProperty("nick", p.getProfile().name());
            if (objective != null) {
                ReadOnlyScoreInfo score = scoreboard.getPlayerScoreInfo(ScoreHolder.fromGameProfile(p.getProfile()), objective);
                if (score != null) {
                    if (objective.getRenderType() == ObjectiveCriteria.RenderType.HEARTS) {
                        r.addProperty("hearts", score.value());
                    } else {
                        JsonArray sc = new JsonArray();
                        text(font, score.formatValue(objective.numberFormatOrDefault(StyledFormat.PLAYER_LIST_DEFAULT)).getVisualOrderText(), sc);
                        r.add("score", sc);
                    }
                }
            }
            rows.add(r);
        }
        o.add("rows", rows);
        // the server's header and footer, as the game wraps them
        int wrap = Math.max(200, mc.getWindow().getGuiScaledWidth() - 100);
        for (String part : new String[] {"header", "footer"}) {
            Component c = part.equals("header") ? tab.header : tab.footer;
            JsonArray lines = new JsonArray();
            if (c != null) {
                for (FormattedCharSequence l : font.split(c, wrap)) {
                    JsonArray a = new JsonArray();
                    text(font, l, a);
                    lines.add(a);
                }
            }
            o.add(part, lines);
        }
        ServerData sd = mc.getCurrentServer();
        o.addProperty("where", sd != null ? (sd.name.isBlank() ? sd.ip : sd.name) : mc.getSingleplayerServer() != null && mc.getSingleplayerServer().getWorldData() != null
                ? mc.getSingleplayerServer().getWorldData().getLevelName() : "");
        o.addProperty("count", all.size());
        o.addProperty("guiScale", mc.getWindow().getGuiScale());
        return o;
    }

    /**
     * The game's turn to draw its player list (Hud.extractTabList, cancelled): the page opened while Tab is held (as
     * the game decides), its data while it changes, a frame each time while it is up or sinking.
     */
    public static void extract(GuiGraphicsExtractor graphics) {
        Minecraft mc = Minecraft.getInstance();
        if (mc.level == null || mc.player == null) {
            if (open) {
                Native.menusClose(id);
                open = false;
            }
            return;
        }
        Scoreboard scoreboard = mc.level.getScoreboard();
        Objective objective = scoreboard.getDisplayObjective(DisplaySlot.LIST);
        if (cursed.recode.dev.Dev.on("CURSED_DEV_VANILLASCREENS")) {
            return;
        }
        boolean show = devHeld() || mc.options.keyPlayerList.isDown() && !mc.gui.hud.isHidden()
                && !(mc.isLocalServer() && mc.player.connection.getListedOnlinePlayers().size() <= 1 && objective == null);
        mc.gui.hud.getTabList().setVisible(show);
        long now = System.nanoTime();
        if (show) {
            if (!open || now - sentAt > 100_000_000L) {
                sentAt = now;
                String s = data(mc, scoreboard, objective).toString();
                if (!open) {
                    id = MenuHost.nextId();
                    MenuHost.syncLanguage();
                    Native.menusOpen(id, "tab", s.getBytes(StandardCharsets.UTF_8));
                    open = true;
                } else if (!s.equals(sent)) {
                    Native.menusData(id, s.getBytes(StandardCharsets.UTF_8));
                }
                sent = s;
            }
        } else if (open) {
            Native.menusClose(id);
            open = false;
            closedAt = now;
        }
        if (!active() || Menus.host(mc.gui.screen()) != null) {
            return;
        }
        int w = mc.getWindow().getWidth(), h = mc.getWindow().getHeight();
        int frame = Native.menusFrame(w, h, LaunchScreen.menusClock(), MenuHost.inp(Page.NONE), true, SAID);
        Toasts.framed = true;
        MenuHost.poll(a -> { });
        Host.blit(graphics, frame, w, h);
        extras(graphics);
    }

    /** The faces and the server's text where the page says: text at `size` window px a font pixel (whole). */
    private static void extras(GuiGraphicsExtractor g) {
        Minecraft mc = Minecraft.getInstance();
        double s = mc.getWindow().getGuiScale();
        Extras.each((kind, ref, x, y, size, alpha) -> {
            if (kind == Extras.FACE && ref >= 0 && ref < FACES.size()) {
                Extras.face(g, FACES.get(ref), x, y, size, alpha);
            } else if (kind == 3 && ref >= 0 && ref < TEXTS.size() && alpha > 0.02F) {
                float k = size / (float) s;
                g.pose().pushMatrix();
                g.pose().translate(x / (float) s, y / (float) s);
                g.pose().scale(k, k);
                g.text(mc.font, TEXTS.get(ref), 0, 0, net.minecraft.util.ARGB.white(alpha), true);
                g.pose().popMatrix();
            }
        });
    }
}
