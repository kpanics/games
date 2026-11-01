package cursed.recode.fx;

import cursed.recode.render.Native;
import java.util.ArrayList;
import java.util.List;
import java.util.Locale;
import net.minecraft.client.Minecraft;
import net.minecraft.client.player.LocalPlayer;
import net.minecraft.world.entity.player.Player;

/**
 * Name Tags (native gui/nametags.rs draws them): while it is on, a player whose name the game would show (as the game
 * decides it: the team's rules, the distance) has it left out of the world (NameTagsMixin) and told here; each frame
 * their facts go to the HUD — where above them, their health, armour, ping. A sneaking one's only when in sight (the
 * game's own is not seen through a wall then either).
 */
public final class NameTags {
    private static volatile boolean on;
    private static final List<Player> SEEN = new ArrayList<>();

    private NameTags() {
    }

    public static boolean on() {
        return on;
    }

    /** A player's name the game was about to show (NameTagsMixin): ours instead. */
    public static void saw(Player p, boolean sneaking) {
        Minecraft mc = Minecraft.getInstance();
        LocalPlayer me = mc.player;
        if (me == null || p == me || SEEN.size() >= 128) {
            return;
        }
        if (sneaking && !me.hasLineOfSight(p)) {
            return;
        }
        SEEN.add(p);
    }

    /** Each frame, before the HUD is drawn: the players seen, told. */
    public static void frame(Minecraft mc, float partial) {
        StringBuilder b = new StringBuilder();
        var conn = mc.getConnection();
        for (Player p : SEEN) {
            if (p.isRemoved()) {
                continue;
            }
            var pos = p.getPosition(partial);
            int ping = -1;
            if (conn != null) {
                var info = conn.getPlayerInfo(p.getUUID());
                if (info != null) {
                    ping = info.getLatency();
                }
            }
            b.append(p.getScoreboardName()).append('\u001f')
                    .append(p.getDisplayName().getString().replace('\u001f', ' ').replace('\n', ' ')).append('\u001f')
                    .append(String.format(Locale.ROOT, "%.3f %.3f %.3f", pos.x, pos.y + p.getBbHeight() + 0.45, pos.z)).append('\u001f')
                    .append(String.format(Locale.ROOT, "%.1f %.1f %.1f", p.getHealth(), p.getMaxHealth(), p.getAbsorptionAmount())).append('\u001f')
                    .append(p.getArmorValue()).append('\u001f').append(ping).append('\n');
        }
        SEEN.clear();
        on = "1".equals(Native.guiCall("tags\tframe\t" + b));
    }
}
