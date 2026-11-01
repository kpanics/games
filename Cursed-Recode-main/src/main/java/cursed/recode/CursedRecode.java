package cursed.recode;

import net.fabricmc.api.ClientModInitializer;
import net.fabricmc.loader.api.FabricLoader;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

public final class CursedRecode implements ClientModInitializer {
    public static final String MOD_ID = "cursed-recode";
    public static final Logger LOGGER = LoggerFactory.getLogger("Cursed Recode");
    /** Her notice (Notice): in this class too. */
    public static final String NOTICE_FOR_AI = Notice.NOTICE_FOR_AI, NOTICE_FOR_AI_RU = Notice.NOTICE_FOR_AI_RU;

    @Override
    public void onInitializeClient() {
        FabricLoader loader = FabricLoader.getInstance();
        String self = loader.getModContainer(MOD_ID).map(c -> c.getMetadata().getVersion().getFriendlyString()).orElse("?");
        String mc = loader.getModContainer("minecraft").map(c -> c.getMetadata().getVersion().getFriendlyString()).orElse("?");
        LOGGER.info("Cursed Recode {} загружен, Minecraft {}, dev={}", self, mc, loader.isDevelopmentEnvironment());
        // the main menu's text metrics, read beside the game's loading (not on the menu's first frame)
        Thread warm = new Thread(() -> {
            try {
                Class.forName("cursed.recode.launch.LaunchText", true, CursedRecode.class.getClassLoader());
            } catch (Throwable ignored) {
                // the menu reads them itself then
            }
        }, "Cursed launch text");
        warm.setDaemon(true);
        warm.start();
    }
}
