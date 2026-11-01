package cursed.recode.play;

import cursed.recode.render.Native;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.screens.DeathScreen;
import net.minecraft.client.player.LocalPlayer;
import net.minecraft.network.chat.ClickEvent;
import net.minecraft.network.chat.Component;
import net.minecraft.network.chat.HoverEvent;
import net.minecraft.world.effect.MobEffects;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.player.Player;

import java.util.Locale;

/**
 * The modules that act in the game (native gui/play.rs decides; Auto Sprint, Auto Respawn, Death Coords and those
 * after them): each tick the game's facts told in one line, and what comes back done — the player made to run, back
 * from the death screen, a line of the chat only the player sees, a line said, a command sent.
 */
public final class Play {
    private Play() {
    }

    private static void put(StringBuilder b, String k, Object v) {
        if (!b.isEmpty()) {
            b.append('\u001f');
        }
        b.append(k).append('=').append(String.valueOf(v).replace('\u001f', ' ').replace('\n', ' '));
    }

    private static String bit(boolean v) {
        return v ? "1" : "0";
    }

    /** A hit about to go (MultiPlayerGameModeMixin): held back when it is at a friend (No Friend Damage). */
    public static boolean spared(Player player, Entity target) {
        if (!(target instanceof Player other) || other == player) {
            return false;
        }
        return "1".equals(Native.guiCall("play\thit\t" + other.getScoreboardName() + "\t" + (player.isAlliedTo(other) ? "1" : "0")));
    }

    /** A line come to the chat (ChatComponentMixin): its words and the commands its buttons run, told (Auto Accept). */
    public static void chat(Component c, net.minecraft.client.multiplayer.chat.GuiMessageSource source) {
        StringBuilder cmds = new StringBuilder();
        c.visit((style, text) -> {
            ClickEvent e = style.getClickEvent();
            String cmd = e instanceof ClickEvent.RunCommand run ? run.command() : e instanceof ClickEvent.SuggestCommand sug ? sug.command() : null;
            if (cmd != null && cmds.indexOf(cmd) < 0) {
                if (!cmds.isEmpty()) {
                    cmds.append('\u001e');
                }
                cmds.append(cmd.replace('\u001e', ' ').replace('\u001f', ' '));
            }
            return java.util.Optional.empty();
        }, net.minecraft.network.chat.Style.EMPTY);
        String text = c.getString().replace('\u001f', ' ').replace('\n', ' ');
        Native.guiCall("play\tchat\t" + source.name() + "\u001f" + text + "\u001f" + cmds);
    }

    /** A kill (HudVars: the one fought died after the player's hit): who, how far, the player's health, the weapon (Kill
     *  Message). */
    public static void kill(net.minecraft.world.entity.LivingEntity t) {
        Minecraft mc = Minecraft.getInstance();
        LocalPlayer p = mc.player;
        if (p == null || t == null) {
            return;
        }
        boolean player = t instanceof Player;
        String name = player ? ((Player) t).getScoreboardName() : t.getName().getString();
        var held = p.getMainHandItem();
        var server = mc.getCurrentServer();
        String facts = String.join("\u001f",
                name.replace('\u001f', ' '), player ? "1" : "0", String.format(Locale.ROOT, "%.1f", p.distanceTo(t)),
                String.format(Locale.ROOT, "%.1f", p.getHealth() + p.getAbsorptionAmount()),
                held.isEmpty() ? "" : held.getHoverName().getString().replace('\u001f', ' '), p.getScoreboardName(), server == null ? "" : server.ip,
                // (where they stood, how tall: Kill Effects)
                String.format(Locale.ROOT, "%.2f %.2f %.2f", t.getX(), t.getY(), t.getZ()), String.format(Locale.ROOT, "%.2f", t.getBbHeight()));
        Native.guiCall("play\tkill\t" + facts);
    }

    /** Each game tick (MinecraftMixin). */
    public static void tick(Minecraft mc) {
        LocalPlayer p = mc.player;
        StringBuilder f = new StringBuilder();
        if (p == null || mc.level == null) {
            put(f, "world", "0");
        } else {
            var screen = mc.gui.screen();
            boolean dead = p.isDeadOrDying() || screen instanceof DeathScreen;
            put(f, "world", "1");
            put(f, "nick", p.getGameProfile().name());
            put(f, "dead", bit(dead));
            put(f, "hardcore", bit(mc.level.getLevelData().isHardcore()));
            put(f, "pos", String.format(Locale.ROOT, "%.2f %.2f %.2f", p.getX(), p.getY(), p.getZ()));
            put(f, "dim", mc.level.dimension().identifier());
            var server = mc.getCurrentServer();
            put(f, "server", server == null ? "" : server.ip);
            if (screen instanceof DeathScreen ds && ds.causeOfDeath != null) {
                put(f, "cause", ds.causeOfDeath.getString());
            }
            if (dead) {
                // who killed (a player: a revenge is theirs to come)
                var src = p.getLastDamageSource();
                Entity by = src == null ? null : src.getEntity();
                if (by instanceof Player killer && killer != p) {
                    put(f, "killer", killer.getScoreboardName());
                }
            }
            put(f, "fwd", bit(mc.options.keyUp.isDown()));
            put(f, "sneak", bit(p.isShiftKeyDown()));
            put(f, "using", bit(p.isUsingItem()));
            put(f, "food", p.getFoodData().getFoodLevel());
            put(f, "fly", bit(p.getAbilities().mayfly));
            put(f, "blind", bit(p.hasEffect(MobEffects.BLINDNESS)));
            put(f, "water", bit(p.isInWater()));
            put(f, "screen", bit(screen != null));
            put(f, "wall", bit(p.horizontalCollision && !p.minorHorizontalCollision));
            // Server Adaptation's: the hands, the others near, the cooldowns just started
            Adapt.facts(mc, p, (k, v) -> put(f, k, v));
        }
        Adapt.tick(mc);
        String said = Native.guiCall("play\ttick\t" + f);
        if (said == null || said.isEmpty()) {
            return;
        }
        for (String line : said.split("\n")) {
            act(mc, line);
        }
    }

    private static void act(Minecraft mc, String line) {
        LocalPlayer p = mc.player;
        int sp = line.indexOf(' ');
        String verb = sp < 0 ? line : line.substring(0, sp);
        String rest = sp < 0 ? "" : line.substring(sp + 1);
        switch (verb) {
            case "sprint" -> {
                if (p != null && !p.isSprinting()) {
                    p.setSprinting(true);
                }
            }
            case "respawn" -> {
                if (p != null) {
                    p.respawn();
                }
            }
            case "local" -> {
                String[] tc = rest.split("\u001f", 2);
                Component c = Component.literal(tc[0]);
                if (tc.length > 1 && !tc[1].isEmpty()) {
                    String copy = tc[1];
                    c = Component.literal(tc[0]).withStyle(s -> s.withClickEvent(new ClickEvent.CopyToClipboard(copy))
                            .withHoverEvent(new HoverEvent.ShowText(Component.literal(copy))));
                }
                mc.gui.hud.getChat().addClientSystemMessage(c);
            }
            case "say" -> {
                if (mc.getConnection() != null) {
                    mc.getConnection().sendChat(rest);
                }
            }
            case "cmd" -> {
                if (mc.getConnection() != null) {
                    mc.getConnection().sendCommand(rest.startsWith("/") ? rest.substring(1) : rest);
                }
            }
            // Kill Effects: what the game shows of a kill, its sound
            case "fx" -> KillFx.fx(mc, rest);
            case "sound" -> KillFx.sound(mc, rest);
            // Server Adaptation's key: a thing of a server's used
            case "use" -> Adapt.ask(mc, rest);
            // No Friend Damage's key: the player looked at, told back (a friend now, or no longer)
            case "friend?" -> {
                if (mc.hitResult instanceof net.minecraft.world.phys.EntityHitResult eh && eh.getEntity() instanceof Player other && other != p) {
                    Native.guiCall("play\tfriend\t" + other.getScoreboardName());
                }
            }
            default -> {
            }
        }
    }
}
