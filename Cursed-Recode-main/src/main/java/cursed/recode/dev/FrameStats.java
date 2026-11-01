package cursed.recode.dev;

import com.mojang.logging.LogUtils;
import java.util.Arrays;
import org.slf4j.Logger;

/**
 * CURSED_DEV_BENCH: the time between frames, in the log every 10 seconds (average FPS and the slow frames), so a
 * renderer can be measured in the same place with the same tool as the one it is compared with.
 */
public final class FrameStats {
    private static final Logger LOGGER = LogUtils.getLogger();
    /** CURSED_DEV_FPS: the frame times alone, without the GPU timestamps of CURSED_DEV_BENCH (they stall the GPU at every pass). */
    private static final boolean ON = Dev.on("CURSED_DEV_BENCH") || Dev.on("CURSED_DEV_FPS");
    private static final long WINDOW_NS = 10_000_000_000L;
    private static long last;
    private static long windowStart;
    private static long[] frames = new long[4096];
    private static int count;

    private FrameStats() {
    }

    /** CURSED_DEV_STALLS: a watcher writes where the render thread is when a frame is 35, 70, 105… ms late (or every so
     *  many ms as its value says: CURSED_DEV_STALLS=8). */
    private static final boolean WATCH = Dev.on("CURSED_DEV_STALLS");
    private static final long STEP = stallStep();

    private static long stallStep() {
        try {
            return Math.max(2L, Long.parseLong(System.getenv("CURSED_DEV_STALLS")));
        } catch (RuntimeException e) {
            return 35L;
        }
    }
    private static volatile long frameAt;
    private static volatile Thread renderThread;

    private static void watch() {
        Thread t = new Thread(() -> {
            long seen = 0L;
            int step = 0;
            while (true) {
                try {
                    Thread.sleep(Math.min(5L, STEP / 2L));
                } catch (InterruptedException e) {
                    return;
                }
                long at = frameAt;
                Thread r = renderThread;
                if (r == null || at == 0L) {
                    continue;
                }
                if (at != seen) {
                    seen = at;
                    step = 0;
                }
                long late = (System.nanoTime() - at) / 1_000_000L;
                if (late >= STEP * (step + 1) && step < 12) {
                    step++;
                    StackTraceElement[] st = r.getStackTrace();
                    StringBuilder b = new StringBuilder();
                    for (int i = 0; i < Math.min(14, st.length); i++) {
                        b.append("\n      at ").append(st[i]);
                    }
                    LOGGER.info("[stall] render thread {} ms into its frame, {}:{}", late, r.getState(), b);
                }
            }
        }, "Cursed stall watcher");
        t.setDaemon(true);
        t.setPriority(Thread.MAX_PRIORITY);
        t.start();
    }

    public static void frame() {
        if (WATCH) {
            if (renderThread == null) {
                renderThread = Thread.currentThread();
                watch();
            }
            frameAt = System.nanoTime();
        }
        if (!ON) {
            return;
        }
        long now = System.nanoTime();
        FeatureTimes.frame(now);
        if (last != 0L) {
            if (count == frames.length) {
                frames = Arrays.copyOf(frames, count * 2);
            }
            frames[count++] = now - last;
        } else {
            windowStart = now;
        }
        last = now;
        if (now - windowStart >= WINDOW_NS && count > 0) {
            long[] sorted = Arrays.copyOf(frames, count);
            Arrays.sort(sorted);
            long total = 0L;
            for (long f : sorted) {
                total += f;
            }
            double avgMs = total / 1e6 / count;
            LOGGER.info("[bench] frames {} fps {} avg {} ms p50 {} p95 {} p99 {} max {} ms",
                    count, String.format("%.1f", 1000.0 / avgMs), String.format("%.2f", avgMs),
                    ms(sorted, 0.50), ms(sorted, 0.95), ms(sorted, 0.99), String.format("%.2f", sorted[count - 1] / 1e6));
            count = 0;
            windowStart = now;
        }
    }

    private static String ms(long[] sorted, double q) {
        return String.format("%.2f", sorted[Math.min(sorted.length - 1, (int) (sorted.length * q))] / 1e6);
    }
}
