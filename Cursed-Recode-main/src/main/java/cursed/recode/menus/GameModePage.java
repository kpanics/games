package cursed.recode.menus;

import com.google.gson.JsonArray;
import com.google.gson.JsonObject;
import java.util.ArrayList;
import java.util.List;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.screens.debug.GameModeSwitcherScreen;
import net.minecraft.network.chat.Component;
import net.minecraft.world.item.ItemStack;

/**
 * The game mode switcher (F3+F4): the four modes as the game's icons in a row, the chosen one lit and named; F4 goes
 * to the next, letting go of F3 switches (the game's screen does both, its keys reach it); a click on a mode switches
 * to it at once.
 */
final class GameModePage implements Page {
    final GameModeSwitcherScreen screen;
    private final List<ItemStack> icons = new ArrayList<>();

    GameModePage(GameModeSwitcherScreen screen) {
        this.screen = screen;
    }

    @Override
    public JsonObject data() {
        this.icons.clear();
        JsonObject o = new JsonObject();
        JsonArray modes = new JsonArray();
        for (GameModeSwitcherScreen.GameModeIcon m : GameModeSwitcherScreen.GameModeIcon.VALUES) {
            modes.add(Widgets.text(m.name));
            this.icons.add(m.renderStack);
        }
        o.add("modes", modes);
        o.addProperty("chosen", this.screen.currentlyHovered.ordinal());
        var mc = Minecraft.getInstance();
        o.addProperty("now", mc.gameMode != null ? indexOf(mc.gameMode.getPlayerMode()) : -1);
        o.addProperty("hint", Component.translatable("debug.gamemodes.select_next", mc.options.keyDebugSwitchGameMode.getTranslatedKeyMessage()).getString());
        return o;
    }

    private static int indexOf(net.minecraft.world.level.GameType type) {
        var v = GameModeSwitcherScreen.GameModeIcon.VALUES;
        for (int i = 0; i < v.length; i++) {
            if (v[i].mode == type) {
                return i;
            }
        }
        return -1;
    }

    @Override
    public void act(String verb, String rest) {
        var v = GameModeSwitcherScreen.GameModeIcon.VALUES;
        int i = rest.isEmpty() ? -1 : Integer.parseInt(rest.trim());
        if (i < 0 || i >= v.length) {
            return;
        }
        switch (verb) {
            case "hover" -> this.screen.currentlyHovered = v[i];
            case "pick" -> {
                this.screen.currentlyHovered = v[i];
                this.screen.switchToHoveredGameMode();
                Minecraft.getInstance().gui.setScreen(null);
            }
            default -> { }
        }
    }

    /** Its keys are the game's screen's (F4 the next mode). */
    @Override
    public boolean key(net.minecraft.client.input.KeyEvent event) {
        return this.screen.keyPressed(event);
    }

    @Override
    public void extras(GuiGraphicsExtractor graphics, float[] said) {
        Extras.items(graphics, this.icons);
    }
}
