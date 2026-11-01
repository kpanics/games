package cursed.recode.menus;

import com.google.gson.JsonArray;
import com.google.gson.JsonObject;
import com.mojang.blaze3d.platform.InputConstants;
import java.util.ArrayList;
import java.util.IdentityHashMap;
import java.util.List;
import java.util.Map;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.ActiveTextCollector;
import net.minecraft.client.gui.TextAlignment;
import net.minecraft.client.gui.components.AbstractWidget;
import net.minecraft.client.gui.components.MultiLineLabel;
import net.minecraft.client.gui.components.events.GuiEventListener;
import net.minecraft.client.gui.screens.AlertScreen;
import net.minecraft.client.gui.screens.BackupConfirmScreen;
import net.minecraft.client.gui.screens.ConnectScreen;
import net.minecraft.client.gui.screens.DatapackLoadFailureScreen;
import net.minecraft.client.gui.screens.ErrorScreen;
import net.minecraft.client.gui.screens.GenericWaitingScreen;
import net.minecraft.client.gui.screens.LevelLoadingScreen;
import net.minecraft.client.gui.screens.ProgressScreen;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.gui.screens.options.UnsupportedGraphicsWarningScreen;
import net.minecraft.client.gui.screens.worldselection.OptimizeWorldScreen;
import net.minecraft.client.multiplayer.ServerData;
import net.minecraft.network.chat.Component;
import net.minecraft.server.level.progress.ChunkLoadStatusView;
import net.minecraft.util.FormattedCharSequence;
import net.minecraft.world.level.chunk.status.ChunkStatus;

/**
 * The game's screens that say something and wait — connecting, loading a world, saving, a message, a question with
 * its answers, an error — as one page: what it is about (a server, a world) small above, the title, the message, how
 * far it has got (a share, or only that it is under way), the screen's own buttons and checkboxes as words. Their text
 * is the game's (the fields the screen draws itself, else its text widgets).
 */
public final class NoticePage implements Page {
    /** The server the game is connecting to (cursed.recode.mixin.client.MenusConnectMixin). */
    public static @org.jspecify.annotations.Nullable ServerData connecting;
    final Screen screen;
    final Widgets widgets = new Widgets();

    NoticePage(Screen screen) {
        this.screen = screen;
    }

    /** Is `s` one of the screens this page draws. */
    static boolean is(Screen s) {
        return s instanceof net.minecraft.client.gui.screens.ConfirmScreen || s instanceof AlertScreen || s instanceof ConnectScreen
                || s instanceof net.minecraft.client.gui.screens.DisconnectedScreen || s instanceof net.minecraft.client.gui.screens.GenericMessageScreen
                || s instanceof GenericWaitingScreen || s instanceof ProgressScreen || s instanceof LevelLoadingScreen || s instanceof ErrorScreen
                || s instanceof DatapackLoadFailureScreen || s instanceof BackupConfirmScreen || s instanceof UnsupportedGraphicsWarningScreen
                || s instanceof OptimizeWorldScreen || s instanceof net.minecraft.client.gui.screens.OutOfMemoryScreen
                || s instanceof net.minecraft.client.gui.screens.FileFixerAbortedScreen || s instanceof net.minecraft.client.gui.screens.worldselection.FileFixerProgressScreen
                || s instanceof net.minecraft.client.gui.screens.NoticeWithLinkScreen || s instanceof net.minecraft.client.gui.screens.multiplayer.WarningScreen
                || s instanceof net.minecraft.client.gui.screens.RecoverWorldDataScreen || s instanceof net.minecraft.client.gui.screens.multiplayer.ServerReconfigScreen
                || s instanceof net.minecraft.client.gui.screens.dialog.WaitingForResponseScreen
                || s instanceof net.minecraft.client.gui.screens.worldselection.ConfirmExperimentalFeaturesScreen;
    }

    /** The lines a label of the game's would draw, as text (its wrapping undone: a paragraph). */
    static String lines(MultiLineLabel label) {
        StringBuilder sb = new StringBuilder();
        label.visitLines(TextAlignment.LEFT, 0, 0, 9, new ActiveTextCollector() {
            private ActiveTextCollector.Parameters p = new ActiveTextCollector.Parameters(new org.joml.Matrix3x2f());

            @Override
            public ActiveTextCollector.Parameters defaultParameters() {
                return this.p;
            }

            @Override
            public void defaultParameters(ActiveTextCollector.Parameters newParameters) {
                this.p = newParameters;
            }

            @Override
            public void accept(TextAlignment alignment, int anchorX, int y, ActiveTextCollector.Parameters parameters, FormattedCharSequence text) {
                if (!sb.isEmpty()) {
                    sb.append(' ');
                }
                text.accept((i, style, cp) -> {
                    sb.appendCodePoint(cp);
                    return true;
                });
            }

            @Override
            public void acceptScrolling(Component message, int centerX, int left, int right, int top, int bottom, ActiveTextCollector.Parameters parameters) {
                this.accept(TextAlignment.LEFT, left, top, parameters, message.getVisualOrderText());
            }
        });
        return sb.toString();
    }

    @Override
    public JsonObject data() {
        this.widgets.clear();
        Minecraft mc = Minecraft.getInstance();
        JsonObject o = new JsonObject();
        String title = Widgets.text(this.screen.getTitle());
        List<String> message = new ArrayList<>();
        // -1 nothing to show; 0..1 how far; -2 under way, no measure
        float progress = -1;
        String about = "";
        String stage = "";
        Screen s = this.screen;
        if (s instanceof ConnectScreen c) {
            ServerData sd = connecting != null ? connecting : mc.getCurrentServer();
            about = sd == null ? "" : sd.name.isBlank() || sd.name.equals(sd.ip) ? sd.ip : sd.name + "  ·  " + sd.ip;
            title = Widgets.text(c.status);
            progress = -2;
        } else if (s instanceof ProgressScreen p) {
            if (p.header != null) {
                title = Widgets.text(p.header);
            }
            if (p.stage != null) {
                stage = Widgets.text(p.stage);
            }
            progress = p.progress > 0 ? p.progress / 100.0F : -2;
        } else if (s instanceof LevelLoadingScreen l) {
            title = Component.translatable(l.reason == LevelLoadingScreen.Reason.OTHER ? "connect.joining" : "multiplayer.downloadingTerrain").getString();
            if (mc.getSingleplayerServer() != null && mc.getSingleplayerServer().getWorldData() != null) {
                about = mc.getSingleplayerServer().getWorldData().getLevelName();
            } else if (mc.getCurrentServer() != null) {
                ServerData sd = mc.getCurrentServer();
                about = sd.name.isBlank() ? sd.ip : sd.name;
            }
            o.addProperty("portal", l.reason.ordinal());
            progress = l.loadTracker.hasProgress() ? l.smoothedProgress : -2;
            // the chunks round where one will stand, by how far each has got (0..10, '.' none)
            ChunkLoadStatusView view = l.loadTracker.statusView();
            if (view != null) {
                int d = view.radius() * 2 + 1;
                StringBuilder g = new StringBuilder(d * d);
                for (int z = 0; z < d; z++) {
                    for (int x = 0; x < d; x++) {
                        ChunkStatus st = view.get(x, z);
                        g.append(st == null ? '.' : (char) ('a' + Math.min(25, st.getIndex())));
                    }
                }
                o.addProperty("grid", g.toString());
                o.addProperty("gridD", d);
            }
        } else if (s instanceof GenericWaitingScreen g) {
            message.add(Widgets.text(g.messageText));
            if (g.showLoadingDots) {
                progress = -2;
            }
        } else if (s instanceof net.minecraft.client.gui.screens.DisconnectedScreen && connecting != null && mc.level == null) {
            about = connecting.name.isBlank() || connecting.name.equals(connecting.ip) ? connecting.ip : connecting.name + "  ·  " + connecting.ip;
        } else if (s instanceof AlertScreen a) {
            message.add(Widgets.text(a.messageText));
        } else if (s instanceof ErrorScreen e) {
            message.add(Widgets.text(e.messageText));
        } else if (s instanceof DatapackLoadFailureScreen d) {
            message.add(lines(d.message));
        } else if (s instanceof BackupConfirmScreen b) {
            message.add(Widgets.text(b.description));
        } else if (s instanceof UnsupportedGraphicsWarningScreen u) {
            for (Component c : u.message) {
                message.add(Widgets.text(c));
            }
        } else if (s instanceof OptimizeWorldScreen ow) {
            stage = Widgets.text(ow.upgrader.getStatus());
            progress = ow.upgrader.getTotalChunks() > 0 ? ow.upgrader.getTotalProgress() : -2;
        } else if (s instanceof net.minecraft.client.gui.screens.worldselection.FileFixerProgressScreen
                || s instanceof net.minecraft.client.gui.screens.multiplayer.ServerReconfigScreen
                || s instanceof net.minecraft.client.gui.screens.dialog.WaitingForResponseScreen
                || s instanceof net.minecraft.client.gui.screens.GenericMessageScreen) {
            progress = -2;
        }
        // the screen's widgets: its text (a message the screen made a widget of), its buttons and checkboxes
        JsonArray items = new JsonArray();
        Map<Object, Boolean> seen = new IdentityHashMap<>();
        List<Object> all = new ArrayList<>(s.renderables);
        for (GuiEventListener c : s.children()) {
            if (!all.contains(c)) {
                all.add(c);
            }
        }
        for (Object e : all) {
            if (seen.put(e, true) != null || !(e instanceof AbstractWidget w)) {
                continue;
            }
            JsonObject j = this.widgets.describe(w, null, Integer.MAX_VALUE);
            if (j == null) {
                continue;
            }
            String t = j.get("t").getAsString();
            if (t.equals("text")) {
                String text = j.get("s").getAsString();
                // the title as the screen wrote it is the page's title already
                if (title.isEmpty()) {
                    title = text;
                } else if (!text.equals(title) && !message.contains(text)) {
                    message.add(text);
                }
                continue;
            }
            items.add(j);
        }
        // a server's link or a file's place the screen says: a word of its own (ConfirmLinkScreen)
        if (s instanceof net.minecraft.client.gui.screens.ConfirmLinkScreen cl && message.stream().noneMatch(m -> m.contains(cl.url.toString()))) {
            o.addProperty("link", cl.url.toString());
        }
        o.addProperty("title", title);
        JsonArray msg = new JsonArray();
        message.stream().filter(m -> !m.isBlank()).forEach(msg::add);
        o.add("message", msg);
        o.addProperty("progress", progress);
        o.addProperty("about", about);
        o.addProperty("stage", stage);
        o.addProperty("esc", s.shouldCloseOnEsc());
        o.add("items", items);
        // going into a world (v12): the page is only the world's name over the horizon's line (Transition)
        if ((mc.level == null || s instanceof LevelLoadingScreen) && loading(s)
                && (cursed.recode.launch.Transition.entering() || s instanceof LevelLoadingScreen && !cursed.recode.launch.Transition.played())) {
            o.add("enter", cursed.recode.launch.Transition.enterNotice(about.contains("  ·  ") ? about.substring(0, about.indexOf("  ·  ")) : about,
                    progress >= 0.0F ? progress : -1.0F, s instanceof ConnectScreen || mc.getSingleplayerServer() == null && s instanceof LevelLoadingScreen));
        }
        return o;
    }

    /** Is `s` a screen of going into a world (connecting, preparing, loading its land). */
    public static boolean loading(Screen s) {
        return s instanceof ConnectScreen || s instanceof ProgressScreen || s instanceof LevelLoadingScreen || s instanceof net.minecraft.client.gui.screens.GenericMessageScreen;
    }

    @Override
    public void act(String verb, String rest) {
        if (this.widgets.act(verb, rest)) {
            return;
        }
        if (verb.equals("key")) {
            int key = switch (rest) {
                case "Return" -> InputConstants.KEY_RETURN;
                case "Escape" -> InputConstants.KEY_ESCAPE;
                default -> -1;
            };
            if (key >= 0) {
                this.screen.keyPressed(new net.minecraft.client.input.KeyEvent(key, 0, 0));
            }
        }
    }
}
