package cursed.recode.menus;

import com.google.gson.JsonArray;
import com.google.gson.JsonObject;
import java.util.ArrayList;
import java.util.List;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.components.Button;
import net.minecraft.client.gui.screens.social.PlayerEntry;
import net.minecraft.client.gui.screens.social.SocialInteractionsScreen;
import net.minecraft.world.entity.player.PlayerSkin;
import org.jspecify.annotations.Nullable;

/**
 * Social interactions (the players one plays with: hide their chat, report them): the game's three tabs as words, its
 * search, each player a row — the face, the name, how it stands (hidden, blocked, gone), hide/show and report as words
 * (the game's own buttons); what the game says when a tab is empty.
 */
final class SocialPage implements Page {
    final SocialInteractionsScreen screen;
    final Widgets widgets = new Widgets();
    private final List<PlayerSkin> faces = new ArrayList<>();
    private boolean faked;

    SocialPage(SocialInteractionsScreen screen) {
        this.screen = screen;
    }

    private int button(@Nullable Button b) {
        return b != null && b.visible ? this.widgets.number(b, null) : -1;
    }

    @Override
    public JsonObject data() {
        this.widgets.clear();
        this.faces.clear();
        SocialInteractionsScreen s = this.screen;
        var list = s.socialInteractionsPlayerList;
        if (!this.faked && list != null && cursed.recode.dev.Dev.on("CURSED_DEV_SOCIALFAKE")) {
            // the stand plays alone: players to show
            this.faked = true;
            for (String n : new String[] {"Lunuca", "anoxis", "Steve_PvP", "Alex", "suicid"}) {
                var p = new net.minecraft.client.multiplayer.PlayerInfo(new com.mojang.authlib.GameProfile(java.util.UUID.nameUUIDFromBytes(n.getBytes()), n), false);
                list.addPlayer(p, SocialInteractionsScreen.Page.ALL);
            }
        }
        JsonObject o = new JsonObject();
        o.addProperty("title", Widgets.text(s.getTitle()));
        o.addProperty("server", s.serverLabel == null ? "" : Widgets.text(s.serverLabel));
        JsonArray tabs = new JsonArray();
        for (Button b : new Button[] {s.allButton, s.hiddenButton, s.blockedButton}) {
            JsonArray t = new JsonArray();
            // the game underlines the chosen one; its words without
            t.add(Widgets.text(b.getMessage()));
            t.add(this.button(b));
            tabs.add(t);
        }
        o.add("tabs", tabs);
        o.addProperty("tab", s.page.ordinal());
        o.addProperty("searchId", this.widgets.number(s.searchBox, null));
        o.addProperty("search", s.searchBox.getValue());
        o.addProperty("hint", Widgets.text(s.searchBox.hint));
        JsonArray rows = new JsonArray();
        if (list != null) {
            for (PlayerEntry e : list.children()) {
                JsonObject r = new JsonObject();
                r.addProperty("name", e.getPlayerName());
                r.addProperty("status", Widgets.text(e.getStatusComponent()));
                r.addProperty("gone", e.isRemoved());
                r.addProperty("face", this.faces.size());
                this.faces.add(e.getSkinGetter().get());
                r.addProperty("hide", this.button(e.hideButton));
                r.addProperty("show", this.button(e.showButton));
                r.addProperty("report", this.button(e.reportButton));
                r.addProperty("reportOn", e.reportButton != null && e.reportButton.active);
                if (e.hideButton != null) {
                    r.addProperty("hideWord", Widgets.text(net.minecraft.network.chat.Component.translatable("gui.socialInteractions.hide")));
                }
                r.addProperty("showWord", Widgets.text(net.minecraft.network.chat.Component.translatable("gui.socialInteractions.show")));
                r.addProperty("reportWord", Widgets.text(net.minecraft.network.chat.Component.translatable("gui.socialInteractions.report")));
                rows.add(r);
            }
        }
        o.add("rows", rows);
        String empty = "";
        if (rows.isEmpty()) {
            String k = !s.searchBox.getValue().isEmpty() ? "gui.socialInteractions.search_empty"
                    : s.page == SocialInteractionsScreen.Page.HIDDEN ? "gui.socialInteractions.empty_hidden"
                    : s.page == SocialInteractionsScreen.Page.BLOCKED ? "gui.socialInteractions.empty_blocked" : "";
            empty = k.isEmpty() ? "" : net.minecraft.network.chat.Component.translatable(k).getString();
        }
        o.addProperty("empty", empty);
        o.addProperty("hintId", this.button(s.blockingHintButton));
        o.addProperty("hintWord", Widgets.text(s.blockingHintButton.getMessage()));
        return o;
    }

    @Override
    public void act(String verb, String rest) {
        this.widgets.act(verb, rest);
    }

    @Override
    public void extras(GuiGraphicsExtractor graphics, float[] said) {
        Extras.each((kind, ref, x, y, size, alpha) -> {
            if (kind == Extras.FACE && ref >= 0 && ref < this.faces.size()) {
                Extras.face(graphics, this.faces.get(ref), x, y, size, alpha);
            }
        });
    }
}
