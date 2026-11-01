package cursed.recode.menus;

import com.google.gson.JsonArray;
import com.google.gson.JsonObject;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.List;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.screens.achievement.StatsScreen;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.network.chat.Component;
import net.minecraft.resources.Identifier;
import net.minecraft.stats.Stat;
import net.minecraft.stats.StatFormatter;
import net.minecraft.stats.StatType;
import net.minecraft.stats.Stats;
import net.minecraft.stats.StatsCounter;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.item.BlockItem;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;

/**
 * Statistics for the menus' page (menus/stats.rs), read from the player's counters once the server has sent them (the
 * game's screen asks for them). The four that matter most with what is behind them: the time played and how many of
 * the world's days that is, the way travelled and by what (on foot, running, swimming, by boat…), the mobs killed and
 * which most, the deaths and the time since the last; the rest of the general ones in groups (moving, fighting, time,
 * the world's things, the rest), each with its raw count for the bars; every item with any count (mined, crafted,
 * used, broken, picked up, dropped) and its model; every mob killed or killed by, drawn as itself (extras kind 9).
 */
final class StatsPage implements Page {
    final StatsScreen screen;
    private final List<ItemStack> items = new ArrayList<>();
    private final List<EntityType<?>> mobTypes = new ArrayList<>();

    StatsPage(StatsScreen screen) {
        this.screen = screen;
    }

    /** Which group a general one goes in, by its name. */
    private static String group(String path) {
        if (path.endsWith("_one_cm")) {
            return "move";
        }
        if (path.startsWith("damage_") || path.equals("mob_kills") || path.equals("player_kills") || path.equals("deaths") || path.startsWith("raid_") || path.equals("target_hit")) {
            return "fight";
        }
        if (path.equals("play_time") || path.equals("total_world_time") || path.startsWith("time_since") || path.equals("sneak_time")) {
            return "time";
        }
        if (path.startsWith("interact_with_") || path.startsWith("open_") || path.startsWith("inspect_") || path.contains("cauldron") || path.startsWith("clean_")
                || path.contains("noteblock") || path.equals("pot_flower") || path.equals("trigger_trapped_chest") || path.equals("play_record") || path.equals("bell_ring")
                || path.equals("enchant_item") || path.startsWith("sleep_in") || path.startsWith("talked_") || path.startsWith("traded_") || path.equals("eat_cake_slice")) {
            return "world";
        }
        return "other";
    }

    @Override
    public JsonObject data() {
        this.items.clear();
        this.mobTypes.clear();
        JsonObject o = new JsonObject();
        o.addProperty("loading", this.screen.isLoading);
        StatsCounter stats = this.screen.stats;
        // the general ones in their groups, the largest first in each
        JsonObject groups = new JsonObject();
        for (String g : new String[] {"move", "fight", "time", "world", "other"}) {
            groups.add(g, new JsonArray());
        }
        List<Object[]> all = new ArrayList<>();
        long cm = 0;
        List<Object[]> modes = new ArrayList<>();
        for (Stat<Identifier> s : Stats.CUSTOM) {
            String path = s.getValue().getPath();
            long v = stats.getValue(s);
            all.add(new Object[] {group(path), name(s), units(s.format((int) v)), v});
            if (path.endsWith("_one_cm") && !path.equals("fall_one_cm") && v > 0) {
                cm += v;
                modes.add(new Object[] {name(s), v});
            }
        }
        all.sort(Comparator.comparingLong((Object[] r) -> -(long) r[3]).thenComparing(r -> (String) r[1]));
        for (Object[] r : all) {
            JsonArray a = new JsonArray();
            a.add((String) r[1]);
            a.add((String) r[2]);
            a.add((long) r[3]);
            groups.getAsJsonArray((String) r[0]).add(a);
        }
        o.add("groups", groups);
        // the four large, and what is behind each
        JsonObject hero = new JsonObject();
        long play = stats.getValue(Stats.CUSTOM.get(Stats.PLAY_TIME));
        hero.add("time", entry(name(Stats.CUSTOM.get(Stats.PLAY_TIME)), units(StatFormatter.TIME.format((int) play)), play));
        JsonObject dist = entry("", units(StatFormatter.DISTANCE.format((int) Math.min(Integer.MAX_VALUE, cm))), cm);
        modes.sort(Comparator.comparingLong((Object[] r) -> -(long) r[1]));
        JsonArray ms = new JsonArray();
        for (Object[] r : modes) {
            JsonArray a = new JsonArray();
            a.add((String) r[0]);
            a.add((long) r[1]);
            ms.add(a);
        }
        dist.add("modes", ms);
        hero.add("dist", dist);
        long kills = stats.getValue(Stats.CUSTOM.get(Stats.MOB_KILLS));
        hero.add("kills", entry(name(Stats.CUSTOM.get(Stats.MOB_KILLS)), units(StatFormatter.DEFAULT.format((int) kills)), kills));
        long deaths = stats.getValue(Stats.CUSTOM.get(Stats.DEATHS));
        JsonObject d = entry(name(Stats.CUSTOM.get(Stats.DEATHS)), units(StatFormatter.DEFAULT.format((int) deaths)), deaths);
        Stat<Identifier> since = Stats.CUSTOM.get(Stats.TIME_SINCE_DEATH);
        d.addProperty("since", units(since.format(stats.getValue(since))));
        d.addProperty("sinceName", name(since));
        hero.add("deaths", d);
        o.add("hero", hero);
        // the items: their counts under the game's headings, the most counted first
        JsonArray heads = new JsonArray();
        for (StatType<?> t : new StatType<?>[] {Stats.BLOCK_MINED, Stats.ITEM_CRAFTED, Stats.ITEM_USED, Stats.ITEM_BROKEN, Stats.ITEM_PICKED_UP, Stats.ITEM_DROPPED}) {
            heads.add(t.getDisplayName().getString());
        }
        o.add("itemHeads", heads);
        List<JsonArray> itemRows = new ArrayList<>();
        for (Item item : BuiltInRegistries.ITEM) {
            int[] v = new int[6];
            boolean any = false;
            if (item instanceof BlockItem bi) {
                v[0] = stats.getValue(Stats.BLOCK_MINED, bi.getBlock());
            }
            v[1] = stats.getValue(Stats.ITEM_CRAFTED, item);
            v[2] = stats.getValue(Stats.ITEM_USED, item);
            v[3] = stats.getValue(Stats.ITEM_BROKEN, item);
            v[4] = stats.getValue(Stats.ITEM_PICKED_UP, item);
            v[5] = stats.getValue(Stats.ITEM_DROPPED, item);
            for (int x : v) {
                any |= x > 0;
            }
            if (!any) {
                continue;
            }
            JsonArray r = new JsonArray();
            ItemStack stack = new ItemStack(item);
            r.add(stack.getHoverName().getString());
            r.add(this.items.size());
            this.items.add(stack);
            for (int x : v) {
                r.add(x);
            }
            itemRows.add(r);
        }
        itemRows.sort(Comparator.comparingLong((JsonArray r) -> {
            long sum = 0;
            for (int i = 2; i < r.size(); i++) {
                sum += r.get(i).getAsLong();
            }
            return -sum;
        }));
        JsonArray items = new JsonArray();
        itemRows.forEach(items::add);
        o.add("items", items);
        // the mobs, drawn as themselves (kind 9 by their place here); the most killed first
        List<JsonArray> mobRows = new ArrayList<>();
        for (EntityType<?> type : BuiltInRegistries.ENTITY_TYPE) {
            int killed = stats.getValue(Stats.ENTITY_KILLED, type), by = stats.getValue(Stats.ENTITY_KILLED_BY, type);
            if (killed > 0 || by > 0) {
                JsonArray r = new JsonArray();
                r.add(type.getDescription().getString());
                r.add(killed);
                r.add(by);
                r.add(this.mobTypes.size());
                this.mobTypes.add(type);
                mobRows.add(r);
            }
        }
        mobRows.sort(Comparator.comparingInt((JsonArray r) -> -r.get(1).getAsInt()).thenComparingInt(r -> -r.get(2).getAsInt()));
        JsonArray mobs = new JsonArray();
        mobRows.forEach(mobs::add);
        o.add("mobs", mobs);
        JsonArray tabs = new JsonArray();
        tabs.add(Component.translatable("stat.generalButton").getString());
        tabs.add(Component.translatable("stat.itemsButton").getString());
        tabs.add(Component.translatable("stat.mobsButton").getString());
        o.add("tabs", tabs);
        return o;
    }

    private static JsonObject entry(String name, String text, long raw) {
        JsonObject e = new JsonObject();
        e.addProperty("name", name);
        e.addProperty("text", text);
        e.addProperty("raw", raw);
        return e;
    }

    private static final java.util.Map<String, String> RU_UNITS = java.util.Map.of("y", "г", "d", "дн", "h", "ч", "min", "мин", "s", "с", "km", "км", "m", "м", "cm", "см");

    /** The game writes its units in English whatever the language ("1.2 km", "3 h"): in Russian, Russian ones. */
    private static String units(String value) {
        String code = net.minecraft.client.Minecraft.getInstance().options.languageCode;
        if (!(code.startsWith("ru") || code.startsWith("uk") || code.startsWith("be"))) {
            return value;
        }
        int sp = value.lastIndexOf(' ');
        if (sp < 0) {
            return value;
        }
        String unit = RU_UNITS.get(value.substring(sp + 1));
        return unit == null ? value : value.substring(0, sp + 1) + unit;
    }

    private static String name(Stat<Identifier> s) {
        return Component.translatable("stat." + s.getValue().toString().replace(':', '.')).getString();
    }

    @Override
    public void act(String verb, String rest) {
    }

    @Override
    public void extras(GuiGraphicsExtractor graphics, float[] said) {
        Extras.items(graphics, this.items);
        Extras.turning(graphics, this.items);
        Extras.mobs(graphics, this.mobTypes);
    }
}
