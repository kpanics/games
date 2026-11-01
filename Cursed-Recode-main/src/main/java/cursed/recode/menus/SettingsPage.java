package cursed.recode.menus;

import com.google.gson.JsonArray;
import com.google.gson.JsonObject;
import com.mojang.blaze3d.platform.InputConstants;
import cursed.recode.CursedRecode;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.function.Supplier;
import net.minecraft.client.KeyMapping;
import net.minecraft.client.Minecraft;
import net.minecraft.client.Options;
import net.minecraft.client.gui.components.AbstractButton;
import net.minecraft.client.gui.components.AbstractWidget;
import net.minecraft.client.gui.screens.CreditsAndAttributionScreen;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.gui.screens.options.AccessibilityOptionsScreen;
import net.minecraft.client.gui.screens.options.ChatOptionsScreen;
import net.minecraft.client.gui.screens.options.FontOptionsScreen;
import net.minecraft.client.gui.screens.options.LanguageSelectScreen;
import net.minecraft.client.gui.screens.options.MouseSettingsScreen;
import net.minecraft.client.gui.screens.options.OnlineOptionsScreen;
import net.minecraft.client.gui.screens.options.OptionsScreen;
import net.minecraft.client.gui.screens.options.OptionsSubScreen;
import net.minecraft.client.gui.screens.options.SkinCustomizationScreen;
import net.minecraft.client.gui.screens.options.SoundOptionsScreen;
import net.minecraft.client.gui.screens.options.VideoSettingsScreen;
import net.minecraft.client.gui.screens.options.controls.ControlsScreen;
import net.minecraft.client.gui.screens.options.controls.KeyBindsScreen;
import net.minecraft.client.gui.screens.packs.PackSelectionScreen;
import net.minecraft.client.gui.screens.telemetry.TelemetryInfoScreen;
import net.minecraft.client.input.KeyEvent;
import net.minecraft.client.resources.language.LanguageInfo;
import net.minecraft.client.resources.language.LanguageManager;
import net.minecraft.network.chat.Component;
import org.jspecify.annotations.Nullable;

/**
 * The game's settings as one page (the preview's): its sections on the left — video, audio, controls, key binds, skin,
 * chat, language, accessibility, online — the section's rows on the right, read from the game's own options screens
 * (made here, never shown: their options lists are the rows) and worked through their widgets. Whichever of the game's
 * options screens is up hosts it (opened from another, the page stays and turns to that section). Language changes at
 * once (only the game's words reload, not every resource); a key is bound by the next key or button pressed.
 */
final class SettingsPage implements Page {
    final Screen host;
    final Minecraft mc = Minecraft.getInstance();
    final Options options = this.mc.options;
    final Widgets widgets = new Widgets();
    private String section;
    private String query = "";
    /** The game's options screens made for the sections (their lists are the rows). */
    private final Map<String, OptionsSubScreen> subs = new LinkedHashMap<>();
    private final List<KeyMapping> keys = new ArrayList<>();
    private @Nullable KeyMapping listening;

    // the language is chosen in the client's menu (its card, every language with a search): not a section here
    static final String[] SECTIONS = {"video", "audio", "controls", "keys", "skin", "chat", "packs", "access", "online"};

    /** The section last looked at: the settings open on it again (from the game's main options screen). */
    private static String lastSection = "video";

    SettingsPage(Screen host) {
        this.host = host;
        this.section = host instanceof OptionsScreen ? lastSection : sectionOf(host);
    }

    /** The words each section is also found by (the search): its names in the four languages and what is in it. */
    private static String keywords(String id) {
        return switch (id) {
            case "video" -> "графика видео video graphics графіка відео графика бейне";
            case "audio" -> "звук громкость музыка sound audio volume music звук гучність музика дыбыс дауыс музыка";
            case "controls" -> "управление мышь чувствительность controls mouse sensitivity керування миша басқару тінтуір";
            case "keys" -> "клавиши бинды кнопки управление keys keybinds bindings клавіші кнопки пернелер батырмалар";
            case "skin" -> "скин одежда плащ skin cape скін одяг скин";
            case "chat" -> "чат сообщения chat messages чат повідомлення чат хабарламалар";
            case "packs" -> "наборы ресурсы текстуры паки packs resource textures набори ресурси текстури жинақтар";
            case "access" -> "доступность шрифт контраст accessibility font contrast доступність шрифт қолжетімділік қаріп";
            case "online" -> "сеть онлайн друзья серверы online network friends servers мережа друзі желі достар";
            default -> "";
        };
    }

    private static final String EN = "qwertyuiop[]asdfghjkl;'zxcvbnm,.`";
    private static final String RU = "йцукенгшщзхъфывапролджэячсмитьбюё";

    /** The query typed in the other layout (the keyboard left on English while typing Russian, and back). */
    private static String otherLayout(String s) {
        StringBuilder b = new StringBuilder(s.length());
        for (char c : s.toCharArray()) {
            int i = EN.indexOf(c), j = RU.indexOf(c);
            b.append(i >= 0 ? RU.charAt(i) : j >= 0 ? EN.charAt(j) : c);
        }
        return b.toString();
    }

    private static boolean found(String text, String q, String alt) {
        String t = text.toLowerCase(Locale.ROOT).replace('ё', 'е');
        return t.contains(q) || !alt.isEmpty() && t.contains(alt);
    }

    static String sectionOf(Screen s) {
        if (s instanceof SoundOptionsScreen) {
            return "audio";
        } else if (s instanceof ControlsScreen || s instanceof MouseSettingsScreen) {
            return "controls";
        } else if (s instanceof KeyBindsScreen) {
            return "keys";
        } else if (s instanceof SkinCustomizationScreen) {
            return "skin";
        } else if (s instanceof ChatOptionsScreen) {
            return "chat";
        } else if (s instanceof LanguageSelectScreen || s instanceof FontOptionsScreen) {
            return "access";
        } else if (s instanceof AccessibilityOptionsScreen) {
            return "access";
        } else if (s instanceof OnlineOptionsScreen) {
            return "online";
        }
        return "video";
    }

    /** Which of the sections' screens `s` is (null: none of them). */
    static @Nullable String subOf(Screen s) {
        if (s instanceof MouseSettingsScreen) {
            return "mouse";
        } else if (s instanceof FontOptionsScreen) {
            return "font";
        } else if (s instanceof VideoSettingsScreen) {
            return "video";
        } else if (s instanceof SoundOptionsScreen || s instanceof ControlsScreen || s instanceof SkinCustomizationScreen || s instanceof ChatOptionsScreen
                || s instanceof AccessibilityOptionsScreen || s instanceof OnlineOptionsScreen) {
            return sectionOf(s);
        }
        return null;
    }

    /** The game's name for a section (the page says its own in English and Russian). */
    private static Component name(String id) {
        return Component.translatable(switch (id) {
            case "audio" -> "options.sounds";
            case "controls" -> "options.controls";
            case "keys" -> "controls.keybinds";
            case "skin" -> "options.skinCustomisation";
            case "chat" -> "options.chat";
            case "language" -> "options.language";
            case "packs" -> "options.resourcepack";
            case "access" -> "options.accessibility";
            case "online" -> "options.online";
            default -> "options.video";
        });
    }

    /** The game's options screen for a section, made once (its widgets made by its init, never drawn). */
    private @Nullable OptionsSubScreen sub(String id) {
        OptionsSubScreen s = this.subs.get(id);
        if (s != null) {
            return s;
        }
        Supplier<OptionsSubScreen> make = switch (id) {
            case "video" -> () -> new VideoSettingsScreen(this.host, this.mc, this.options);
            case "audio" -> () -> new SoundOptionsScreen(this.host, this.options);
            case "controls" -> () -> new ControlsScreen(this.host, this.options);
            case "mouse" -> () -> new MouseSettingsScreen(this.host, this.options);
            case "skin" -> () -> new SkinCustomizationScreen(this.host, this.options);
            case "chat" -> () -> new ChatOptionsScreen(this.host, this.options);
            case "font" -> () -> new FontOptionsScreen(this.host, this.options);
            case "access" -> () -> new AccessibilityOptionsScreen(this.host, this.options);
            case "online" -> () -> new OnlineOptionsScreen(this.host, this.options);
            default -> null;
        };
        if (make == null) {
            return null;
        }
        // the screen up is the section's own: its list is the rows; else one is made (its init makes the widgets)
        s = this.host instanceof OptionsSubScreen hs && id.equals(subOf(this.host)) ? hs : make.get();
        if (s != this.host) {
            s.init(this.host.width, this.host.height);
        }
        this.subs.put(id, s);
        return s;
    }

    @Override
    public JsonObject data() {
        this.widgets.clear();
        JsonObject o = new JsonObject();
        o.addProperty("section", this.section);
        o.addProperty("query", this.query);
        o.addProperty("world", this.mc.level != null);
        JsonArray secs = new JsonArray();
        for (String id : SECTIONS) {
            JsonObject s = new JsonObject();
            s.addProperty("id", id);
            s.addProperty("name", name(id).getString().replace("...", "").replace("…", "").strip());
            secs.add(s);
        }
        o.add("sections", secs);
        JsonArray rows = new JsonArray();
        if (!this.query.isBlank()) {
            // every section's rows that answer the search: by their caption, what they are for, the header they are
            // under, or the section's own name (all of it then); typed in the other layout too. Each section's found
            // rows under its name.
            String q = this.query.strip().toLowerCase(Locale.ROOT).replace('ё', 'е');
            String alt = otherLayout(q);
            if (alt.equals(q)) {
                alt = "";
            }
            for (String id : SECTIONS) {
                JsonArray part = new JsonArray();
                this.rows(id, part);
                boolean whole = found(name(id).getString() + " " + keywords(id), q, alt);
                boolean underHit = false;
                JsonArray hits = new JsonArray();
                for (var e : part) {
                    JsonObject r = e.getAsJsonObject();
                    String t = r.get("t").getAsString();
                    if (t.equals("header")) {
                        underHit = found(r.get("s").getAsString(), q, alt);
                        continue;
                    }
                    // the languages whose name, region or code answer it
                    if (t.equals("langs")) {
                        JsonArray langs = new JsonArray();
                        for (var l : r.getAsJsonArray("list")) {
                            JsonObject lo = l.getAsJsonObject();
                            if (whole || found(lo.get("name").getAsString() + " " + lo.get("region").getAsString() + " " + lo.get("code").getAsString(), q, alt)) {
                                langs.add(lo);
                            }
                        }
                        if (!langs.isEmpty()) {
                            r.add("list", langs);
                            r.addProperty("sec", id);
                            hits.add(r);
                        }
                        continue;
                    }
                    String label = r.has("label") ? r.get("label").getAsString() : r.has("s") ? r.get("s").getAsString() : "";
                    String tip = r.has("tip") ? r.get("tip").getAsString() : "";
                    if (whole || underHit || found(label + " " + tip, q, alt)) {
                        r.addProperty("sec", id);
                        hits.add(r);
                    }
                }
                if (!hits.isEmpty()) {
                    JsonObject g = new JsonObject();
                    g.addProperty("t", "group");
                    g.addProperty("sec", id);
                    g.addProperty("s", name(id).getString().replace("...", "").replace("…", "").strip());
                    rows.add(g);
                    rows.addAll(hits);
                }
            }
        } else {
            this.rows(this.section, rows);
        }
        o.add("rows", rows);
        o.addProperty("listening", this.listening != null);
        return o;
    }

    /** A section's rows into `out`. */
    private void rows(String id, JsonArray out) {
        switch (id) {
            case "video" -> {
                JsonObject fov = this.widgets.describe(this.options.fov().createButton(this.options), this.options.fov(), 0);
                if (fov != null) {
                    out.add(fov);
                }
                this.list("video", out, false);
            }
            case "controls" -> {
                this.list("mouse", out, false);
                this.list("controls", out, true);
            }
            case "keys" -> this.keyRows(out);
            case "language" -> {
                this.languageRows(out);
                this.list("font", out, false);
            }
            case "packs" -> {
                JsonObject j = new JsonObject();
                j.addProperty("t", "link");
                j.addProperty("id", "packs");
                j.addProperty("s", Component.translatable("options.resourcepack").getString().replace("...", "").strip());
                out.add(j);
            }
            case "online" -> {
                this.list("online", out, false);
                for (String link : new String[] {"telemetry", "credits"}) {
                    JsonObject j = new JsonObject();
                    j.addProperty("t", "link");
                    j.addProperty("id", link);
                    j.addProperty("s", Component.translatable(link.equals("telemetry") ? "options.telemetry" : "options.credits_and_attribution").getString().replace("...", "").strip());
                    if (link.equals("telemetry") && !this.mc.allowsTelemetry()) {
                        j.addProperty("on", false);
                    }
                    out.add(j);
                }
            }
            case "access" -> {
                this.list("access", out, false);
                // the font's options (they were the language's section)
                this.list("font", out, false);
            }
            default -> this.list(id, out, false);
        }
    }

    /** A game's options screen's list as rows; `noLinks`: its buttons that open another screen left out. */
    private void list(String id, JsonArray out, boolean noLinks) {
        OptionsSubScreen s = this.sub(id);
        if (s == null || s.list == null) {
            return;
        }
        JsonArray part = new JsonArray();
        Mirror.rows(this.widgets, s.list, part, Integer.MAX_VALUE, "");
        for (var e : part) {
            JsonObject r = e.getAsJsonObject();
            if (noLinks && r.get("t").getAsString().equals("button") && !r.has("key")) {
                continue;
            }
            out.add(r);
        }
    }

    private void keyRows(JsonArray out) {
        this.keys.clear();
        KeyMapping[] all = this.options.keyMappings.clone();
        java.util.Arrays.sort(all);
        KeyMapping.Category last = null;
        for (KeyMapping k : all) {
            // the F3 keys cut are not offered either
            if (cursed.recode.hud.HudF3.cut(this.options, k)) {
                continue;
            }
            if (k.getCategory() != last) {
                last = k.getCategory();
                JsonObject h = new JsonObject();
                h.addProperty("t", "header");
                h.addProperty("s", last.label().getString());
                out.add(h);
            }
            JsonObject j = new JsonObject();
            j.addProperty("t", "keybind");
            j.addProperty("id", this.keys.size());
            j.addProperty("label", Component.translatable(k.getName()).getString());
            j.addProperty("value", k.getTranslatedKeyMessage().getString());
            j.addProperty("changed", !k.isDefault());
            j.addProperty("listen", k == this.listening);
            StringBuilder clash = new StringBuilder();
            if (!k.isUnbound()) {
                for (KeyMapping other : this.options.keyMappings) {
                    if (other != k && k.same(other)) {
                        if (!clash.isEmpty()) {
                            clash.append(", ");
                        }
                        clash.append(Component.translatable(other.getName()).getString());
                    }
                }
            }
            if (!clash.isEmpty()) {
                j.addProperty("clash", clash.toString());
            }
            this.keys.add(k);
            out.add(j);
        }
    }

    private void languageRows(JsonArray out) {
        LanguageManager lm = this.mc.getLanguageManager();
        String sel = lm.getSelected();
        JsonObject j = new JsonObject();
        j.addProperty("t", "langs");
        JsonArray list = new JsonArray();
        for (Map.Entry<String, LanguageInfo> e : lm.getLanguages().entrySet()) {
            JsonObject l = new JsonObject();
            l.addProperty("code", e.getKey());
            l.addProperty("name", e.getValue().name());
            l.addProperty("region", e.getValue().region());
            list.add(l);
        }
        j.add("list", list);
        j.addProperty("sel", sel);
        out.add(j);
    }

    /** The skin's model (the Skin section), made when first shown; when it was. */
    private net.minecraft.client.gui.components.PlayerSkinWidget skinModel;
    private long skinSince;

    /** The player in their skin where the page says (extras kind 7: the box's left, top and height), turning slowly,
     *  wearing the parts of it that are on. */
    @Override
    public void extras(net.minecraft.client.gui.GuiGraphicsExtractor graphics, float[] said) {
        Extras.each((kind, ref, x, y, size, alpha) -> {
            if (kind != 7 || alpha < 0.05F) {
                return;
            }
            double s = this.mc.getWindow().getGuiScale();
            // out of a world the engine's entity shaders have nothing to light with: the skin flat, from the front
            if (this.mc.level == null) {
                this.flatSkin(graphics, x, y, size, alpha);
                return;
            }
            if (this.skinModel == null) {
                var lookup = this.mc.player != null ? (java.util.function.Supplier<net.minecraft.world.entity.player.PlayerSkin>) this.mc.player::getSkin
                        : this.mc.getSkinManager().createLookup(this.mc.getGameProfile(), false);
                this.skinModel = new net.minecraft.client.gui.components.PlayerSkinWidget(10, 10, this.mc.getEntityModels(), lookup);
                this.skinSince = System.nanoTime();
            }
            var m = this.skinModel;
            int h = Math.max(8, Math.round(size / (float) s));
            m.setRectangle(Math.max(4, h / 2), h, Math.round(x / (float) s), Math.round(y / (float) s));
            // turning on its own, coming in from a quarter turn away as it shows
            float t = (System.nanoTime() - this.skinSince) / 1e9F;
            m.rotationY = 30.0F + t * 22.0F - (1.0F - alpha) * 90.0F;
            m.rotationX = -5.0F + (float) Math.sin(t * 0.6F) * 3.0F;
            for (var model : new net.minecraft.client.model.Model.Simple[] {m.wideModel, m.slimModel}) {
                var root = model.root();
                root.getChild("head").getChild("hat").visible = this.options.isModelPartEnabled(net.minecraft.world.entity.player.PlayerModelPart.HAT);
                root.getChild("body").getChild("jacket").visible = this.options.isModelPartEnabled(net.minecraft.world.entity.player.PlayerModelPart.JACKET);
                root.getChild("left_arm").getChild("left_sleeve").visible = this.options.isModelPartEnabled(net.minecraft.world.entity.player.PlayerModelPart.LEFT_SLEEVE);
                root.getChild("right_arm").getChild("right_sleeve").visible = this.options.isModelPartEnabled(net.minecraft.world.entity.player.PlayerModelPart.RIGHT_SLEEVE);
                root.getChild("left_leg").getChild("left_pants").visible = this.options.isModelPartEnabled(net.minecraft.world.entity.player.PlayerModelPart.LEFT_PANTS_LEG);
                root.getChild("right_leg").getChild("right_pants").visible = this.options.isModelPartEnabled(net.minecraft.world.entity.player.PlayerModelPart.RIGHT_PANTS_LEG);
            }
            m.extractRenderState(graphics, -1, -1, 0.0F);
        });
    }

    /** The skin as a figure seen from the front (its texture's front faces, the parts that are on over them). */
    private void flatSkin(net.minecraft.client.gui.GuiGraphicsExtractor g, float x, float y, float size, float alpha) {
        var skin = this.mc.getSkinManager().createLookup(this.mc.getGameProfile(), false).get();
        var tex = skin.body().texturePath();
        boolean slim = skin.model() == net.minecraft.world.entity.player.PlayerModelType.SLIM;
        double gs = this.mc.getWindow().getGuiScale();
        // 32 skin pixels tall, 16 wide, in the box's middle; breathing a little
        float px = size * 0.92F / 32.0F;
        float t = (System.nanoTime() - this.skinSince) / 1e9F;
        float ox = x + size * 0.25F - 8.0F * px, oy = y + size * 0.04F + (float) Math.sin(t * 1.3F) * px * 0.25F;
        g.pose().pushMatrix();
        g.pose().translate(ox / (float) gs, oy / (float) gs);
        g.pose().scale(px / (float) gs, px / (float) gs);
        int a = net.minecraft.util.ARGB.white(alpha);
        int aw = slim ? 3 : 4;
        var o = this.options;
        // x, y, u, v, w, h; then its outer layer's u, v and the part that shows it
        Object[][] parts = {
            {4, 0, 8, 8, 8, 8, 40, 8, net.minecraft.world.entity.player.PlayerModelPart.HAT},
            {4, 8, 20, 20, 8, 12, 20, 36, net.minecraft.world.entity.player.PlayerModelPart.JACKET},
            {4 - aw, 8, 44, 20, aw, 12, 44, 36, net.minecraft.world.entity.player.PlayerModelPart.RIGHT_SLEEVE},
            {12, 8, 36, 52, aw, 12, 52, 52, net.minecraft.world.entity.player.PlayerModelPart.LEFT_SLEEVE},
            {4, 20, 4, 20, 4, 12, 4, 36, net.minecraft.world.entity.player.PlayerModelPart.RIGHT_PANTS_LEG},
            {8, 20, 20, 52, 4, 12, 4, 52, net.minecraft.world.entity.player.PlayerModelPart.LEFT_PANTS_LEG},
        };
        for (Object[] q : parts) {
            int qx = (Integer) q[0], qy = (Integer) q[1], u = (Integer) q[2], v = (Integer) q[3], w = (Integer) q[4], h = (Integer) q[5];
            g.blit(net.minecraft.client.renderer.RenderPipelines.GUI_TEXTURED, tex, qx, qy, u, v, w, h, 64, 64, a);
            if (o.isModelPartEnabled((net.minecraft.world.entity.player.PlayerModelPart) q[8])) {
                g.blit(net.minecraft.client.renderer.RenderPipelines.GUI_TEXTURED, tex, qx, qy, (Integer) q[6], (Integer) q[7], w, h, 64, 64, a);
            }
        }
        g.pose().popMatrix();
    }

    @Override
    public float[] inp() {
        return NONE;
    }

    @Override
    public void act(String verb, String rest) {
        switch (verb) {
            case "section" -> {
                this.query = "";
                this.listening = null;
                this.section = rest.strip();
                lastSection = this.section;
            }
            case "sectionreset" -> this.resetSection(this.section);
            case "search" -> this.query = rest;
            case "reset" -> {
                var o = this.widgets.optionAt(rest);
                if (o != null) {
                    for (OptionsSubScreen s : this.subs.values()) {
                        s.resetOption(o);
                    }
                    if (o == this.options.fov()) {
                        this.reset(this.options.fov());
                    }
                }
            }
            case "listen" -> {
                int i = Integer.parseInt(rest.strip());
                this.listening = i >= 0 && i < this.keys.size() ? this.keys.get(i) : null;
            }
            case "keyreset" -> {
                int i = Integer.parseInt(rest.strip());
                if (i >= 0 && i < this.keys.size()) {
                    this.keys.get(i).setKey(this.keys.get(i).getDefaultKey());
                    KeyMapping.resetMapping();
                    this.options.save();
                }
            }
            case "keysreset" -> {
                for (KeyMapping k : this.options.keyMappings) {
                    k.setKey(k.getDefaultKey());
                }
                KeyMapping.resetMapping();
                this.options.save();
            }
            case "lang" -> language(rest.strip());
            case "link" -> {
                switch (rest.strip()) {
                    case "packs" -> {
                        Screen back = this.host;
                        this.mc.gui.setScreen(new PackSelectionScreen(this.mc.getResourcePackRepository(), repo -> {
                            this.options.updateResourcePacks(repo);
                            this.mc.gui.setScreen(back);
                        }, this.mc.getResourcePackDirectory(), Component.translatable("resourcePack.title")));
                    }
                    case "telemetry" -> this.mc.gui.setScreen(new TelemetryInfoScreen(this.host, this.options));
                    case "credits" -> this.mc.gui.setScreen(new CreditsAndAttributionScreen(this.host));
                    default -> {
                    }
                }
            }
            case "close" -> this.close();
            default -> {
                if (!this.widgets.act(verb, rest)) {
                    CursedRecode.LOGGER.warn("[menus] settings were asked «{} {}»", verb, rest);
                }
            }
        }
    }

    @SuppressWarnings("unchecked")
    private <T> void reset(net.minecraft.client.OptionInstance<T> o) {
        o.set((T) o.initialValue);
    }

    /** Every option of a section back to its default (the keys' own button does the keys). */
    private void resetSection(String id) {
        String[] parts = switch (id) {
            case "controls" -> new String[] {"mouse", "controls"};
            case "access" -> new String[] {"access", "font"};
            case "video", "audio", "skin", "chat", "online" -> new String[] {id};
            default -> new String[0];
        };
        for (String p : parts) {
            OptionsSubScreen s = this.sub(p);
            if (s == null || s.list == null) {
                continue;
            }
            for (Object entry : s.list.children()) {
                if (entry instanceof net.minecraft.client.gui.components.OptionsList.Entry oe) {
                    for (var ow : oe.children) {
                        if (ow.optionInstance() != null) {
                            s.resetOption(ow.optionInstance());
                        }
                    }
                }
            }
        }
        if (id.equals("video")) {
            this.reset(this.options.fov());
        }
        this.options.save();
    }

    /**
     * The game's language now, everywhere: only its words reload (the game reloads every resource for it; the menus'
     * words follow at once, crossing over).
     */
    public static void language(String code) {
        Minecraft mc = Minecraft.getInstance();
        LanguageManager lm = mc.getLanguageManager();
        if (code.isEmpty() || code.equals(lm.getSelected()) || lm.getLanguage(code) == null) {
            return;
        }
        lm.setSelected(code);
        mc.options.languageCode = code;
        lm.onResourceManagerReload(mc.getResourceManager());
        mc.options.save();
        MenuHost.syncLanguage();
    }

    /** Leaving the settings: what the sections changed is applied and saved, then the game's screen closes. */
    private void close() {
        this.gone();
        if (this.host instanceof OptionsScreen os) {
            this.mc.gui.setScreen(os.getLastScreen());
        } else {
            this.host.onClose();
        }
    }

    @Override
    public void gone() {
        for (OptionsSubScreen s : this.subs.values()) {
            if (s.list != null) {
                s.list.applyUnsavedChanges();
            }
            if (s != this.host) {
                s.removed();
            }
        }
        this.subs.clear();
        this.options.save();
    }

    /** A key while one is being bound: it is the binding (Esc unbinds). */
    @Override
    public boolean key(KeyEvent event) {
        if (this.listening == null) {
            return false;
        }
        this.listening.setKey(event.isEscape() ? InputConstants.UNKNOWN : InputConstants.getKey(event));
        this.bound();
        return true;
    }

    @Override
    public boolean mouse(int sdlButton) {
        if (this.listening == null) {
            return false;
        }
        this.listening.setKey(InputConstants.Type.MOUSE.getOrCreate(sdlButton));
        this.bound();
        return true;
    }

    private void bound() {
        this.listening = null;
        KeyMapping.resetMapping();
        this.options.save();
    }

    static boolean isLink(AbstractWidget w) {
        return w instanceof AbstractButton;
    }
}
