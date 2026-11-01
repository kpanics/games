package cursed.recode.menus;

import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import cursed.recode.License;
import cursed.recode.LicenseScreen;

/**
 * The license's screen (cursed.recode.LicenseScreen) as the loader's page (menus/loader.rs), the one the start's
 * refusal is said with: what it is about, the headline, the words, what may be pressed — the account's page on the
 * site, the way out (Esc too).
 */
final class LicensePage implements Page {
    private final JsonObject data = new JsonObject();

    LicensePage(LicenseScreen s) {
        for (String k : new String[] {"cap", "title", "text"}) {
            this.data.addProperty(k, s.stop.has(k) ? s.stop.get(k).getAsString() : "");
        }
        this.data.addProperty("title_face", "thin");
        JsonArray words = new JsonArray();
        if (s.stop.has("words")) {
            for (JsonElement w : s.stop.getAsJsonArray("words")) {
                JsonArray a = w.getAsJsonArray().deepCopy();
                // the way out is the page's way back: Esc takes it
                if (a.get(0).getAsString().equals("quit")) {
                    a.set(0, JsonParser.parseString("\"cancel\""));
                }
                words.add(a);
            }
        }
        this.data.add("words", words);
        this.data.addProperty("nick", "");
        this.data.addProperty("build", "Cursed Recode");
        this.data.add("news", new JsonArray());
        this.data.addProperty("sub", "");
    }

    @Override
    public JsonObject data() {
        return this.data;
    }

    @Override
    public void act(String verb, String rest) {
        switch (verb) {
            case "site" -> License.site();
            case "open-loader", "cancel" -> License.quit();
            default -> {
            }
        }
    }
}
