package cursed.recode.menus;

import com.google.gson.JsonArray;
import com.google.gson.JsonObject;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.screens.WinScreen;
import net.minecraft.network.chat.TextColor;
import net.minecraft.util.FormattedCharSequence;

/**
 * The credits (and the End's poem before them) as the menus' page: the game's lines, each as its runs of text with
 * their colour and whether they are scrambled (the player's name in the poem), which are centred; the page sets them
 * in the menus' faces. The scroll is the game's screen's (its keys reach it: Space faster, Ctrl faster still, Up
 * back; it ends the credits itself), stepped here each frame as the game would while drawing it.
 */
final class CreditsPage implements Page {
    final WinScreen screen;
    private JsonObject data;
    private long last;

    CreditsPage(WinScreen screen) {
        this.screen = screen;
    }

    @Override
    public JsonObject data() {
        if (this.data != null || this.screen.lines == null) {
            return this.data != null ? this.data : new JsonObject();
        }
        JsonObject o = new JsonObject();
        o.addProperty("poem", this.screen.poem);
        JsonArray lines = new JsonArray();
        for (int i = 0; i < this.screen.lines.size(); i++) {
            JsonObject l = new JsonObject();
            l.add("runs", runs(this.screen.lines.get(i)));
            if (this.screen.centeredLines.contains(i)) {
                l.addProperty("c", true);
            }
            lines.add(l);
        }
        o.add("lines", lines);
        this.data = o;
        return o;
    }

    /** A line's runs: [text, colour (rgb, -1 none), scrambled]. */
    private static JsonArray runs(FormattedCharSequence seq) {
        JsonArray out = new JsonArray();
        StringBuilder sb = new StringBuilder();
        int[] cur = {-2, 0};
        seq.accept((i, style, cp) -> {
            TextColor tc = style.getColor();
            int col = tc == null ? -1 : tc.getValue();
            int obf = style.isObfuscated() ? 1 : 0;
            if ((col != cur[0] || obf != cur[1]) && !sb.isEmpty()) {
                JsonArray r = new JsonArray();
                r.add(sb.toString());
                r.add(cur[0]);
                r.add(cur[1] == 1);
                out.add(r);
                sb.setLength(0);
            }
            cur[0] = col;
            cur[1] = obf;
            sb.appendCodePoint(cp);
            return true;
        });
        if (!sb.isEmpty()) {
            JsonArray r = new JsonArray();
            r.add(sb.toString());
            r.add(cur[0]);
            r.add(cur[1] == 1);
            out.add(r);
        }
        return out;
    }

    /** Each frame: the scroll stepped by the game's ticks since the last frame (as its own drawing does), then said. */
    @Override
    public float[] inp() {
        long now = System.nanoTime();
        float ticks = this.last == 0 ? 0 : Math.min(0.25F, (now - this.last) / 1e9F) * 20.0F;
        this.last = now;
        this.screen.scroll = Math.max(0.0F, this.screen.scroll + ticks * this.screen.scrollSpeed);
        return new float[] {this.screen.scroll, (float) Minecraft.getInstance().getWindow().getGuiScale()};
    }

    @Override
    public void act(String verb, String rest) {
    }

    /** Its keys are the game's screen's (faster, back, Esc to leave). */
    @Override
    public boolean key(net.minecraft.client.input.KeyEvent event) {
        return this.screen.keyPressed(event);
    }
}
