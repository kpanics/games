package cursed.recode.menus;

import com.google.gson.JsonArray;
import com.google.gson.JsonObject;
import cursed.recode.launch.LaunchScreen;
import cursed.recode.render.Native;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.List;
import net.minecraft.advancements.AdvancementType;
import net.minecraft.advancements.DisplayInfo;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.components.toasts.AdvancementToast;
import net.minecraft.client.gui.components.toasts.NowPlayingToast;
import net.minecraft.client.gui.components.toasts.RecipeToast;
import net.minecraft.client.gui.components.toasts.SystemToast;
import net.minecraft.client.gui.components.toasts.ToastManager;
import net.minecraft.client.gui.components.toasts.TutorialToast;
import net.minecraft.client.gui.screens.PauseScreen;
import net.minecraft.network.chat.Component;
import net.minecraft.util.FormattedCharSequence;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;

/**
 * The game's toasts (an advancement made, recipes unlocked, the game's notices, the tutorial's hints, the music now
 * playing) as the menus' page "toasts" over everything: glass at the top right, each with its icon (an item the game
 * draws; the tutorial's keys, mouse and people the page's own icons, not the game's pixels), what it is and what it says. The game keeps their timing and their places (its slots); a toast of
 * a kind the page does not know is drawn by the game as it is. Drawn last in the frame: the menus' frame is made here
 * when no screen, list or page made one this frame. Over them all, while Recode's subscription has ended (or the server
 * is gone), the time left before the game closes (cursed.recode.License): with the HUD hidden too.
 */
public final class Toasts {
    private static int id;
    private static boolean open;
    private static long closedAt;
    private static String sent = "";
    private static final List<ItemStack> ICONS = new ArrayList<>();
    private static final float[] SAID = new float[64];
    /** The colour the game gives a tutorial toast's title (TutorialToast's constructor). */
    private static final int TUTORIAL_TITLE = -11534256;
    /** A menus frame was made this frame (a screen's page, the player list, a page sinking): the toasts are in it. */
    static boolean framed;

    /**
     * The main menu made this frame's menus frame (LaunchScreen): the toasts are in it. A frame of their own over it
     * would be laid out with no screen's numbers, the menu's words not risen in it — the menu gone while a toast or the
     * license's notice is up.
     */
    public static void framedByMenu() {
        framed = true;
    }
    /** "Saving world": how much it shows (the game's own fade), said by the HUD's mixin. */
    public static float saving;

    private Toasts() {
    }

    private static String text(List<FormattedCharSequence> lines, int from, int to) {
        StringBuilder sb = new StringBuilder();
        for (int i = from; i < Math.min(to, lines.size()); i++) {
            if (!sb.isEmpty()) {
                sb.append(' ');
            }
            lines.get(i).accept((k, style, cp) -> {
                sb.appendCodePoint(cp);
                return true;
            });
        }
        return sb.toString();
    }

    /**
     * How many of a tutorial toast's lines are its title: the game wraps its title and its words at 126 of its px into
     * one list, the title's in its colour (a long title is two lines: «Социальные / взаимодействия»).
     */
    private static int titleLines(List<FormattedCharSequence> lines) {
        int n = 0;
        while (n < lines.size()) {
            int[] rgb = {0};
            lines.get(n).accept((k, style, cp) -> {
                rgb[0] = style.getColor() == null ? 0 : style.getColor().getValue();
                return false;
            });
            if ((rgb[0] & 0xFFFFFF) != (TUTORIAL_TITLE & 0xFFFFFF)) {
                break;
            }
            n++;
        }
        return Math.max(1, n);
    }

    private static int icon(ItemStack stack) {
        ICONS.add(stack);
        return ICONS.size() - 1;
    }

    /** A toast the page draws, or null (the game draws it). */
    private static JsonObject toast(ToastManager.ToastInstance<?> t) {
        JsonObject j = new JsonObject();
        if (t.toast instanceof AdvancementToast a) {
            DisplayInfo d = a.advancement.value().display().orElse(null);
            if (d == null) {
                return null;
            }
            j.addProperty("title", d.type().getDisplayName().getString());
            j.addProperty("text", d.title().getString());
            j.addProperty("desc", d.description().getString());
            j.addProperty("icon", icon(a.iconItem));
            j.addProperty("hot", d.type() == AdvancementType.CHALLENGE);
            j.addProperty("adv", d.type().ordinal());
        } else if (t.toast instanceof RecipeToast r) {
            if (r.recipeItems.isEmpty()) {
                return null;
            }
            j.addProperty("title", Component.translatable("recipe.toast.title").getString());
            j.addProperty("text", Component.translatable("recipe.toast.description").getString());
            var e = (RecipeToast.Entry) r.recipeItems.get(Math.min(r.displayedRecipeIndex, r.recipeItems.size() - 1));
            j.addProperty("icon", icon(e.unlockedItem()));
            // the others unlocked with it: the page lays them out behind the first
            JsonArray more = new JsonArray();
            for (int i = 0; i < Math.min(4, r.recipeItems.size()); i++) {
                more.add(icon(((RecipeToast.Entry) r.recipeItems.get(i)).unlockedItem()));
            }
            j.add("icons", more);
            j.addProperty("count", r.recipeItems.size());
        } else if (t.toast instanceof SystemToast s) {
            j.addProperty("title", text(s.titleLines, 0, 99));
            j.addProperty("text", text(s.messageLines, 0, 99));
        } else if (t.toast instanceof TutorialToast tt) {
            int n = titleLines(tt.lines);
            j.addProperty("title", text(tt.lines, 0, n));
            j.addProperty("text", text(tt.lines, n, 99));
            ItemStack stack = switch (tt.icon) {
                case TREE -> new ItemStack(Items.OAK_LOG);
                case WOODEN_PLANKS -> new ItemStack(Items.OAK_PLANKS);
                case RECIPE_BOOK -> new ItemStack(Items.KNOWLEDGE_BOOK);
                default -> null;
            };
            if (stack != null) {
                j.addProperty("icon", icon(stack));
            } else {
                // the page's icons (gui.comp tutIcon; 35 the chat's people)
                j.addProperty("glyph", switch (tt.icon) {
                    case MOVEMENT_KEYS -> 46;
                    case MOUSE -> 47;
                    case RIGHT_CLICK -> 48;
                    default -> 35;
                });
            }
            if (tt.progressable) {
                j.addProperty("progress", tt.smoothedProgress);
            }
        } else {
            return null;
        }
        j.addProperty("slot", t.firstSlotIndex);
        j.addProperty("p", t.visiblePortion);
        j.addProperty("k", System.identityHashCode(t.toast));
        return j;
    }

    /** The game's turn to draw its toasts (ToastManager.extractRenderState, cancelled): see the class. */
    public static void extract(ToastManager manager, GuiGraphicsExtractor graphics) {
        Minecraft mc = Minecraft.getInstance();
        boolean frameMade = framed;
        framed = false;
        int license = cursed.recode.License.frame();
        if (cursed.recode.dev.Dev.on("CURSED_DEV_VANILLASCREENS")) {
            return;
        }
        ICONS.clear();
        JsonArray list = new JsonArray();
        if (!mc.gui.hud.isHidden()) {
            for (Object o : manager.visibleToasts) {
                var t = (ToastManager.ToastInstance<?>) o;
                if (t.hasFinishedRendering) {
                    continue;
                }
                JsonObject j = toast(t);
                if (j != null) {
                    list.add(j);
                } else {
                    t.extractRenderState(graphics, graphics.guiWidth());
                }
            }
            var np = manager.nowPlayingToast;
            if (np != null && mc.options.musicToast().get().renderToast() && !(mc.gui.screen() instanceof PauseScreen) && np.toast instanceof NowPlayingToast) {
                String key = mc.getMusicManager().getCurrentMusicTranslationKey();
                if (key != null && np.visiblePortion > 0.001F) {
                    JsonObject j = new JsonObject();
                    j.addProperty("music", Component.translatable(key.replace("/", ".")).getString());
                    j.addProperty("p", np.visiblePortion);
                    j.addProperty("k", key.hashCode());
                    list.add(j);
                }
            }
        }
        long now = System.nanoTime();
        float save = saving;
        saving = 0;
        if (!list.isEmpty() || save > 0.01F || license == 1) {
            JsonObject o = new JsonObject();
            o.add("toasts", list);
            if (license == 1) {
                o.addProperty("license", true);
            }
            // the main menu's account is top right: the toasts go under it
            if (mc.gui.screen() instanceof LaunchScreen) {
                o.addProperty("menu", true);
            }
            if (save > 0.01F) {
                o.addProperty("saving", save);
            }
            String s = o.toString();
            if (!open) {
                id = MenuHost.nextId();
                MenuHost.syncLanguage();
                Native.menusOpen(id, "toasts", s.getBytes(StandardCharsets.UTF_8));
                open = true;
            } else if (!s.equals(sent)) {
                Native.menusData(id, s.getBytes(StandardCharsets.UTF_8));
            }
            sent = s;
        } else if (open) {
            Native.menusClose(id);
            open = false;
            closedAt = now;
        }
        if (!open && now - closedAt > 400_000_000L) {
            return;
        }
        if (!frameMade) {
            int w = mc.getWindow().getWidth(), h = mc.getWindow().getHeight();
            int frame = Native.menusFrame(w, h, LaunchScreen.menusClock(), MenuHost.inp(Page.NONE), mc.level != null, SAID);
            MenuHost.poll(a -> { });
            Host.blit(graphics, frame, w, h);
        }
        // the toasts' items: real models over everything, turning (the number's fraction), fading for real
        Extras.eachRaw((kind, number, x, y, size, alpha) -> {
            int ref = (int) Math.floor(number);
            if (kind == 5 && ref >= 0 && ref < ICONS.size() && !ItemStage.OVER.add(graphics, ICONS.get(ref), x, y, size, (number - ref) * 360.0F, -12.0F, alpha)) {
                Extras.gameItem(graphics, ICONS.get(ref), x, y, size, alpha);
            }
        });
    }
}
