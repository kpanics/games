package cursed.recode.menus;

import com.google.gson.JsonArray;
import com.google.gson.JsonObject;
import com.mojang.blaze3d.platform.InputConstants;
import java.util.ArrayList;
import java.util.IdentityHashMap;
import java.util.List;
import java.util.Map;
import net.minecraft.client.gui.components.AbstractSelectionList;
import net.minecraft.client.gui.components.AbstractWidget;
import net.minecraft.client.gui.components.ContainerObjectSelectionList;
import net.minecraft.client.gui.components.ObjectSelectionList;
import net.minecraft.client.gui.components.OptionsList;
import net.minecraft.client.gui.components.events.GuiEventListener;
import net.minecraft.client.gui.screens.Screen;

/**
 * Any of the game's screens as the menus draw it when it has no page of its own: its widgets as the game made them
 * (in their order on the screen), each told by what it is — text, a button, a choice, a slider, a field, a list — and
 * worked through the game's own widget when the page asks.
 */
final class Mirror implements Page {
    final Screen screen;
    final Widgets widgets = new Widgets();

    Mirror(Screen screen) {
        this.screen = screen;
    }

    @Override
    public JsonObject data() {
        this.widgets.clear();
        JsonObject o = new JsonObject();
        String title = Widgets.text(this.screen.getTitle());
        o.addProperty("title", title);
        o.addProperty("cls", this.screen.getClass().getSimpleName());
        JsonArray items = new JsonArray();
        Map<Object, Boolean> seen = new IdentityHashMap<>();
        List<Object> all = new ArrayList<>(this.screen.renderables);
        for (GuiEventListener c : this.screen.children()) {
            if (!all.contains(c)) {
                all.add(c);
            }
        }
        int footY = this.screen.height - 36;
        for (Object e : all) {
            if (seen.put(e, true) == null) {
                rows(this.widgets, e, items, footY, title);
            }
        }
        o.add("items", inOrder(items));
        return o;
    }

    /**
     * The rows as they stand on the game's screen: top to bottom, then left to right (a row without a place — a header
     * — keeps the place of the one before it). The screens add their widgets in no particular order.
     */
    static JsonArray inOrder(JsonArray items) {
        List<int[]> keys = new ArrayList<>();
        int lastY = Integer.MIN_VALUE, lastX = 0;
        for (int i = 0; i < items.size(); i++) {
            JsonObject j = items.get(i).getAsJsonObject();
            if (j.has("y") && !j.has("row")) {
                lastY = j.get("y").getAsInt();
                lastX = j.has("x") ? j.get("x").getAsInt() : 0;
            }
            keys.add(new int[] {lastY, j.has("row") ? lastX : j.has("x") ? j.get("x").getAsInt() : lastX, i});
        }
        // rows of a list stay together and in order (their place is their list's)
        keys.sort((a, b) -> a[0] != b[0] ? Integer.compare(a[0], b[0]) : a[1] != b[1] ? Integer.compare(a[1], b[1]) : Integer.compare(a[2], b[2]));
        JsonArray out = new JsonArray();
        for (int[] k : keys) {
            out.add(items.get(k[2]));
        }
        return out;
    }

    /** `e`'s rows: a list's entries, an options list's widgets and headers, a widget; the screen's own title left out. */
    static void rows(Widgets widgets, Object e, JsonArray items, int footY, String title) {
        if (e instanceof AbstractSelectionList<?> list) {
            if (list instanceof ContainerObjectSelectionList<?>) {
                // rows of widgets (the options' lists): their widgets in order, headers as they come
                for (Object entry : list.children()) {
                    if (entry instanceof OptionsList.HeaderEntry h) {
                        JsonObject j = new JsonObject();
                        j.addProperty("t", "header");
                        j.addProperty("s", Widgets.text(h.widget.getMessage()));
                        items.add(j);
                    } else if (entry instanceof OptionsList.Entry oe) {
                        int row = items.size();
                        for (var ow : oe.children) {
                            // the vignette is never drawn (HudMixin): no option for it
                            if (ow.optionInstance() == net.minecraft.client.Minecraft.getInstance().options.vignette()) {
                                continue;
                            }
                            JsonObject j = widgets.describe(ow.widget(), ow.optionInstance(), footY);
                            if (j != null) {
                                j.addProperty("row", row);
                                items.add(j);
                            }
                        }
                    } else if (entry instanceof ContainerObjectSelectionList.Entry<?> ce) {
                        int row = items.size();
                        // a row of on/off buttons for one thing, only one of them on (the debug screen's never /
                        // in F3 / always): one choice of their words
                        List<net.minecraft.client.gui.components.CycleButton<?>> flags = new ArrayList<>();
                        for (GuiEventListener c : ce.children()) {
                            if (c instanceof net.minecraft.client.gui.components.CycleButton<?> cb && cb.getValue() instanceof Boolean && cb.visible) {
                                flags.add(cb);
                            }
                        }
                        if (flags.size() >= 2 && flags.size() == ce.children().size() && flags.stream().allMatch(f -> f.name.equals(flags.get(0).name))) {
                            JsonObject j = new JsonObject();
                            j.addProperty("t", "choose");
                            j.addProperty("label", Widgets.text(flags.get(0).name));
                            JsonArray words = new JsonArray(), ids = new JsonArray();
                            int sel = -1;
                            for (var f : flags) {
                                words.add(Widgets.text((net.minecraft.network.chat.Component) ((java.util.function.Function) f.valueStringifier).apply(Boolean.TRUE)));
                                ids.add(widgets.number(f, null));
                                if (Boolean.TRUE.equals(f.getValue())) {
                                    sel = words.size() - 1;
                                }
                            }
                            j.add("values", words);
                            j.add("ids", ids);
                            j.addProperty("index", sel);
                            j.addProperty("id", ids.get(0).getAsInt());
                            j.addProperty("y", flags.get(0).getY());
                            items.add(j);
                            continue;
                        }
                        // a row that is only a line of text: a heading of the list (the game rules' categories)
                        if (ce.children().size() == 1 && ce.children().get(0) instanceof net.minecraft.client.gui.components.AbstractStringWidget sw) {
                            String text = Widgets.text(sw.getMessage());
                            if (!text.isBlank() && text.length() < 48) {
                                JsonObject j = new JsonObject();
                                j.addProperty("t", "header");
                                j.addProperty("s", text);
                                items.add(j);
                                continue;
                            }
                        }
                        for (GuiEventListener c : ce.children()) {
                            if (c instanceof AbstractWidget w) {
                                JsonObject j = widgets.describe(w, null, footY);
                                if (j != null) {
                                    j.addProperty("row", row);
                                    items.add(j);
                                }
                            }
                        }
                    }
                }
                return;
            }
            JsonObject j = new JsonObject();
            j.addProperty("t", "list");
            j.addProperty("id", widgets.number(list, null));
            JsonArray rows = new JsonArray();
            Object sel = list.getSelected();
            int at = -1, n = 0;
            for (Object entry : list.children()) {
                rows.add(entry instanceof ObjectSelectionList.Entry<?> oe ? Widgets.text(oe.getNarration()) : entry.toString());
                if (entry == sel) {
                    at = n;
                }
                n++;
            }
            j.add("rows", rows);
            j.addProperty("sel", at);
            items.add(j);
            return;
        }
        // a box of widgets that scrolls (the game's newer screens): its widgets in order
        if (e instanceof net.minecraft.client.gui.components.AbstractContainerWidget box && widgets.describe(box, null, footY) == null) {
            for (GuiEventListener c : box.children()) {
                rows(widgets, c, items, footY, title);
            }
            return;
        }
        if (e instanceof AbstractWidget w) {
            JsonObject j = widgets.describe(w, null, footY);
            // the title as the game wrote it over the screen is the page's title already
            if (j != null && !(j.get("t").getAsString().equals("text") && j.get("s").getAsString().equals(title))) {
                items.add(j);
            }
        }
    }

    @Override
    public void act(String verb, String rest) {
        if (this.widgets.act(verb, rest)) {
            return;
        }
        if (verb.equals("key") && rest.equals("Return")) {
            this.screen.keyPressed(new net.minecraft.client.input.KeyEvent(InputConstants.KEY_RETURN, 0, 0));
            return;
        }
        cursed.recode.CursedRecode.LOGGER.warn("[menus] the mirror of {} was asked «{} {}»", this.screen.getClass().getSimpleName(), verb, rest);
    }
}
