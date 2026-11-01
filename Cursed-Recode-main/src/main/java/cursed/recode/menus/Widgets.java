package cursed.recode.menus;

import com.google.gson.JsonArray;
import com.google.gson.JsonObject;
import com.mojang.blaze3d.platform.InputConstants;
import java.util.ArrayList;
import java.util.List;
import java.util.Objects;
import net.minecraft.client.Minecraft;
import net.minecraft.client.OptionInstance;
import net.minecraft.client.gui.components.AbstractButton;
import net.minecraft.client.gui.components.AbstractSelectionList;
import net.minecraft.client.gui.components.AbstractSliderButton;
import net.minecraft.client.gui.components.AbstractStringWidget;
import net.minecraft.client.gui.components.AbstractWidget;
import net.minecraft.client.gui.components.Checkbox;
import net.minecraft.client.gui.components.CycleButton;
import net.minecraft.client.gui.components.EditBox;
import net.minecraft.client.gui.components.LockIconButton;
import net.minecraft.client.gui.components.Tooltip;
import net.minecraft.client.gui.components.events.GuiEventListener;
import net.minecraft.client.gui.components.tabs.TabNavigationBar;
import net.minecraft.client.input.MouseButtonEvent;
import net.minecraft.client.input.MouseButtonInfo;
import net.minecraft.network.chat.CommonComponents;
import net.minecraft.network.chat.Component;
import net.minecraft.network.chat.contents.TranslatableContents;
import org.jspecify.annotations.Nullable;

/**
 * The game's widgets as the menus' pages see them: each numbered and told by what it is (text, a button, a choice of
 * values, a slider, a field, a lock), the option behind it when there is one (its caption's key, whether it is off its
 * default); and worked through the widget itself when the page asks.
 */
final class Widgets {
    /** The widgets as numbered in the last data (the page's numbers). */
    final List<Object> numbered = new ArrayList<>();
    /** The option behind each numbered widget, when there is one. */
    final List<@Nullable OptionInstance<?>> options = new ArrayList<>();
    /** The field a paste went into (its text goes back to the page once). */
    @Nullable EditBox pasted;

    void clear() {
        this.numbered.clear();
        this.options.clear();
    }

    static String text(@Nullable Component c) {
        return c == null ? "" : c.getString();
    }

    static String key(@Nullable Component c) {
        return c != null && c.getContents() instanceof TranslatableContents t ? t.getKey() : "";
    }

    static String tip(AbstractWidget w) {
        Tooltip t = w.tooltip != null ? w.tooltip.tooltip : null;
        return t != null ? text(t.message) : "";
    }

    /** "Caption: value" as the game writes an option on its button: the caption and the value apart. */
    static String[] split(String message) {
        int i = message.indexOf(": ");
        return i > 0 ? new String[] {message.substring(0, i), message.substring(i + 2)} : new String[] {message, ""};
    }

    int number(Object w, @Nullable OptionInstance<?> option) {
        this.numbered.add(w);
        this.options.add(option);
        return this.numbered.size() - 1;
    }

    /** A widget as the page's row (null: nothing to show); `footY` the screen's foot (its buttons go to the bottom). */
    @Nullable JsonObject describe(AbstractWidget w, @Nullable OptionInstance<?> option, int footY) {
        if (!w.visible) {
            return null;
        }
        JsonObject j = new JsonObject();
        String msg = text(w.getMessage());
        if (w instanceof TabNavigationBar bar) {
            j.addProperty("t", "tabs");
            JsonArray tabs = new JsonArray();
            for (GuiEventListener c : bar.children()) {
                if (c instanceof AbstractWidget tb) {
                    tabs.add(text(tb.getMessage()));
                }
            }
            j.add("tabs", tabs);
            j.addProperty("id", this.number(w, null));
        } else if (w instanceof CycleButton<?> cb) {
            j.addProperty("t", "cycle");
            j.addProperty("id", this.number(w, option));
            j.addProperty("label", text(cb.name));
            values(cb, j);
        } else if (w instanceof AbstractSliderButton s) {
            String[] p = split(msg);
            j.addProperty("t", "slider");
            j.addProperty("id", this.number(w, option));
            j.addProperty("label", option != null ? text(option.caption) : p[0]);
            j.addProperty("value", p[1].isEmpty() ? msg : p[1]);
            j.addProperty("v", s.value);
        } else if (w instanceof EditBox eb) {
            j.addProperty("t", "edit");
            j.addProperty("id", this.number(w, null));
            j.addProperty("value", eb.getValue());
            j.addProperty("hint", text(eb.hint));
            // what the field is for, as the game names it (screens draw it over the field themselves)
            String label = msg.strip();
            if (!label.isEmpty() && !label.equals(text(eb.hint).strip())) {
                j.addProperty("label", label);
            }
            j.addProperty("max", eb.maxLength);
            if (eb == this.pasted) {
                j.addProperty("pasted", true);
                this.pasted = null;
            }
        } else if (w instanceof Checkbox cb) {
            j.addProperty("t", "check");
            j.addProperty("id", this.number(w, option));
            j.addProperty("label", msg);
            j.addProperty("v", cb.selected());
        } else if (w instanceof LockIconButton lb) {
            j.addProperty("t", "lock");
            j.addProperty("id", this.number(w, null));
            j.addProperty("v", lb.isLocked());
        } else if (w instanceof AbstractStringWidget) {
            if (msg.isBlank()) {
                return null;
            }
            j.addProperty("t", "text");
            j.addProperty("s", msg);
        } else if (w instanceof AbstractButton) {
            j.addProperty("t", "button");
            j.addProperty("id", this.number(w, option));
            j.addProperty("s", msg.isBlank() ? tip(w) : msg);
            Component m = w.getMessage();
            j.addProperty("back", m.equals(CommonComponents.GUI_BACK) || m.equals(CommonComponents.GUI_CANCEL) || m.equals(CommonComponents.GUI_DONE) || m.equals(CommonComponents.GUI_NO));
        } else {
            return null;
        }
        j.addProperty("on", w.active);
        String tip = tip(w);
        if (!tip.isEmpty()) {
            j.addProperty("tip", tip);
        }
        j.addProperty("y", w.getY());
        j.addProperty("x", w.getX());
        if (w.getY() >= footY && footY > 60) {
            j.addProperty("foot", true);
        }
        if (option != null) {
            j.addProperty("key", key(option.caption));
            j.addProperty("changed", !Objects.equals(option.get(), option.initialValue));
            if (option.get() instanceof Number n) {
                j.addProperty("num", n.doubleValue());
            } else if (option.get() instanceof Boolean b) {
                j.addProperty("num", b ? 1 : 0);
            } else if (option.get() instanceof Enum<?> e) {
                j.addProperty("num", e.ordinal());
            }
        }
        return j;
    }

    @SuppressWarnings({"unchecked", "rawtypes"})
    private static void values(CycleButton cb, JsonObject j) {
        List<Object> list = cb.values.getSelectedList();
        JsonArray vs = new JsonArray();
        int at = -1;
        for (int i = 0; i < list.size(); i++) {
            vs.add(text((Component) cb.valueStringifier.apply(list.get(i))));
            if (Objects.equals(list.get(i), cb.getValue())) {
                at = i;
            }
        }
        j.add("values", vs);
        j.addProperty("index", at);
        j.addProperty("value", text((Component) cb.valueStringifier.apply(cb.getValue())));
    }

    @Nullable Object at(String n) {
        try {
            int i = Integer.parseInt(n.trim());
            return i >= 0 && i < this.numbered.size() ? this.numbered.get(i) : null;
        } catch (NumberFormatException e) {
            return null;
        }
    }

    @Nullable OptionInstance<?> optionAt(String n) {
        try {
            int i = Integer.parseInt(n.trim());
            return i >= 0 && i < this.options.size() ? this.options.get(i) : null;
        } catch (NumberFormatException e) {
            return null;
        }
    }

    static MouseButtonEvent centre(AbstractWidget w) {
        return new MouseButtonEvent(w.getX() + w.getWidth() / 2.0, w.getY() + w.getHeight() / 2.0, new MouseButtonInfo(InputConstants.MOUSE_BUTTON_LEFT, 0));
    }

    /** A widget worked as the page asks: true when the verb was one of the widgets'. */
    @SuppressWarnings({"unchecked", "rawtypes"})
    boolean act(String verb, String rest) {
        String[] a = rest.split(" ", 2);
        Object o = this.at(a[0]);
        switch (verb) {
            case "press" -> {
                if (o instanceof AbstractButton b && b.active) {
                    b.onPress(new MouseButtonInfo(InputConstants.MOUSE_BUTTON_LEFT, 0));
                } else if (o instanceof AbstractWidget w && w.active) {
                    w.mouseClicked(centre(w), false);
                    w.mouseReleased(centre(w));
                }
            }
            case "slide" -> {
                if (o instanceof AbstractSliderButton s && s.active) {
                    s.setValue(Double.parseDouble(a[1]));
                }
            }
            case "release" -> {
                if (o instanceof AbstractSliderButton s) {
                    s.onRelease(centre(s));
                }
            }
            case "cycle" -> {
                if (o instanceof CycleButton cb && cb.active) {
                    List<Object> list = cb.values.getSelectedList();
                    int to = Integer.parseInt(a[1]);
                    int from = list.indexOf(cb.getValue());
                    if (to >= 0 && to < list.size() && from >= 0 && to != from) {
                        cb.cycleValue(to - from);
                    }
                }
            }
            case "text" -> {
                if (o instanceof EditBox eb) {
                    eb.setValue(a.length > 1 ? a[1] : "");
                }
            }
            case "paste" -> {
                if (o instanceof EditBox eb) {
                    eb.insertText(Minecraft.getInstance().keyboardHandler.getClipboard());
                    this.pasted = eb;
                }
            }
            case "tab" -> {
                if (o instanceof TabNavigationBar bar) {
                    bar.selectTab(Integer.parseInt(a[1]), true);
                }
            }
            case "select", "open" -> {
                if (o instanceof AbstractSelectionList list) {
                    int i = Integer.parseInt(a[1]);
                    if (i >= 0 && i < list.children().size()) {
                        Object entry = list.children().get(i);
                        list.setSelected((AbstractSelectionList.Entry) entry);
                        if (verb.equals("open") && entry instanceof GuiEventListener g) {
                            g.mouseClicked(new MouseButtonEvent(0, 0, new MouseButtonInfo(InputConstants.MOUSE_BUTTON_LEFT, 0)), true);
                        }
                    }
                }
            }
            default -> {
                return false;
            }
        }
        return true;
    }
}
