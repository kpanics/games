package cursed.recode.dev;

import net.fabricmc.loader.api.FabricLoader;

/** Switches for testing; they work only in the development environment (never in a built client). */
public final class Dev {
    private static final boolean DEVELOPMENT = FabricLoader.getInstance().isDevelopmentEnvironment();

    private Dev() {
    }

    public static boolean on(String env) {
        return DEVELOPMENT && System.getenv(env) != null;
    }

    /** CURSED_DEV_MOJANG: the game draws with Mojang's own renderers, to measure ours against them. */
    public static boolean mojang() {
        return on("CURSED_DEV_MOJANG");
    }
}
