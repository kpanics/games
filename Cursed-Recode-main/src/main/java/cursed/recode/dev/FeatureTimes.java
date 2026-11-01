package cursed.recode.dev;

import com.mojang.logging.LogUtils;
import java.util.LinkedHashMap;
import java.util.Map;
import org.slf4j.Logger;

/**
 * CURSED_DEV_BENCH: time spent in some of the game's per-frame work (render thread), per frame, in the log every 10
 * seconds — numbers that other programs on the machine disturb far less than the frame rate.
 */
public final class FeatureTimes {
    private static final Logger LOGGER = LogUtils.getLogger();
    private static final boolean ON = Dev.on("CURSED_DEV_BENCH") || Dev.on("CURSED_DEV_FPS");
    private static final Map<String, long[]> TOTALS = new LinkedHashMap<>();
    private static final Map<String, long[]> COUNTS = new LinkedHashMap<>();
    private static long window;
    private static long frames;

    private FeatureTimes() {
    }

    /** Starts timing `what` (each name keeps its own start, so timings may nest). */
    public static void start(String what) {
        if (ON) {
            TOTALS.computeIfAbsent(what, k -> new long[2])[1] = System.nanoTime();
        }
    }

    public static void stop(String what) {
        if (ON) {
            long[] t = TOTALS.computeIfAbsent(what, k -> new long[2]);
            t[0] += System.nanoTime() - t[1];
        }
    }

    /** Adds n to the count `what` (bytes, draws...), shown per frame next to the times. */
    public static void count(String what, long n) {
        if (ON) {
            COUNTS.computeIfAbsent(what, k -> new long[1])[0] += n;
        }
    }

    /** Once per frame (FrameStats). */
    static void frame(long now) {
        if (!ON) {
            return;
        }
        frames++;
        if (window == 0) {
            window = now;
        }
        if (now - window >= 10_000_000_000L && frames > 0) {
            StringBuilder s = new StringBuilder("[bench] per frame:");
            for (Map.Entry<String, long[]> e : TOTALS.entrySet()) {
                s.append(' ').append(e.getKey()).append(' ').append(String.format("%.1f", e.getValue()[0] / 1000.0 / frames)).append(" us;");
                e.getValue()[0] = 0;
            }
            for (Map.Entry<String, long[]> e : COUNTS.entrySet()) {
                s.append(' ').append(e.getKey()).append(' ').append(e.getValue()[0] / frames).append(';');
                e.getValue()[0] = 0;
            }
            LOGGER.info(s.toString());
            frames = 0;
            window = now;
        }
    }
}
