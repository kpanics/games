package cursed.recode.menus;

import com.google.gson.JsonArray;
import com.google.gson.JsonObject;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.components.AbstractButton;
import net.minecraft.client.gui.components.AbstractWidget;
import net.minecraft.client.gui.screens.PauseScreen;
import net.minecraft.client.multiplayer.ServerData;

/**
 * The pause menu (Esc in a world) as the main menu's in the world: the world frosted behind, the words on the left
 * rising from their lines (back to the game the main one), where one plays above them, the account top right; the
 * game's small buttons (friends, reports, feedback) as small words under them. Each word is the game's own button.
 */
final class PausePage implements Page {
    final PauseScreen screen;
    final Widgets widgets = new Widgets();

    PausePage(PauseScreen screen) {
        this.screen = screen;
        // where she is now: the world's picture for Singleplayer
        if (screen.showsPauseMenu()) {
            WorldShot.ask();
        }
    }

    /** What a button of the pause menu is for, by its words' key. */
    private static String role(AbstractWidget w) {
        String k = Widgets.key(w.getMessage());
        return switch (k) {
            case "menu.returnToGame" -> "back";
            case "gui.advancements" -> "adv";
            case "gui.stats" -> "stats";
            case "menu.options" -> "options";
            case "options.worldOptions.button" -> "world";
            case "menu.disconnect", "menu.returnToMenu", "menu.disconnectLocal" -> "quit";
            case "menu.reportBugs", "menu.sendFeedback", "menu.playerReporting", "gui.friends.button", "menu.friends" -> "small";
            default -> w.getWidth() <= 24 ? "small" : "other";
        };
    }

    /** Alt Manager's card under the nickname: the accounts' faces. */
    @Override
    public void extras(net.minecraft.client.gui.GuiGraphicsExtractor graphics, float[] said) {
        Extras.altFaces(graphics);
    }

    @Override
    public JsonObject data() {
        this.widgets.clear();
        Minecraft mc = Minecraft.getInstance();
        JsonObject o = new JsonObject();
        o.addProperty("menu", this.screen.showsPauseMenu());
        o.addProperty("title", Widgets.text(this.screen.getTitle()));
        o.addProperty("nick", mc.getUser().getName());
        ServerData sd = mc.getCurrentServer();
        if (mc.getSingleplayerServer() != null && mc.getSingleplayerServer().getWorldData() != null) {
            o.addProperty("where", mc.getSingleplayerServer().getWorldData().getLevelName());
            o.addProperty("single", true);
        } else if (sd != null) {
            o.addProperty("where", sd.name.isBlank() ? sd.ip : sd.name);
            o.addProperty("address", sd.ip);
            if (mc.getConnection() != null) {
                o.addProperty("players", mc.getConnection().getOnlinePlayers().size());
            }
        }
        JsonArray items = new JsonArray();
        for (Object e : this.screen.renderables) {
            if (e instanceof AbstractButton b && b.visible) {
                JsonObject j = this.widgets.describe(b, null, Integer.MAX_VALUE);
                if (j != null) {
                    j.addProperty("role", role(b));
                    items.add(j);
                }
            }
        }
        o.add("items", items);
        return o;
    }

    @Override
    public void act(String verb, String rest) {
        this.widgets.act(verb, rest);
    }

}
