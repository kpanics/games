package cursed.recode.menus;

import com.google.gson.JsonObject;
import net.minecraft.client.gui.GuiGraphicsExtractor;

/**
 * A game's screen as the menus' page sees it: its data for the page (JSON, sent when it changes), what the page says
 * each frame, what the page asks for done on the game's screen, what the game still draws over the page (items).
 */
public interface Page {
    float[] NONE = new float[0];

    JsonObject data();

    /** What the page is told each frame (menus/mod.rs inp). */
    default float[] inp() {
        return NONE;
    }

    /** What the page asked for: its verb and the rest of its line. */
    void act(String verb, String rest);

    /** Drawn by the game over the page, where it says (`said`: menus/mod.rs out). */
    default void extras(GuiGraphicsExtractor graphics, float[] said) {
    }

    /** The page's screen is gone. */
    default void gone() {
    }

    /** A key before the page hears it (true: taken here — a key being bound). */
    default boolean key(net.minecraft.client.input.KeyEvent event) {
        return false;
    }

    /** A mouse button (SDL's number) before the page hears it (true: taken here). */
    default boolean mouse(int sdlButton) {
        return false;
    }
}
