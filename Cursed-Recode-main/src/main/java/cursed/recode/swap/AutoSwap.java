package cursed.recode.swap;

import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import cursed.recode.render.Native;
import java.util.ArrayDeque;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import net.minecraft.client.Minecraft;
import net.minecraft.client.player.LocalPlayer;
import net.minecraft.client.resources.sounds.SimpleSoundInstance;
import net.minecraft.sounds.SoundEvents;
import net.minecraft.world.entity.boss.enderdragon.EndCrystal;
import net.minecraft.world.entity.item.PrimedTnt;
import net.minecraft.world.entity.monster.Creeper;
import net.minecraft.world.item.ItemStack;
import org.jspecify.annotations.Nullable;

/**
 * Auto Swap (the menu's module "auto-swap"; its swaps are kept by the menu, native gui/swap.rs): each "when → what →
 * where" done. A key pressed puts on the next thing of the list after what is on (round; one missing is passed), a key
 * held the first there is and, let go, what was on before (where it went: Moves puts the old thing where the new one
 * came from). A condition: health low, falling, danger near put on the first there is while it holds (and what was on
 * back when it passes, if asked); worn out puts on a better one of the list; ran out (a totem gone, food eaten) the
 * first there is. "All armour": each piece worn past the number changed for the best spare one, a piece at a time.
 * Of a kind the best goes first (Things.worth), then one in the hotbar (fewer clicks), then the lowest slot. What is
 * asked waits its turn, one swap at a time, each at the ticks the menu plans for it (swaptime.rs: the module's timing
 * or the swap's own — the delays, how it clicks, how the player stands). On FunTime a thing from the hotbar goes by
 * hand, from above it only a totem or a sphere, after FunTime's pause after its key and after the swap before. Conditions
 * only where they may (the menu says: never a totem, never on FunTime or ReallyWorld); on ReallyWorld only from the
 * hotbar; on HolyWorld nothing (the menu hides the module there).
 */
public final class AutoSwap {
    private static final String MODULE = "auto-swap";
    /** CURSED_DEV_SWAP: each swap done, in the log. */
    private static final boolean LOG = cursed.recode.dev.Dev.on("CURSED_DEV_SWAP");
    /** The place "All armour" (gui/swap.rs PLACES). */
    static final int ALL = 5;
    /** The conditions (gui/swap.rs CONDS). */
    private static final int HEALTH = 0;
    private static final int WORN = 1;
    private static final int RAN_OUT = 2;
    private static final int FALLING = 3;
    private static final int DANGER = 4;

    private AutoSwap() {
    }

    /** A swap as the menu keeps it, and what it put on over what (to put back). */
    static final class Swap {
        boolean on;
        int place;
        List<Things.Pick> picks = new ArrayList<>();
        int trigger;
        int cond;
        float value;
        boolean back;
        /** Whether it may go by itself on this server (the menu's word). */
        boolean auto;
        /** On while held or while its condition holds: what was on before and the slot it went to. */
        @Nullable ItemStack before;
        int beforeAt = -1;
        /** Its condition holds now (what it put on stays till it passes). */
        boolean met;
        /** Something was on its place last tick, and it has been looked at once ("Ran out"). */
        boolean had;
        boolean seen;
        /** Not asked again before this tick (worn out: once asked, it waits for what it did). */
        long rest;
    }

    /** A wish (a key's, a condition's, a piece of "All armour"), and the tick it was asked at. */
    private record Job(int swap, String what, long at) {
    }

    private static Map<String, String> set = Map.of();
    private static long setAt;
    private static String json = "";
    private static List<Swap> swaps = List.of();
    private static long swapsAt;
    private static final ArrayDeque<Job> JOBS = new ArrayDeque<>();
    private static long tick;
    private static long lastDone = -1000;
    /** The things the HUD draws (the notes, the Swap Ring), by the number the menu was told; the wish shown waiting out
     *  FunTime's pause. */
    private static final ItemStack[] SHOWN = new ItemStack[32];
    private static int told;
    private static @Nullable Job waitShown;
    /** The first wish's plan (the menu's, swaptime.rs): the wish, what it was planned with (whether it goes by the
     *  inventory's clicks, from above the hotbar), the tick its first click goes at, the tick at whose end the keys are
     *  let go (-1 none), the gaps after its first and second click, whether three left clicks carry it, whether the
     *  inventory is told closed after, the keys let go (bits: walking, sprint, jump, sneak). */
    private static @Nullable Job planned;
    private static boolean plannedClicks;
    private static boolean plannedAbove;
    private static long due;
    private static long stopFrom = -1;
    private static final int[] GAPS = new int[2];
    private static boolean three;
    private static boolean close = true;
    private static int keys;
    /** A swap under way (its clicks begun, the rest at their ticks): its clicks, the next one and its tick, as its plan
     *  said (the gaps, the keys let go through it, the inventory closed after), what it puts on where, its line for
     *  the log. */
    private static @Nullable List<Moves.Click> clicks;
    private static int next;
    private static long nextAt;
    private static final int[] RUN_GAPS = new int[2];
    private static int runKeys;
    private static boolean runClose;
    private static long runStood;
    private static ItemStack runGoing = ItemStack.EMPTY;
    private static int runPlace;
    private static String runSaid = "";
    /** The keys let go the next tick (KeyboardInputStopMixin): Auto Swap's stop. */
    private static int letGo;
    /** A thing left carried by a swap's left clicks, to be put back (Moves.giveBack): the menu's slot it came from;
     *  -1 none. */
    private static int owed = -1;

    /** Each game tick (MinecraftMixin). */
    public static void tick(Minecraft mc) {
        tick++;
        letGo = 0;
        Servers.tick(mc);
        LocalPlayer p = mc.player;
        // (the keys' events always taken: one pressed while it was off is not done later)
        String events = Native.guiCall("swapev");
        if (p == null || mc.gameMode == null || mc.level == null) {
            JOBS.clear();
            clicks = null;
            planned = null;
            owed = -1;
            waiting(p, "");
            return;
        }
        poll();
        String server = Servers.now(mc);
        if (!"1".equals(set.get("on")) || Servers.HOLYWORLD.equals(server)) {
            JOBS.clear();
            flush(mc, p);
            planned = null;
            waiting(p, server);
            return;
        }
        if (events != null && !events.isEmpty()) {
            for (String line : events.split("\n")) {
                String[] w = line.trim().split(" ");
                if (w.length == 2) {
                    try {
                        JOBS.add(new Job(Integer.parseInt(w[1]), w[0], tick));
                    } catch (NumberFormatException ignored) {
                        // not ours
                    }
                }
            }
        }
        conditions(p, server);
        if (owed >= 0 && clicks == null && Moves.giveBack(mc, p, owed)) {
            owed = -1;
        }
        // a swap under way: its clicks whose ticks are here
        boolean clicked = step(mc, p);
        if (clicks == null) {
            Job j = JOBS.peek();
            if (clicked) {
                // one swap a tick: the next begun from the next tick
                after(mc, p, server);
            } else {
                // one with nothing to do takes no tick, the next is looked at at once
                while (j != null && ready(p, j, server)) {
                    JOBS.poll();
                    planned = null;
                    boolean did = run(mc, p, j, server);
                    j = JOBS.peek();
                    if (did) {
                        if (clicks == null) {
                            after(mc, p, server);
                        }
                        break;
                    }
                }
            }
        }
        // the keys let go the next tick: through a swap under way that stands, and from the tick its plan says before one
        Job head = JOBS.peek();
        if (clicks != null) {
            letGo = runKeys;
        } else if (head != null && head == planned && stopFrom >= 0 && tick >= stopFrom) {
            letGo = keys;
        }
        if (head == null && clicks == null) {
            planned = null;
        }
        waiting(p, server);
    }

    /** The keys let go now (bits: walking, sprint, jump, sneak; 0 none): Auto Swap's stop before a swap from the
     *  inventory, or while it clicks. */
    public static int stopped() {
        return letGo;
    }

    /**
     * Whether the first wish goes now: planned by the menu (swaptime.rs, from what it takes: by the inventory's clicks
     * or by hand, from above the hotbar or not; planned anew when that changed while it waits), and its tick come.
     */
    private static boolean ready(LocalPlayer p, Job j, String server) {
        int from = source(p, j, server);
        if (from < 0) {
            // nothing to do (or "All armour" to part into its pieces): at once, it takes no tick and no delay
            return true;
        }
        boolean byClicks = byClicks(p, j, from, server);
        boolean above = from >= 9;
        if (planned != j || byClicks != plannedClicks || above != plannedAbove) {
            planned = j;
            plannedClicks = byClicks;
            plannedAbove = above;
            String r = Native.guiCall("swaptime\t" + j.swap() + "\t" + (byClicks ? 1 : 0) + "\t" + (above ? 1 : 0) + "\t" + j.at() + "\t" + lastDone + "\t" + tick);
            try {
                String[] f = r.split("\t");
                due = Long.parseLong(f[0]);
                stopFrom = Long.parseLong(f[1]);
                GAPS[0] = Integer.parseInt(f[2]);
                GAPS[1] = Integer.parseInt(f[3]);
                three = "1".equals(f[4]);
                close = "1".equals(f[5]);
                keys = Integer.parseInt(f[6]);
            } catch (RuntimeException e) {
                // no plan from the menu: at once, one press, no stop
                due = tick;
                stopFrom = -1;
                GAPS[0] = 0;
                GAPS[1] = 0;
                three = false;
                close = true;
                keys = 0;
            }
        }
        return tick >= due;
    }

    /** A swap done this tick: the wishes after it with nothing to do go at once (they take no tick: a "free" with
     *  nothing to put back forgets, "All armour" parts into its pieces), the next that does is planned now — the
     *  player stands on for it, when it stands too (the pieces of "All armour" one after another, the keys not back
     *  between them). */
    private static void after(Minecraft mc, LocalPlayer p, String server) {
        Job j = JOBS.peek();
        while (j != null && ready(p, j, server) && source(p, j, server) < 0) {
            JOBS.poll();
            planned = null;
            if (run(mc, p, j, server)) {
                break;
            }
            j = JOBS.peek();
        }
    }

    /** Whether a wish taking from `from` goes by the inventory's clicks (the player stands for those, as the timing
     *  says): not one by hand from the hotbar, not one with nothing to take. */
    private static boolean byClicks(LocalPlayer p, Job j, int from, String server) {
        if (from < 0 || p.containerMenu != p.inventoryMenu || j.swap() < 0 || j.swap() >= swaps.size()) {
            return false;
        }
        Swap s = swaps.get(j.swap());
        int place = s.place != ALL ? s.place : j.what().startsWith("piece") ? j.what().charAt(5) - '0' : -1;
        return place >= 0 && Moves.byClicks(p, place, from, reach(server));
    }

    /** The swap under way: its clicks whose ticks are here (with no gap, the next at once); after the last, it is over.
     *  True when it clicked. */
    private static boolean step(Minecraft mc, LocalPlayer p) {
        boolean clicked = false;
        while (clicks != null && tick >= nextAt) {
            if (!Moves.click(mc, p, clicks.get(next), next)) {
                // the inventory not the player's now, or nothing carried any more: what could be done is
                end(mc, p, next > 0);
                break;
            }
            clicked = true;
            next++;
            // (left clicks are over when what they carried went where nothing was)
            if (next < clicks.size() && !Moves.settled(p, clicks.get(next - 1), next - 1)) {
                nextAt = tick + RUN_GAPS[Math.min(next - 1, 1)];
            } else {
                end(mc, p, true);
            }
        }
        return clicked;
    }

    /** The swap under way over: when it did something, the inventory told closed (as asked) and it said; a thing left
     *  carried (it was cut short) put back as soon as it can be. */
    private static void end(Minecraft mc, LocalPlayer p, boolean did) {
        List<Moves.Click> done = clicks;
        clicks = null;
        if (done != null && !p.inventoryMenu.getCarried().isEmpty()) {
            owed = done.get(0).slot();
            if (Moves.giveBack(mc, p, owed)) {
                owed = -1;
            }
        }
        if (!did || done == null) {
            return;
        }
        if (runClose) {
            Moves.close(mc, p);
        }
        lastDone = tick;
        if (LOG) {
            cursed.recode.CursedRecode.LOGGER.info("{}, {} clicks {} (last at tick {}), stood {} ticks, speed {}, keys {}", runSaid, done.size(), done, tick,
                    runStood, String.format(java.util.Locale.ROOT, "%.3f", p.getDeltaMovement().horizontalDistance()), p.getLastSentInput());
        }
        if (!runGoing.isEmpty()) {
            tell(mc, runGoing, runPlace);
        }
    }

    /** The swap under way done now, its gaps not waited (the module off): nothing is left carried. */
    private static void flush(Minecraft mc, LocalPlayer p) {
        RUN_GAPS[0] = 0;
        RUN_GAPS[1] = 0;
        nextAt = Long.MIN_VALUE;
        step(mc, p);
        clicks = null;
    }

    /** A wish waiting its plan's tick (FunTime's pause, the delays, the stand: 4 ticks or more) shown by the crosshair
     *  (the Swap Ring) with what it would put on; none waiting, nothing shown. */
    private static void waiting(@Nullable LocalPlayer p, String server) {
        Job head = JOBS.peek();
        Job w = p != null && head != null && head == planned && clicks == null && due - tick >= (head == waitShown ? 1 : 4) ? head : null;
        if (w == waitShown) {
            return;
        }
        waitShown = w;
        int from = w == null ? -1 : source(p, w, server);
        ItemStack st = from < 0 ? ItemStack.EMPTY : p.getInventory().getItem(from);
        if (st.isEmpty()) {
            Native.guiCall("swapwait\t-1");
            return;
        }
        Native.guiCall("swapwait\t" + remember(st) + "\t" + (due - tick) + "\t" + st.getHoverName().getString().replace('\t', ' '));
    }

    /** The slot a wish would take its thing from now (what goes back, for a key let go); -1 none, or a wish that only
     *  parts "All armour" into its pieces. */
    private static int source(LocalPlayer p, Job j, String server) {
        if (j.swap() < 0 || j.swap() >= swaps.size()) {
            return -1;
        }
        Swap s = swaps.get(j.swap());
        Moves.Reach reach = reach(server);
        if (s.place == ALL) {
            return j.what().startsWith("piece") ? piece(p, j.what().charAt(5) - '0', s.value, reach) : -1;
        }
        if (s.picks.isEmpty() || s.place < 0 || s.place >= Moves.SLOTS.length) {
            return -1;
        }
        ItemStack worn = Moves.worn(p, s.place);
        return switch (j.what()) {
            case "press" -> next(p, s, worn, reach);
            case "hold" -> first(p, s, worn, reach);
            case "when" -> switch (s.cond) {
                case WORN -> better(p, s, worn, reach);
                case RAN_OUT -> worn.isEmpty() ? first(p, s, worn, reach) : -1;
                default -> first(p, s, worn, reach);
            };
            case "free" -> s.before == null || !s.back ? -1 : back(p, s, reach);
            default -> -1;
        };
    }

    /** A thing kept for the HUD to draw; the number it is told by. */
    private static int remember(ItemStack st) {
        told = (told + 1) % 1_000_000;
        SHOWN[told % SHOWN.length] = st.copy();
        return told;
    }

    /** A thing the HUD draws by its number (HudLayer: icon kind 10). */
    public static ItemStack shown(int number) {
        ItemStack st = SHOWN[Math.floorMod(number, SHOWN.length)];
        return st == null ? ItemStack.EMPTY : st;
    }

    /** Where things are taken from: only the hotbar (ReallyWorld's rule, or the module's setting), FunTime's rule, or
     *  anywhere. */
    static Moves.Reach reach(String server) {
        if (Servers.REALLYWORLD.equals(server) || "1".equals(set.get("Only from the hotbar"))) {
            return Moves.Reach.HOTBAR;
        }
        return Servers.FUNTIME.equals(server) ? Moves.Reach.FUNTIME : Moves.Reach.ALL;
    }

    /** The swaps that go by themselves: each condition looked at, a wish when it starts (and when it passes). */
    private static void conditions(LocalPlayer p, String server) {
        if (Servers.FUNTIME.equals(server) || Servers.REALLYWORLD.equals(server)) {
            return;
        }
        for (int i = 0; i < swaps.size(); i++) {
            Swap s = swaps.get(i);
            if (!s.on || s.trigger != 2 || !s.auto || (s.picks.isEmpty() && s.place != ALL)) {
                continue;
            }
            switch (s.cond) {
                case HEALTH, FALLING, DANGER -> {
                    boolean holds = s.met ? !passed(p, s) : starts(p, s);
                    if (holds && !s.met) {
                        s.met = true;
                        JOBS.add(new Job(i, "when", tick));
                    } else if (!holds && s.met) {
                        s.met = false;
                        if (s.back) {
                            JOBS.add(new Job(i, "free", tick));
                        }
                    }
                }
                case WORN -> {
                    // "All armour": when a piece has a spare to change for (at 0, whenever a better one is there)
                    if (tick >= s.rest && (s.place == ALL ? anyPiece(p, s.value, reach(server)) : wornOut(p, s))) {
                        s.rest = tick + 20;
                        JOBS.add(new Job(i, "when", tick));
                    }
                }
                case RAN_OUT -> {
                    boolean has = s.place != ALL && !Moves.worn(p, s.place).isEmpty();
                    if (s.seen && s.had && !has) {
                        JOBS.add(new Job(i, "when", tick));
                    }
                    s.had = has;
                    s.seen = true;
                }
                default -> {
                }
            }
        }
    }

    /** Whether a held condition starts now: health at most the number (hearts), falling at least the number of blocks,
     *  danger within the number of blocks. */
    private static boolean starts(LocalPlayer p, Swap s) {
        return switch (s.cond) {
            case HEALTH -> p.getHealth() <= s.value * 2.0F;
            case FALLING -> !p.onGround() && p.fallDistance >= s.value && !p.isFallFlying() && !p.isInWater() && !p.getAbilities().flying && !p.isPassenger();
            case DANGER -> danger(p, s.value);
            default -> false;
        };
    }

    /** Whether a condition that held is past: a heart more, on the ground or in water, no danger a block further. */
    private static boolean passed(LocalPlayer p, Swap s) {
        return switch (s.cond) {
            case HEALTH -> p.getHealth() > s.value * 2.0F + 2.0F;
            case FALLING -> p.onGround() || p.isInWater() || p.isPassenger();
            case DANGER -> !danger(p, s.value + 1.0F);
            default -> true;
        };
    }

    /** An end crystal, a creeper or lit TNT within `r` blocks. */
    private static boolean danger(LocalPlayer p, float r) {
        return !p.level().getEntities(p, p.getBoundingBox().inflate(r), e -> (e instanceof EndCrystal || e instanceof Creeper || e instanceof PrimedTnt)
                && e.distanceToSqr(p) <= r * r).isEmpty();
    }

    /** Whether what is worn on its place is worn past the number (% left). */
    private static boolean wornOut(LocalPlayer p, Swap s) {
        return past(Moves.worn(p, s.place), s.value);
    }

    /** Whether a piece of armour would be changed now ("All armour" at `percent`). */
    private static boolean anyPiece(LocalPlayer p, float percent, Moves.Reach reach) {
        for (int k = 1; k <= 4; k++) {
            if (piece(p, k, percent, reach) >= 0) {
                return true;
            }
        }
        return false;
    }

    private static boolean past(ItemStack st, float percent) {
        return !st.isEmpty() && st.isDamageableItem() && Things.left(st) * 100.0 < percent;
    }

    /** The module's settings (four times a second) and its swaps (when the menu's line of them changed). */
    private static void poll() {
        long now = System.nanoTime();
        if (now - setAt > 250_000_000L) {
            setAt = now;
            Map<String, String> v = new HashMap<>();
            String s = Native.guiCall("set\t" + MODULE);
            if (s != null && !s.isEmpty()) {
                String[] f = s.split("\t");
                v.put("on", f[0]);
                for (int i = 1; i < f.length; i++) {
                    int eq = f[i].indexOf('=');
                    if (eq > 0) {
                        v.put(f[i].substring(0, eq), f[i].substring(eq + 1));
                    }
                }
            }
            set = v;
        }
        if (now - swapsAt > 250_000_000L) {
            swapsAt = now;
            String s = Native.guiCall("swaps");
            if (s != null && !s.equals(json)) {
                json = s;
                swaps = read(s);
                JOBS.clear();
            }
        }
    }

    static List<Swap> read(String s) {
        List<Swap> out = new ArrayList<>();
        try {
            JsonObject o = JsonParser.parseString(s).getAsJsonObject();
            JsonArray a = o.getAsJsonArray("swaps");
            for (JsonElement e : a) {
                JsonObject w = e.getAsJsonObject();
                Swap sw = new Swap();
                sw.on = w.get("on").getAsBoolean();
                sw.place = w.get("place").getAsInt();
                sw.trigger = w.get("trigger").getAsInt();
                sw.cond = w.get("cond").getAsInt();
                sw.value = w.get("value").getAsFloat();
                sw.back = w.get("back").getAsBoolean();
                sw.auto = w.has("auto") && w.get("auto").getAsBoolean();
                for (JsonElement pe : w.getAsJsonArray("picks")) {
                    JsonObject po = pe.getAsJsonObject();
                    sw.picks.add(po.has("kind") ? new Things.Pick(null, null, null, po.get("kind").getAsString())
                            : new Things.Pick(po.get("id").getAsString(), po.get("name").getAsString(), po.get("ench").getAsString(), null));
                }
                out.add(sw);
            }
        } catch (RuntimeException e) {
            cursed.recode.CursedRecode.LOGGER.warn("[auto swap] the swaps could not be read: {}", e.toString());
        }
        return out;
    }

    /** A wish done now; false when there was nothing to do (or it could not be done). */
    private static boolean run(Minecraft mc, LocalPlayer p, Job j, String server) {
        if (j.swap() < 0 || j.swap() >= swaps.size()) {
            return false;
        }
        Swap s = swaps.get(j.swap());
        if (!s.on) {
            return false;
        }
        Moves.Reach reach = reach(server);
        if (s.place == ALL) {
            if (j.what().startsWith("piece")) {
                int k = j.what().charAt(5) - '0';
                return done(mc, p, j, s, k, piece(p, k, s.value, reach), reach, server);
            }
            if (!"free".equals(j.what())) {
                // each piece a wish of its own, one after another (the delays between swaps, FunTime's pause between them;
                // the one after the key already waited)
                for (int k = 4; k >= 1; k--) {
                    JOBS.addFirst(new Job(j.swap(), "piece" + k, j.at()));
                }
            }
            return false;
        }
        if (s.picks.isEmpty() || !Moves.free(mc, p, s.place)) {
            return false;
        }
        ItemStack worn = Moves.worn(p, s.place).copy();
        int from;
        switch (j.what()) {
            case "press" -> from = next(p, s, worn, reach);
            case "hold" -> from = holdOn(p, s, worn, reach);
            case "when" -> from = switch (s.cond) {
                case WORN -> better(p, s, worn, reach);
                case RAN_OUT -> worn.isEmpty() ? first(p, s, worn, reach) : -1;
                default -> holdOn(p, s, worn, reach);
            };
            case "free" -> {
                if (s.before == null || !s.back) {
                    s.before = null;
                    return false;
                }
                from = back(p, s, reach);
                s.before = null;
            }
            default -> from = -1;
        }
        return done(mc, p, j, s, s.place, from, reach, server);
    }

    /** The thing in `from` put on `place`: by hand at once, or its clicks begun as its plan says (the first now, the
     *  rest at their ticks: step), said when done; false when there was none or it could not be. */
    private static boolean done(Minecraft mc, LocalPlayer p, Job j, Swap s, int place, int from, Moves.Reach reach, String server) {
        if (from < 0 || !Moves.free(mc, p, place)) {
            return false;
        }
        ItemStack worn = Moves.worn(p, place).copy();
        ItemStack going = p.getInventory().getItem(from).copy();
        List<Moves.Click> list = Moves.clicks(mc, p, place, from, reach, three);
        if (list == null) {
            return false;
        }
        String said = !LOG ? "" : String.format(java.util.Locale.ROOT, "[auto swap] tick %d (asked %d, planned %d): swap %d %s: slot %d %s → place %d (was %s), reach %s, server '%s'",
                tick, j.at(), due, j.swap(), j.what(), from, going.getHoverName().getString(), place, worn.getHoverName().getString(), reach, server);
        if (list.isEmpty()) {
            if (!Moves.byHand(mc, p, place, from)) {
                return false;
            }
            lastDone = tick;
            if (LOG) {
                cursed.recode.CursedRecode.LOGGER.info("{}, by hand, speed {}", said, String.format(java.util.Locale.ROOT, "%.3f", p.getDeltaMovement().horizontalDistance()));
            }
            if (!going.isEmpty()) {
                tell(mc, going, place);
            }
            return true;
        }
        clicks = list;
        next = 0;
        nextAt = tick;
        RUN_GAPS[0] = GAPS[0];
        RUN_GAPS[1] = GAPS[1];
        runKeys = stopFrom >= 0 ? keys : 0;
        runClose = close;
        runStood = stopFrom >= 0 ? tick - stopFrom : 0;
        runGoing = going;
        runPlace = place;
        runSaid = said;
        // (its first click not taken, nothing was done)
        return step(mc, p);
    }

    /** While held (a key, a condition): the first thing of the list there is, what was on remembered to go back. */
    private static int holdOn(LocalPlayer p, Swap s, ItemStack worn, Moves.Reach reach) {
        int from = first(p, s, worn, reach);
        if (from >= 0) {
            s.before = worn;
            s.beforeAt = from;
        }
        return from;
    }

    /** The slot of the next thing of the list after the one on (round), the best of its kind; -1 none. */
    static int next(LocalPlayer p, Swap s, ItemStack worn, Moves.Reach reach) {
        int on = -1;
        for (int i = 0; i < s.picks.size() && on < 0; i++) {
            if (Things.fits(s.picks.get(i), worn, worn, Moves.SLOTS[s.place])) {
                on = i;
            }
        }
        int n = s.picks.size();
        for (int k = 1; k <= n; k++) {
            int i = ((on < 0 ? -1 : on) + k) % n;
            int at = best(p, s.picks.get(i), s.place, worn, reach);
            if (at >= 0) {
                return at;
            }
        }
        return -1;
    }

    /** The slot of the first thing of the list there is (none when it is on already). */
    static int first(LocalPlayer p, Swap s, ItemStack worn, Moves.Reach reach) {
        if (Things.fits(s.picks.get(0), worn, worn, Moves.SLOTS[s.place])) {
            return -1;
        }
        for (Things.Pick pick : s.picks) {
            int at = best(p, pick, s.place, worn, reach);
            if (at >= 0) {
                return at;
            }
        }
        return -1;
    }

    /** Worn out: the first of the list with one less worn than what is on (by its order, then the best of it). */
    static int better(LocalPlayer p, Swap s, ItemStack worn, Moves.Reach reach) {
        for (Things.Pick pick : s.picks) {
            int at = best(p, pick, s.place, worn, reach);
            if (at >= 0 && Things.left(p.getInventory().getItem(at)) > Things.left(worn) + 1e-6) {
                return at;
            }
        }
        return -1;
    }

    /** "All armour", piece `k` (1 the head … 4 the feet): its best spare when what is on is worn past `percent` (0:
     *  whenever there is a better one) — another of the same thing first, else the best armour for it. */
    static int piece(LocalPlayer p, int k, float percent, Moves.Reach reach) {
        ItemStack worn = Moves.worn(p, k);
        if (!worn.isEmpty() && percent > 0.0F && !past(worn, percent)) {
            return -1;
        }
        int same = worn.isEmpty() ? -1 : best(p, new Things.Pick(null, null, null, "same"), k, worn, reach);
        if (same >= 0 && Things.left(p.getInventory().getItem(same)) > Things.left(worn) + 1e-6) {
            return same;
        }
        String kind = new String[] {"", "helmet", "chestplate", "leggings", "boots"}[k];
        int any = best(p, new Things.Pick(null, null, null, kind), k, worn, reach);
        if (any >= 0 && (worn.isEmpty() || Things.worth(p.getInventory().getItem(any)) > Things.worth(worn) + 1e-6)) {
            return any;
        }
        return -1;
    }

    /** Where what was on before is now: the slot it went to, else wherever the same thing is. */
    private static int back(LocalPlayer p, Swap s, Moves.Reach reach) {
        ItemStack b = s.before;
        if (b == null || b.isEmpty()) {
            // nothing was on: the thing goes back to its slot (an empty one there and back)
            return s.beforeAt >= 0 && p.getInventory().getItem(s.beforeAt).isEmpty() ? s.beforeAt : -1;
        }
        if (s.beforeAt >= 0 && s.beforeAt < 36 && Things.same(p.getInventory().getItem(s.beforeAt), b) && reach.takes(s.beforeAt, b)) {
            return s.beforeAt;
        }
        for (int i = 0; i < 36; i++) {
            ItemStack st = p.getInventory().getItem(i);
            if (Things.same(st, b) && reach.takes(i, st)) {
                return i;
            }
        }
        return -1;
    }

    /** The slot of the best thing for `pick` (not the very thing on): the best of its kind, then the hotbar's, then
     *  the lowest slot; -1 none. */
    static int best(LocalPlayer p, Things.Pick pick, int place, ItemStack worn, Moves.Reach reach) {
        int found = -1;
        double top = -1.0;
        for (int i = 0; i < 36; i++) {
            ItemStack st = p.getInventory().getItem(i);
            if (st.isEmpty() || !reach.takes(i, st) || !Things.fits(pick, st, worn, Moves.SLOTS[place]) || (Things.same(st, worn) && !"same".equals(pick.kind()))) {
                continue;
            }
            double w = Things.worth(st) + (i < 9 ? 0.5 : 0.0);
            if (w > top) {
                top = w;
                found = i;
            }
        }
        return found;
    }

    /** What went on, said: to the HUD (the Swap Ring always, a note when Notifications is on) and a sound (its own). */
    private static void tell(Minecraft mc, ItemStack going, int place) {
        int note = "0".equals(set.get("Notifications")) ? 0 : 1;
        Native.guiCall("swapnote\t" + going.getHoverName().getString().replace('\t', ' ') + "\t" + place + "\t" + remember(going) + "\t" + note);
        if (!"0".equals(set.get("Sound"))) {
            mc.getSoundManager().play(SimpleSoundInstance.forUI(SoundEvents.ARMOR_EQUIP_GENERIC.value(), 1.0F, 0.6F));
        }
    }
}
