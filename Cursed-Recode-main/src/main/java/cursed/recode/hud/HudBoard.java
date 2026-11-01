package cursed.recode.hud;

import java.util.ArrayList;
import java.util.Comparator;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import net.minecraft.client.Minecraft;
import net.minecraft.network.chat.Component;
import net.minecraft.network.chat.numbers.NumberFormat;
import net.minecraft.network.chat.numbers.StyledFormat;
import net.minecraft.world.scores.DisplaySlot;
import net.minecraft.world.scores.Objective;
import net.minecraft.world.scores.PlayerScoreEntry;
import net.minecraft.world.scores.PlayerTeam;
import net.minecraft.world.scores.Scoreboard;
import net.minecraft.world.scores.TeamColor;
import org.jspecify.annotations.Nullable;

/**
 * The Custom Scoreboard (Interface; native gui/hudboard.rs): the server's sidebar as it sent it — its title, its lines,
 * their scores — written by the HUD in its own letters on its glass (HudText: the server's colours and a pack's glyphs
 * stay). Here only what the game has: the lines in the game's order, whether the scores are only the lines' numbers
 * (6, 5, 4…: the HUD can leave those out), a line that changed and when.
 */
public final class HudBoard {
    private HudBoard() {
    }

    private static final Comparator<PlayerScoreEntry> ORDER = Comparator.comparing(PlayerScoreEntry::value).reversed()
            .thenComparing(PlayerScoreEntry::owner, String.CASE_INSENSITIVE_ORDER);

    private static boolean active;
    private static int maxLines = 15;
    /** Each line's text last seen and since when it is so (a change is lit a moment). */
    private static final Map<String, String> SEEN = new HashMap<>();
    private static final Map<String, Long> CHANGED = new HashMap<>();

    public static boolean active() {
        return active;
    }

    /** From the HUD's state (Native.hudState: 25 on, 39 the most lines). */
    static void state(float[] s) {
        active = s[25] > 0.5F;
        maxLines = Math.max(1, Math.min(32, (int) s[39]));
    }

    /** The sidebar the game would show (a team's colour's slot before the plain one). */
    private static @Nullable Objective objective(Minecraft mc) {
        if (mc.level == null || mc.player == null) {
            return null;
        }
        Scoreboard board = mc.level.getScoreboard();
        PlayerTeam team = board.getPlayersTeam(mc.player.getScoreboardName());
        if (team != null) {
            Optional<TeamColor> c = team.getColor();
            if (c.isPresent()) {
                Objective o = board.getDisplayObjective(c.get().displaySlot());
                if (o != null) {
                    return o;
                }
            }
        }
        return board.getDisplayObjective(DisplaySlot.SIDEBAR);
    }

    /**
     * The sidebar for the HUD (gui/hudboard.rs Board::parse): the numbers — how many lines (-1: none), whether the
     * scores only count the lines, then each line's seconds since it changed; the text — the title's runs, then each
     * line's and its score's.
     */
    static void append(Minecraft mc, List<Double> n, StringBuilder text) {
        long now = System.nanoTime();
        Objective o = objective(mc);
        if (o == null) {
            n.add(-1.0);
            n.add(0.0);
            return;
        }
        Scoreboard board = o.getScoreboard();
        NumberFormat format = o.numberFormatOrDefault(StyledFormat.SIDEBAR_DEFAULT);
        List<PlayerScoreEntry> list = new ArrayList<>(board.listPlayerScores(o).stream().filter(s -> !s.isHidden()).sorted(ORDER).limit(maxLines).toList());
        // the scores only count the lines: less each line down, small, hardly a gap between them (a line left out
        // here and there: 6 5 4 3 1); or all the same
        boolean counter = list.size() >= 2;
        for (int i = 0; i + 1 < list.size() && counter; i++) {
            counter = list.get(i).value() > list.get(i + 1).value();
        }
        if (counter) {
            int top = list.get(0).value(), bottom = list.get(list.size() - 1).value();
            counter = bottom >= 0 && top <= 32 && top - bottom <= list.size() + 1;
        }
        if (!counter && list.size() >= 2) {
            counter = list.stream().allMatch(e -> e.value() == list.get(0).value());
        }
        n.add((double) list.size());
        n.add(counter ? 1.0 : 0.0);
        text.append(HudText.encode(HudText.BOARD_TITLE, o.getDisplayName())).append('\n');
        for (int i = 0; i < list.size(); i++) {
            PlayerScoreEntry e = list.get(i);
            PlayerTeam team = board.getPlayersTeam(e.owner());
            Component name = PlayerTeam.formatNameForTeam(team, e.ownerName());
            Component score = e.formatValue(format);
            String was = SEEN.put(e.owner(), name.getString() + "\u0000" + score.getString());
            if (was != null && !was.equals(SEEN.get(e.owner()))) {
                CHANGED.put(e.owner(), now);
            }
            Long at = CHANGED.get(e.owner());
            n.add(at == null ? 99.0 : (now - at) / 1e9);
            text.append(HudText.encode(HudText.BOARD_LINE + i, name)).append('\n');
            text.append(HudText.encode(HudText.BOARD_SCORE + i, score)).append('\n');
        }
        if (SEEN.size() > 64) {
            SEEN.keySet().removeIf(k -> list.stream().noneMatch(en -> en.owner().equals(k)));
            CHANGED.keySet().retainAll(SEEN.keySet());
        }
    }
}
