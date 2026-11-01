package cursed.recode;

import com.google.gson.JsonObject;
import net.minecraft.network.chat.Component;

/**
 * What stops the game while it plays (the license: its end come, another game on the account, a ban), said as at its
 * start — the loader's page (menus: cursed.recode.menus.LicensePage), the same words (native license.rs, Stop). Esc
 * is its way out too (the page's), not a way back into the game.
 */
public final class LicenseScreen extends net.minecraft.client.gui.screens.Screen {
    /** What the screen says: cap, title, text, words [action, label, kind]. */
    public final JsonObject stop;

    LicenseScreen(JsonObject stop) {
        super(Component.literal(stop.has("title") ? stop.get("title").getAsString() : ""));
        this.stop = stop;
    }

    @Override
    public boolean shouldCloseOnEsc() {
        return false;
    }

    @Override
    public boolean isPauseScreen() {
        return false;
    }
}
