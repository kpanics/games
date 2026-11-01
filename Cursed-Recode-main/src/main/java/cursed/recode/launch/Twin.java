package cursed.recode.launch;

import cursed.recode.render.Native;
import java.nio.file.Files;
import java.nio.file.Path;
import net.fabricmc.loader.api.FabricLoader;
import net.fabricmc.loader.api.entrypoint.PreLaunchEntrypoint;
import org.jspecify.annotations.Nullable;

/**
 * The window before the game (native/src/twin): started before the game (cursed_twin.exe — the launcher, run.cmd or
 * IntelliJ) and joined here, or, without it, started as early as the game's process allows (Fabric's preLaunch).
 * A spark burns the game's window open over the desktop and the card plays in it while the game loads. The game's
 * window is made hidden in the same place (CursedBackend, WindowMixin) and shown under it once it draws the menu
 * (LaunchScreen); both draw the card from one clock, so the window simply goes. On the way out the same window closes
 * the game's back into the spark.
 *
 * Not with a fullscreen game. CURSED_DEV_NOTWIN (or -Dcursed.recode.notwin) leaves it out.
 */
public final class Twin implements PreLaunchEntrypoint {
    /** The game's window: picture x y w h, frame x y w h (physical px); null without the window before the game. */
    private static int @Nullable [] place;
    private static boolean handed;
    private static final float[] TIMES = new float[4];

    @Override
    public void onPreLaunch() {
        try {
            boot();
        } catch (Throwable t) {
            place = null;
        }
    }

    private static void boot() throws Exception {
        if (Boolean.getBoolean("cursed.recode.notwin") || System.getenv("CURSED_DEV_NOTWIN") != null) {
            return;
        }
        // started before the game by the launcher, run.cmd or IntelliJ (cursed_twin.exe): join it
        String link = System.getenv("CURSED_TWIN");
        if (link == null) {
            // started by IntelliJ's run configuration (cursed.recode.dev.IdeaBoot)
            link = System.getProperty("cursed.recode.twin");
        }
        Native.loadEarly();
        Native.appId();
        if (link != null && !link.isBlank()) {
            int[] p = Native.twinAttach(link);
            if (p != null) {
                place = p;
                org.slf4j.LoggerFactory.getLogger("Cursed Recode").info("The window before the game joined: {}", link);
                return;
            }
            // its program is another build's (the link's layout differs) or it is gone: its card would play on over
            // the game's own window
            org.slf4j.LoggerFactory.getLogger("Cursed Recode").warn("The window before the game ({}) could not be joined: CursedRecode.exe and cursed_twin.exe of another build than the DLL? (cargo build --release in native)", link);
        }
        Path options = FabricLoader.getInstance().getGameDir().resolve("options.txt");
        if (Files.isRegularFile(options) && Files.readString(options).contains("fullscreen:true")) {
            return;
        }
        int w = 0, h = 0;
        String[] args = FabricLoader.getInstance().getLaunchArguments(false);
        for (int i = 0; i + 1 < args.length; i++) {
            if (args[i].equals("--width")) {
                w = Integer.parseInt(args[i + 1]);
            } else if (args[i].equals("--height")) {
                h = Integer.parseInt(args[i + 1]);
            }
        }
        // the game's default 854 × 480 is a stamp on today's screens: then the window gets most of the screen
        if (w == 854 && h == 480) {
            w = h = 0;
        }
        String at = System.getenv("CURSED_DEV_WINDOW_AT");
        int ax = 0, ay = 0;
        if (at != null) {
            String[] xy = at.split(",");
            ax = Integer.parseInt(xy[0].trim());
            ay = Integer.parseInt(xy[1].trim());
        }
        Native.launchVolume(mute() ? 0.0F : option(options, "soundCategory_master"), option(options, "soundCategory_music"));
        int[] p = Native.twinPlace(w, h, ax, ay, at != null);
        if (p != null && Native.twinStart(p, mute())) {
            place = p;
            // the game's process waits for the way out to finish before it ends
            Runtime.getRuntime().addShutdownHook(new Thread(() -> {
                long until = System.nanoTime() + 7_000_000_000L;
                while (Native.twinBusy() && System.nanoTime() < until) {
                    try {
                        Thread.sleep(15);
                    } catch (InterruptedException e) {
                        return;
                    }
                }
            }, "Cursed twin wait"));
        }
    }

    /** CURSED_DEV_MUTE: the launch makes no sound (test runs on her PC). */
    public static boolean mute() {
        return FabricLoader.getInstance().isDevelopmentEnvironment() && System.getenv("CURSED_DEV_MUTE") != null;
    }

    /** A volume from options.txt (1 when it is not there). */
    private static float option(Path options, String key) {
        try {
            for (String line : Files.readAllLines(options)) {
                if (line.startsWith(key + ":")) {
                    return Float.parseFloat(line.substring(key.length() + 1).trim());
                }
            }
        } catch (Exception ignored) {
            // the default
        }
        return 1.0F;
    }

    /** The window before the game plays the launch: the game's window waits hidden at {@link #place()}. */
    public static boolean active() {
        return place != null && !handed;
    }

    public static int @Nullable [] place() {
        return place;
    }

    /**
     * The card's clocks as the window before the game keeps them (life, card, forge, bits: 1 it plays the card, 2 its
     * wait for the game is over), or null without it. The game keeps its `life` for the whole session (the farewell
     * goes on from it).
     */
    public static float @Nullable [] times() {
        if (place == null) {
            return null;
        }
        Native.twinTimes(TIMES);
        return Float.isNaN(TIMES[0]) ? null : TIMES;
    }

    public static void ready() {
        if (place != null) {
            Native.twinReady();
        }
    }

    private static float loadSaid = -1.0F;

    /** The game's resources loading (0..1), as the loading screen sees it: the card's point may run that far. */
    public static void load(float p) {
        if (place != null && !handed && p >= loadSaid + 0.01F) {
            loadSaid = p;
            Native.twinLoad(p);
        }
    }

    /** The game's window is up under it, drawing: it goes. */
    public static void release() {
        if (place != null && !handed) {
            handed = true;
            Native.twinRelease();
        }
    }

    /** The window before the game is minimised: the game's window comes up minimised, its taskbar button blinking. */
    public static boolean hidden() {
        return place != null && !handed && Native.twinHidden();
    }

    /** The farewell's first picture is up over the game's window. */
    public static boolean shown() {
        return place != null && Native.twinShown();
    }

    /** How long the game waits for the farewell's first picture before it goes by itself (s). */
    private static final float SHOW_WAIT = 5.0F;

    /**
     * The farewell asked for at `askedAt` (ns): whether the game may go now — its first picture is up over the game's
     * window, or it has not come in time. Which it was (and when) goes into the log.
     */
    public static boolean handedOver(long askedAt, long now) {
        float s = (now - askedAt) / 1e9F;
        if (shown()) {
            org.slf4j.LoggerFactory.getLogger("Cursed Recode").info("[quit] the window before the game took the way out {} ms after it was asked", Math.round(s * 1000.0F));
            return true;
        }
        if (s >= SHOW_WAIT) {
            org.slf4j.LoggerFactory.getLogger("Cursed Recode").warn("[quit] the window before the game did not show in {} s: the game goes by itself", SHOW_WAIT);
            return true;
        }
        return false;
    }

    /** The way out over the game's window (its HWND); see {@link Native#twinFarewell}. */
    public static boolean farewell(long hwnd, float[] menu, float[] view) {
        try {
            if (place == null) {
                org.slf4j.LoggerFactory.getLogger("Cursed Recode").info("The farewell: no window before the game this time (the menu goes out alone)");
                return false;
            }
            int[] p = Native.twinPlaceOf(hwnd);
            boolean ok = p != null && p[2] > 0 && Native.twinFarewell(p, menu, view);
            if (!ok) {
                org.slf4j.LoggerFactory.getLogger("Cursed Recode").warn("The farewell was not taken: place {}", p == null ? "none" : java.util.Arrays.toString(p));
            }
            return ok;
        } catch (Throwable e) {
            org.slf4j.LoggerFactory.getLogger("Cursed Recode").warn("The farewell failed", e);
            return false;
        }
    }
}
