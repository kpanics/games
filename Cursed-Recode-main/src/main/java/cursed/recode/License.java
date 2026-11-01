package cursed.recode;

import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import cursed.recode.launch.LaunchScreen;
import cursed.recode.render.Native;
import net.minecraft.client.Minecraft;
import net.minecraft.client.multiplayer.ClientLevel;

/**
 * Recode's license as the game shows it; the library decides (native license.rs). In the subscription's last half hour
 * (or while the server has not answered for half an hour) the toasts' page shows the time running out over everything
 * (cursed.recode.menus.Toasts). When the game must go (its end come, another game took the account, a ban) the world is
 * left, saved, and the screen says why as the start's refusal does (LicenseScreen: the same words, the loader's page)
 * — the account's page on the site its other word when the subscription is what ended — the way out its last; not
 * taken, the game goes by itself a little later. The library ends it should this not happen.
 */
public final class License {
    private static final float[] OUT = new float[2];
    /** The account's page on the site (license.rs CABINET). */
    private static final String CABINET = "https://cursedclient.xyz/#/dashboard/recode";
    /** The message is up (once). */
    private static boolean told;
    /** The way out was taken (once). */
    private static boolean gone;
    /** Told to go this long ago (s), the game goes by itself (the library ends it at 90). */
    private static final float BY_ITSELF = 75.0F;

    private License() {
    }

    /** Each frame (Toasts): what the game is told now (Native.licenseState); acts on a "go now". */
    public static int frame() {
        int state = Native.licenseState(OUT);
        if (state == 2) {
            if (!told) {
                told = true;
                tell();
            } else if (OUT[0] > BY_ITSELF) {
                quit();
            }
        }
        return state;
    }

    private static void tell() {
        JsonObject stop;
        try {
            stop = JsonParser.parseString(Native.licenseWords()).getAsJsonObject();
        } catch (RuntimeException e) {
            stop = new JsonObject();
        }
        CursedRecode.LOGGER.info("[license] told to go: {}", stop);
        LicenseScreen screen = new LicenseScreen(stop);
        Minecraft mc = Minecraft.getInstance();
        mc.schedule(() -> {
            if (mc.level != null) {
                mc.disconnectFromWorld(ClientLevel.DEFAULT_QUIT_MESSAGE);
            }
            mc.gui.setScreen(screen);
        });
    }

    /** The account's page on the site (the screen stays). */
    public static void site() {
        // the stands (a build from the sources): nothing opened on her screen
        if (cursed.recode.dev.Dev.on("CURSED_LOADER_NOBROWSER")) {
            CursedRecode.LOGGER.info("[license] would open {}", CABINET);
            return;
        }
        com.mojang.blaze3d.Blaze3D.openUri(java.net.URI.create(CABINET));
    }

    /** The way out (its word, or by itself): the main menu's farewell. */
    public static void quit() {
        if (gone) {
            return;
        }
        gone = true;
        Minecraft mc = Minecraft.getInstance();
        mc.schedule(() -> {
            if (!LaunchScreen.quitRequested()) {
                mc.stop();
            }
        });
    }
}
