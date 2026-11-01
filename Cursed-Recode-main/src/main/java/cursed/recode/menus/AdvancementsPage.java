package cursed.recode.menus;

import com.google.gson.JsonArray;
import com.google.gson.JsonObject;
import java.io.IOException;
import java.io.Reader;
import java.io.Writer;
import java.nio.file.Files;
import java.nio.file.Path;
import java.time.Instant;
import java.time.ZoneId;
import java.time.format.DateTimeFormatter;
import java.time.format.FormatStyle;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.Properties;
import net.minecraft.advancements.AdvancementHolder;
import net.minecraft.advancements.AdvancementProgress;
import net.minecraft.advancements.CriterionProgress;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.screens.advancements.AdvancementTab;
import net.minecraft.client.gui.screens.advancements.AdvancementWidget;
import net.minecraft.client.gui.screens.advancements.AdvancementsScreen;
import net.minecraft.client.multiplayer.ServerData;
import net.minecraft.locale.Language;
import net.minecraft.resources.Identifier;
import net.minecraft.world.item.ItemStack;
import org.jspecify.annotations.Nullable;

/**
 * Advancements for the menus' page (menus/adv.rs, the constellations): the game's tabs (their name, icon, how many are
 * done), the chosen tab's tree as the game laid it out (each node's place, what it is, done or how far, when it was
 * made, its parent), and what was made since this world's advancements were last looked at (kept in
 * cursed-recode/advancements.properties). The game draws the items over the stars where the page says; an item that
 * turns is drawn narrower as it turns (extras kind 8: the number's fraction is how far round it is).
 */
final class AdvancementsPage implements Page {
    final AdvancementsScreen screen;
    private final List<ItemStack> items = new ArrayList<>();
    private final List<AdvancementTab> tabs = new ArrayList<>();
    private static final DateTimeFormatter DATE = DateTimeFormatter.ofLocalizedDateTime(FormatStyle.MEDIUM, FormatStyle.SHORT);
    private static final String VISITS = "advancements.properties";
    /** When this world's advancements were looked at before this time (epoch ms; 0 never). */
    private final long lastVisit;
    private final String world;

    AdvancementsPage(AdvancementsScreen screen) {
        this.screen = screen;
        this.world = worldKey();
        this.lastVisit = visits().containsKey(this.world) ? parse(visits().getProperty(this.world)) : 0L;
    }

    private static long parse(String s) {
        try {
            return Long.parseLong(s.trim());
        } catch (NumberFormatException e) {
            return 0L;
        }
    }

    /** This world's name for the visits: the singleplayer world's folder, else the server's address. */
    private static String worldKey() {
        Minecraft mc = Minecraft.getInstance();
        if (mc.getSingleplayerServer() != null && mc.getSingleplayerServer().getWorldData() != null) {
            return "world." + mc.getSingleplayerServer().getWorldData().getLevelName().replaceAll("[^\\p{L}\\p{N}_.-]", "_");
        }
        ServerData sd = mc.getCurrentServer();
        return sd != null ? "server." + sd.ip.replaceAll("[^\\p{L}\\p{N}_.-]", "_") : "world.unknown";
    }

    private static Path visitsFile() {
        return Minecraft.getInstance().gameDirectory.toPath().resolve("cursed-recode").resolve(VISITS);
    }

    private static Properties visits() {
        Properties p = new Properties();
        Path f = visitsFile();
        if (Files.isRegularFile(f)) {
            try (Reader r = Files.newBufferedReader(f)) {
                p.load(r);
            } catch (IOException e) {
                cursed.recode.CursedRecode.LOGGER.warn("[menus] the advancements' visits", e);
            }
        }
        return p;
    }

    /** When the last of an advancement's criteria was met (epoch ms), or -1. */
    private static long doneAt(@Nullable AdvancementProgress p) {
        if (p == null || !p.isDone()) {
            return -1L;
        }
        Instant last = null;
        for (String c : p.getCompletedCriteria()) {
            CriterionProgress cp = p.getCriterion(c);
            if (cp != null && cp.getObtained() != null && (last == null || cp.getObtained().isAfter(last))) {
                last = cp.getObtained();
            }
        }
        return last == null ? -1L : last.toEpochMilli();
    }

    @Override
    @SuppressWarnings("unchecked")
    public JsonObject data() {
        this.items.clear();
        this.tabs.clear();
        JsonObject o = new JsonObject();
        JsonArray tabs = new JsonArray();
        int sel = -1;
        for (AdvancementTab tab : ((Map<AdvancementHolder, AdvancementTab>) this.screen.tabs).values()) {
            JsonObject t = new JsonObject();
            t.addProperty("title", tab.getTitle().getString());
            t.addProperty("id", tab.getRootAdvancement().id().toString());
            t.addProperty("icon", this.items.size());
            this.items.add(tab.icon);
            int done = 0, all = 0, fresh = 0;
            for (AdvancementWidget w : ((Map<AdvancementHolder, AdvancementWidget>) tab.widgets).values()) {
                all++;
                if (w.progress != null && w.progress.isDone()) {
                    done++;
                    if (doneAt(w.progress) > this.lastVisit) {
                        fresh++;
                    }
                }
            }
            t.addProperty("done", done);
            t.addProperty("all", all);
            t.addProperty("new", this.lastVisit > 0 ? fresh : 0);
            if (tab == this.screen.selectedTab) {
                sel = this.tabs.size();
            }
            this.tabs.add(tab);
            tabs.add(t);
        }
        o.add("tabs", tabs);
        o.addProperty("sel", sel);
        JsonArray nodes = new JsonArray();
        if (this.screen.selectedTab != null) {
            Map<AdvancementHolder, Integer> index = new HashMap<>();
            var widgets = ((Map<AdvancementHolder, AdvancementWidget>) this.screen.selectedTab.widgets).values();
            for (AdvancementWidget w : widgets) {
                index.put(w.getAdvancement(), index.size());
            }
            for (AdvancementWidget w : widgets) {
                JsonObject n = new JsonObject();
                var d = w.getDisplay();
                n.addProperty("x", w.getX());
                n.addProperty("y", w.getY());
                n.addProperty("title", d.title().getString());
                n.addProperty("desc", d.description().getString());
                n.addProperty("type", d.type().ordinal());
                AdvancementProgress p = w.progress;
                n.addProperty("done", p != null && p.isDone());
                n.addProperty("pct", p != null ? p.getPercent() : 0.0F);
                if (p != null && p.getProgressText() != null) {
                    n.addProperty("prog", p.getProgressText().getString());
                }
                // what it asks, part by part, when it asks for all of many (the biomes to visit, every mob to hunt):
                // each part of the game's requirements is one of its criteria or another (most have one)
                var req = w.getAdvancement().value().requirements();
                if (p != null && req.size() > 1) {
                    JsonArray crit = new JsonArray();
                    int made = 0;
                    for (List<String> part : req.requirements()) {
                        boolean done = false;
                        for (String c : part) {
                            CriterionProgress cp = p.getCriterion(c);
                            done |= cp != null && cp.isDone();
                        }
                        JsonArray one = new JsonArray();
                        one.add(part.isEmpty() ? "?" : criterion(part.getFirst()));
                        one.add(done);
                        crit.add(one);
                        made += done ? 1 : 0;
                    }
                    n.add("crit", crit);
                    n.addProperty("cdone", made);
                    n.addProperty("call", crit.size());
                }
                long at = doneAt(p);
                if (at > 0) {
                    n.addProperty("date", DATE.format(Instant.ofEpochMilli(at).atZone(ZoneId.systemDefault())));
                    n.addProperty("new", this.lastVisit > 0 && at > this.lastVisit);
                }
                var parent = w.advancementNode.parent();
                n.addProperty("parent", parent != null && index.containsKey(parent.holder()) ? index.get(parent.holder()) : -1);
                n.addProperty("icon", this.items.size());
                this.items.add(w.icon);
                nodes.add(n);
            }
        }
        o.add("nodes", nodes);
        return o;
    }

    /**
     * A criterion's name as the game would say it: most are the id of a biome, a mob, an item, a block or an effect
     * (Adventuring Time's biomes, Monsters Hunted's mobs, A Balanced Diet's food); else its words.
     */
    private static String criterion(String c) {
        Identifier id = Identifier.tryParse(c);
        if (id == null) {
            return words(c);
        }
        Language lang = Language.getInstance();
        String path = id.getPath().replace('/', '.');
        for (String kind : new String[] {"biome", "entity", "item", "block", "effect", "enchantment", "trim_pattern", "trim_material"}) {
            String key = kind + "." + id.getNamespace() + "." + path;
            if (lang.has(key)) {
                return lang.getOrDefault(key);
            }
        }
        return words(id.getPath());
    }

    /** `snowy_taiga` as `Snowy taiga`. */
    private static String words(String s) {
        String t = s.substring(s.lastIndexOf('/') + 1).replace('_', ' ').trim();
        return t.isEmpty() ? s : Character.toUpperCase(t.charAt(0)) + t.substring(1);
    }

    @Override
    public void act(String verb, String rest) {
        if (verb.equals("tab")) {
            int i = Integer.parseInt(rest.strip());
            if (!this.tabs.isEmpty()) {
                i = Math.floorMod(i, this.tabs.size());
                this.screen.advancements.setSelectedTab(this.tabs.get(i).getRootAdvancement(), true);
            }
        }
    }

    /** The page is gone: this world's advancements were looked at now. */
    @Override
    public void gone() {
        Properties p = visits();
        p.setProperty(this.world, Long.toString(System.currentTimeMillis()));
        Path f = visitsFile();
        try {
            Files.createDirectories(f.getParent());
            try (Writer w = Files.newBufferedWriter(f)) {
                p.store(w, "Cursed Recode: when the advancements of each world were looked at last");
            }
        } catch (IOException e) {
            cursed.recode.CursedRecode.LOGGER.warn("[menus] the advancements' visits", e);
        }
    }

    /**
     * The items over the stars and the tabs, real models in the menus' item stage; kind 8 turns (the number's fraction
     * is how far round), tilted a little toward the viewer as an item dropped in the world is seen from above.
     */
    @Override
    public void extras(GuiGraphicsExtractor graphics, float[] said) {
        double s = Minecraft.getInstance().getWindow().getGuiScale();
        Extras.each((kind, ref, x, y, size, alpha) -> {
            if (ref < 0 || ref >= this.items.size()) {
                return;
            }
            if (kind == Extras.ITEM) {
                Extras.item(graphics, this.items.get(ref), x, y, size, alpha);
            }
        });
        // the turning ones: their numbers carry how far round (the fraction)
        Extras.eachRaw((kind, number, x, y, size, alpha) -> {
            if (kind != 8) {
                return;
            }
            int ref = (int) Math.floor(number);
            if (ref < 0 || ref >= this.items.size() || alpha <= 0.02F) {
                return;
            }
            float turn = number - ref;
            if (ItemStage.MENUS.add(graphics, this.items.get(ref), x, y, size, turn * 360.0F, -12.0F, alpha)) {
                return;
            }
            // without the stage: the width is how much of its face is seen
            float face = (float) Math.abs(Math.cos(turn * Math.PI * 2.0));
            float k = size / 16.0F / (float) s * Math.min(1.0F, 0.4F + 0.6F * alpha);
            float cx = (x + size / 2.0F) / (float) s, cy = (y + size / 2.0F) / (float) s;
            graphics.pose().pushMatrix();
            graphics.pose().translate(cx, cy);
            graphics.pose().scale(k * Math.max(0.08F, face), k);
            graphics.fakeItem(this.items.get(ref), -8, -8);
            graphics.pose().popMatrix();
        });
    }
}
