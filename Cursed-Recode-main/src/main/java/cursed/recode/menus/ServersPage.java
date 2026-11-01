package cursed.recode.menus;

import com.google.gson.JsonArray;
import com.google.gson.JsonObject;
import com.mojang.blaze3d.platform.NativeImage;
import cursed.recode.CursedRecode;
import cursed.recode.launch.LastServer;
import java.net.UnknownHostException;
import java.util.Optional;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import net.minecraft.SharedConstants;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.components.AbstractButton;
import net.minecraft.client.gui.screens.multiplayer.JoinMultiplayerScreen;
import net.minecraft.client.gui.screens.multiplayer.ServerSelectionList;
import net.minecraft.client.multiplayer.ServerData;
import net.minecraft.network.chat.CommonComponents;
import net.minecraft.network.chat.Component;
import net.minecraft.network.chat.FormattedText;
import net.minecraft.network.chat.Style;
import net.minecraft.server.network.EventLoopGroupHolder;
import org.jspecify.annotations.Nullable;

/**
 * Multiplayer (the game's server list) for the menus' page: her servers with their icons, their description in their
 * own colours, how many play, how fast they answer (asked as the game asks, aside); games on the network; joining,
 * moving one up or down, taking one off (it can come back for a while), adding, editing, a server by its address,
 * asking them all again — the game's own screen does each.
 */
final class ServersPage implements Page {
    final JoinMultiplayerScreen screen;
    final Minecraft mc = Minecraft.getInstance();
    private static final ExecutorService PINGS = Executors.newFixedThreadPool(5, r -> {
        Thread t = new Thread(r, "Cursed server pings");
        t.setDaemon(true);
        return t;
    });
    private ServerSelectionList.@Nullable Entry[] numbered = new ServerSelectionList.Entry[0];
    /** The versions a server can be set to, newest first: [protocol, name, whether it can be joined yet]. */
    private static final JsonArray VERSIONS = versions();

    private static JsonArray versions() {
        JsonArray a = new JsonArray();
        for (var v : cursed.recode.via.Via.releases()) {
            JsonArray e = new JsonArray();
            e.add(v.getVersion());
            e.add(v.getName());
            e.add(v.getVersion() == cursed.recode.via.Via.OWN || v.getVersion() >= cursed.recode.via.Via.OLDEST);
            a.add(e);
        }
        return a;
    }

    /** The server taken off last (it comes back with Undo) and where it was. */
    private @Nullable ServerData removed;
    private int removedAt;

    ServersPage(JoinMultiplayerScreen screen) {
        this.screen = screen;
    }

    /** The description as runs of one style: text, colour (rgb, -1 none), bold, italic, underlined, struck, obfuscated. */
    static JsonArray styled(Component c) {
        JsonArray runs = new JsonArray();
        // (a description's colours written into its text — § and a letter, as most servers do — read as the game does)
        cursed.recode.hud.HudText.pieces(c, (Style style, String text) -> {
            JsonArray r = new JsonArray();
            r.add(text);
            r.add(style.getColor() != null ? style.getColor().getValue() : -1);
            r.add((style.isBold() ? 1 : 0) | (style.isItalic() ? 2 : 0) | (style.isUnderlined() ? 4 : 0) | (style.isStrikethrough() ? 8 : 0) | (style.isObfuscated() ? 16 : 0));
            runs.add(r);
        });
        return runs;
    }

    /** Asked as the game's list asks when it draws a server (it is never drawn here). */
    private void ping(ServerData data) {
        if (data.state() != ServerData.State.INITIAL) {
            return;
        }
        data.setState(ServerData.State.PINGING);
        data.motd = CommonComponents.EMPTY;
        data.status = CommonComponents.EMPTY;
        PINGS.submit(() -> {
            try {
                this.screen.getPinger().pingServer(data, () -> this.mc.execute(() -> this.screen.getServers().save()),
                        () -> data.setState(data.protocol == SharedConstants.getCurrentVersion().protocolVersion() ? ServerData.State.SUCCESSFUL : ServerData.State.INCOMPATIBLE),
                        EventLoopGroupHolder.remote(this.mc.options.useNativeTransport()));
            } catch (UnknownHostException e) {
                data.setState(ServerData.State.UNREACHABLE);
                data.motd = Component.translatable("multiplayer.status.cannot_resolve");
            } catch (Exception e) {
                data.setState(ServerData.State.UNREACHABLE);
                data.motd = Component.translatable("multiplayer.status.cannot_connect");
            }
        });
    }

    @Override
    public JsonObject data() {
        JsonObject o = new JsonObject();
        JsonArray servers = new JsonArray(), lan = new JsonArray();
        var list = this.screen.serverSelectionList;
        java.util.List<ServerSelectionList.Entry> all = new java.util.ArrayList<>();
        int sel = -1;
        LastServer.Entry last = LastServer.get();
        if (list != null) {
            for (var e : list.children()) {
                if (e instanceof ServerSelectionList.OnlineServerEntry se) {
                    ServerData d = se.getServerData();
                    this.ping(d);
                    JsonObject s = new JsonObject();
                    s.addProperty("name", d.name);
                    s.addProperty("ip", d.ip);
                    s.add("motd", styled(d.motd));
                    s.addProperty("state", d.state().ordinal());
                    if (d.players != null) {
                        s.addProperty("online", d.players.online());
                        s.addProperty("max", d.players.max());
                    }
                    s.addProperty("ping", d.ping);
                    s.addProperty("version", d.version.getString());
                    // its version: as it says (once it answered) and the one chosen for it by hand (0 none)
                    boolean said = d.state() == ServerData.State.SUCCESSFUL || d.state() == ServerData.State.INCOMPATIBLE;
                    s.addProperty("proto", said ? cursed.recode.via.Compatibility.automatic(d.ip, d.protocol) : -1);
                    s.addProperty("via", cursed.recode.via.Choices.of(d.ip));
                    s.addProperty("last", last != null && last.address().equalsIgnoreCase(d.ip));
                    byte[] icon = d.getIconBytes();
                    if (icon != null) {
                        String key = "server:" + d.ip + ":" + java.util.Arrays.hashCode(icon);
                        if (!cursed.recode.render.Native.menusPicHas(key)) {
                            try (NativeImage img = NativeImage.read(icon)) {
                                Pics.put(key, img);
                            } catch (Exception ignored) {
                                key = null;
                            }
                        }
                        if (key != null) {
                            s.addProperty("icon", key);
                        }
                    }
                    if (e == list.getSelected()) {
                        sel = all.size();
                    }
                    all.add(e);
                    servers.add(s);
                } else if (e instanceof ServerSelectionList.NetworkServerEntry ne) {
                    JsonObject s = new JsonObject();
                    s.addProperty("i", all.size());
                    s.addProperty("motd", ne.serverData.getMotd());
                    s.addProperty("ip", ne.serverData.getAddress());
                    if (e == list.getSelected()) {
                        sel = all.size();
                    }
                    all.add(e);
                    lan.add(s);
                }
            }
        }
        this.numbered = all.toArray(new ServerSelectionList.Entry[0]);
        o.add("servers", servers);
        o.add("lan", lan);
        o.add("versions", VERSIONS);
        o.addProperty("sel", sel);
        o.addProperty("undo", this.removed != null);
        if (this.removed != null) {
            o.addProperty("removed", this.removed.name);
        }
        return o;
    }

    private ServerSelectionList.@Nullable Entry at(String n) {
        try {
            int i = Integer.parseInt(n.trim());
            return i >= 0 && i < this.numbered.length ? this.numbered[i] : null;
        } catch (NumberFormatException e) {
            return null;
        }
    }

    /** One of the game's screen's buttons, by its words' key. */
    private void press(String key) {
        for (Object r : this.screen.renderables) {
            if (r instanceof AbstractButton b && Widgets.key(b.getMessage()).equals(key) && b.active) {
                b.onPress(new net.minecraft.client.input.MouseButtonInfo(com.mojang.blaze3d.platform.InputConstants.MOUSE_BUTTON_LEFT, 0));
                return;
            }
        }
    }

    @Override
    public void act(String verb, String rest) {
        String[] a = rest.split(" ");
        var list = this.screen.serverSelectionList;
        switch (verb) {
            case "select" -> {
                var e = this.at(a[0]);
                if (e != null && list != null) {
                    list.setSelected(e);
                    this.screen.onSelectedChange();
                }
            }
            case "join" -> {
                var e = this.at(a[0]);
                if (e != null) {
                    e.join();
                }
            }
            case "move" -> {
                int from = Integer.parseInt(a[0]), to = Integer.parseInt(a[1]);
                var servers = this.screen.getServers();
                if (from >= 0 && to >= 0 && from < servers.size() && to < servers.size() && from != to) {
                    int step = to > from ? 1 : -1;
                    for (int i = from; i != to; i += step) {
                        servers.swap(i, i + step);
                    }
                    servers.save();
                    if (list != null) {
                        list.updateOnlineServers(servers);
                        list.setSelected(list.children().get(to));
                    }
                }
            }
            case "remove" -> {
                var e = this.at(a[0]);
                if (e instanceof ServerSelectionList.OnlineServerEntry se && list != null) {
                    var servers = this.screen.getServers();
                    this.removedAt = Integer.parseInt(a[0]);
                    this.removed = se.getServerData();
                    servers.remove(this.removed);
                    servers.save();
                    list.setSelected(null);
                    list.updateOnlineServers(servers);
                }
            }
            case "undo" -> {
                if (this.removed != null && list != null) {
                    var servers = this.screen.getServers();
                    servers.add(this.removed, false);
                    for (int i = servers.size() - 1; i > this.removedAt && i > 0; i--) {
                        servers.swap(i, i - 1);
                    }
                    servers.save();
                    this.removed = null;
                    list.updateOnlineServers(servers);
                }
            }
            case "direct" -> this.press("selectServer.direct");
            case "add" -> this.press("selectServer.add");
            case "edit" -> {
                var e = this.at(a[0]);
                if (e != null && list != null) {
                    list.setSelected(e);
                    this.screen.onSelectedChange();
                }
                this.press("selectServer.edit");
            }
            case "refresh" -> this.press("selectServer.refresh");
            case "via" -> {
                // a server's version chosen by hand (0: as it says)
                var e = this.at(a[0]);
                if (e instanceof ServerSelectionList.OnlineServerEntry se && a.length > 1) {
                    try {
                        cursed.recode.via.Choices.choose(se.getServerData().ip, Integer.parseInt(a[1]));
                    } catch (NumberFormatException ignored) {
                    }
                }
            }
            case "forget" -> this.removed = null;
            default -> CursedRecode.LOGGER.warn("[menus] servers were asked «{} {}»", verb, rest);
        }
    }

    @SuppressWarnings("unused")
    private static FormattedText unused() {
        return FormattedText.EMPTY;
    }
}
