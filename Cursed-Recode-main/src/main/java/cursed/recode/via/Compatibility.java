package cursed.recode.via;

import java.util.Locale;
import java.util.Map;
import java.util.concurrent.ConcurrentHashMap;

/** Session-local Auto hints for proxies that advertise a modern protocol but send incompatible
 * play packets. An explicit version always wins; selecting Auto again discards its hint. */
public final class Compatibility {
    public static final int FALLBACK = 774; // 1.21.11, the last release before the 26.x protocols.
    private static final Map<String, Integer> HINTS = new ConcurrentHashMap<>();
    private Compatibility() {}

    private static String key(String address) { return address.trim().toLowerCase(Locale.ROOT); }

    public static int automatic(String address, int advertised) {
        return address == null || advertised <= FALLBACK ? advertised : HINTS.getOrDefault(key(address), advertised);
    }

    public static void reset(String address) { if (address != null) HINTS.remove(key(address)); }

    /** One fresh attempt only, before the connection has settled. Ordinary kicks, authentication
     * failures, timeouts, manual versions and a failed fallback never start another connection. */
    public static boolean failed(String address, int selected, int manual, long ageMillis, String error) {
        if (address == null || manual != 0 || selected <= FALLBACK || selected > 777 || ageMillis < 0 || ageMillis > 15_000
                || error == null || !error.startsWith("Failed to decode packet 'clientbound/minecraft:")) return false;
        return HINTS.putIfAbsent(key(address), FALLBACK) == null;
    }
}
