package cursed.recode.dev;

import com.mojang.logging.LogUtils;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;
import java.util.UUID;
import net.minecraft.client.Minecraft;
import net.minecraft.client.server.IntegratedServer;
import net.minecraft.commands.CommandSourceStack;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import org.slf4j.Logger;

/**
 * CURSED_DEV_COMMANDS=file: two seconds after the player is in a singleplayer world, the file's lines run as commands
 * at the player (with the server's rights) — the same busy scene every time, for measuring (mobs, particles, ...).
 */
public final class DevCommands {
    private static final Logger LOGGER = LogUtils.getLogger();
    private static final boolean ON = Dev.on("CURSED_DEV_COMMANDS");
    /** CURSED_DEV_F3: the debug overlay on once in the world (a screenful of text every frame, for measuring the GUI). */
    private static final boolean F3 = Dev.on("CURSED_DEV_F3");
    private static boolean f3Done;
    private static int ticks;
    private static boolean done;

    private DevCommands() {
    }

    /** Lines "every N: command" of the file: run again every N ticks after the first run (moving the camera...). */
    private static final java.util.List<String[]> REPEAT = new java.util.ArrayList<>();
    /** Lines "at N: command": run once, N ticks after the first run (a chat filling up message by message...). */
    private static final java.util.List<String[]> AT = new java.util.ArrayList<>();
    private static int repeatTicks;

    /** CURSED_DEV_SCREEN=video: Video Settings open once the player is in the world (to look at the engine's options). */
    private static final boolean VIDEO = Dev.on("CURSED_DEV_SCREEN") && "video".equals(System.getenv("CURSED_DEV_SCREEN"));
    /** CURSED_DEV_SCREEN=cycle: the inventory opens and closes every 4 seconds (the HUD's texture across screens). */
    private static final boolean CYCLE = Dev.on("CURSED_DEV_SCREEN") && "cycle".equals(System.getenv("CURSED_DEV_SCREEN"));
    /** CURSED_DEV_SCREEN=inventory: the inventory open from 3 seconds in, kept open (the GUI with a screen, measured). */
    private static final boolean INVENTORY = Dev.on("CURSED_DEV_SCREEN") && "inventory".equals(System.getenv("CURSED_DEV_SCREEN"));
    private static int screenTicks;
    private static final String[] @org.jspecify.annotations.Nullable [] OPEN = Dev.on("CURSED_DEV_OPEN")
            ? java.util.Arrays.stream(System.getenv("CURSED_DEV_OPEN").split(",")).map(s -> s.trim().split(":")).toArray(String[][]::new) : null;
    private static int openTicks;
    private static final String @org.jspecify.annotations.Nullable [] CONNECT = Dev.on("CURSED_DEV_CONNECT") ? System.getenv("CURSED_DEV_CONNECT").split(",", 2) : null;
    private static int connectTicks;
    /** CURSED_DEV_SCREEN=menu: the client's menu opens 3 seconds after the player is in the world (with CURSED_DEV_GUI,
     *  the script it then plays on itself: gui/dev.rs). */
    private static final boolean MENU = Dev.on("CURSED_DEV_SCREEN") && "menu".equals(System.getenv("CURSED_DEV_SCREEN"));
    /** CURSED_DEV_SCREEN=menukey: the game's own way in (the test window takes no keys): Right Shift through its keyboard
     *  handler at 3 s, a left click on the screen at CURSED_DEV_CLICK=x,y (window px) at 5 s, Right Shift again at 7 s. */
    private static final boolean MENU_KEY = Dev.on("CURSED_DEV_SCREEN") && "menukey".equals(System.getenv("CURSED_DEV_SCREEN"));
    /** CURSED_DEV_SCREEN=menucycle: Right Shift through the keyboard handler every 25 seconds from 5 s in (the menu open,
     *  closed, open...), for measuring frames with it against frames without. */
    private static final boolean MENU_CYCLE = Dev.on("CURSED_DEV_SCREEN") && "menucycle".equals(System.getenv("CURSED_DEV_SCREEN"));

    /** CURSED_DEV_SCREEN=hud: the HUD with things to show: at 2 s the nearest living thing is the target, a pearl's
     *  cooldown starts; at 3 s the layout editor opens (unless CURSED_DEV_HUDEDIT=0; its script: CURSED_DEV_HUD). */
    private static final boolean HUD = Dev.on("CURSED_DEV_SCREEN") && "hud".equals(System.getenv("CURSED_DEV_SCREEN"));

    /** CURSED_DEV_FULLSCREEN=s,s...: fullscreen toggled (as F11) at those seconds of ticking (the stand's keys do not reach SDL). */
    private static final String FULLSCREEN = System.getenv("CURSED_DEV_FULLSCREEN");
    private static int fullscreenTicks;

    private static final String NEW_WORLD = Dev.on("CURSED_DEV_NEWWORLD") ? System.getenv("CURSED_DEV_NEWWORLD") : null;
    private static boolean newWorldMade;
    private static int newWorldTicks;
    private static final boolean FLY = Dev.on("CURSED_DEV_FLY");
    /** CURSED_DEV_SPIN=degrees: from five seconds in the world, hovering in creative flight and turning that much a tick. */
    private static final float SPIN = Dev.on("CURSED_DEV_SPIN") ? Float.parseFloat(System.getenv("CURSED_DEV_SPIN")) : 0.0F;
    private static int spinTicks;
    /** CURSED_DEV_WALK=ticks: from five seconds in the world, walking forward that many ticks, back as many, again. */
    private static final int WALK = Dev.on("CURSED_DEV_WALK") ? Integer.parseInt(System.getenv("CURSED_DEV_WALK")) : 0;
    private static int walkTicks;
    private static int flyTicks;

    /** A screen of the game by its dev name (CURSED_DEV_OPEN, DevInput "open"): "c-chest", "c-furnace"… are the
     *  client's own containers (their menu not a server's), the rest the game's screens; null for none. */
    public static net.minecraft.client.gui.screens.@org.jspecify.annotations.Nullable Screen screen(Minecraft minecraft, String name, @org.jspecify.annotations.Nullable String label) {
        var p = minecraft.player;
        if (name.startsWith("c-")) {
            var inv = minecraft.player.getInventory();
            var title = net.minecraft.network.chat.Component.literal(label != null ? label : name.substring(2));
            net.minecraft.client.gui.screens.Screen sc = switch (name.substring(2)) {
                case "chest" -> new net.minecraft.client.gui.screens.inventory.ContainerScreen(net.minecraft.world.inventory.ChestMenu.threeRows(1, inv), inv, title);
                case "big" -> new net.minecraft.client.gui.screens.inventory.ContainerScreen(net.minecraft.world.inventory.ChestMenu.sixRows(1, inv), inv, title);
                case "furnace" -> new net.minecraft.client.gui.screens.inventory.FurnaceScreen(new net.minecraft.world.inventory.FurnaceMenu(1, inv), inv, title);
                case "villager" -> {
                    // a trader with a few offers of its own (one sold out, one cheaper), a level and experience
                    var mm = new net.minecraft.world.inventory.MerchantMenu(1, inv);
                    var offers = new net.minecraft.world.item.trading.MerchantOffers();
                    offers.add(new net.minecraft.world.item.trading.MerchantOffer(new net.minecraft.world.item.trading.ItemCost(net.minecraft.world.item.Items.EMERALD, 6),
                            java.util.Optional.of(new net.minecraft.world.item.trading.ItemCost(net.minecraft.world.item.Items.BOOK, 1)),
                            new net.minecraft.world.item.ItemStack(net.minecraft.world.item.Items.ENCHANTED_BOOK), 12, 5, 0.2F));
                    var cheap = new net.minecraft.world.item.trading.MerchantOffer(new net.minecraft.world.item.trading.ItemCost(net.minecraft.world.item.Items.EMERALD, 12),
                            new net.minecraft.world.item.ItemStack(net.minecraft.world.item.Items.DIAMOND_SWORD), 3, 10, 0.2F);
                    cheap.setSpecialPriceDiff(-4);
                    offers.add(cheap);
                    var sold = new net.minecraft.world.item.trading.MerchantOffer(new net.minecraft.world.item.trading.ItemCost(net.minecraft.world.item.Items.WHEAT, 20),
                            new net.minecraft.world.item.ItemStack(net.minecraft.world.item.Items.EMERALD), 1, 2, 0.05F);
                    sold.setToOutOfStock();
                    offers.add(sold);
                    for (int k = 0; k < 6; k++) {
                        offers.add(new net.minecraft.world.item.trading.MerchantOffer(new net.minecraft.world.item.trading.ItemCost(net.minecraft.world.item.Items.EMERALD, 1 + k),
                                new net.minecraft.world.item.ItemStack(net.minecraft.world.item.Items.BREAD, 4 + k), 16, 2, 0.05F));
                    }
                    mm.setOffers(offers);
                    mm.setMerchantLevel(2);
                    mm.setXp(30);
                    mm.setShowProgressBar(true);
                    yield new net.minecraft.client.gui.screens.inventory.MerchantScreen(mm, inv, title);
                }
                case "crafting" -> new net.minecraft.client.gui.screens.inventory.CraftingScreen(new net.minecraft.world.inventory.CraftingMenu(1, inv), inv, title);
                case "enchant" -> new net.minecraft.client.gui.screens.inventory.EnchantmentScreen(new net.minecraft.world.inventory.EnchantmentMenu(1, inv), inv, title);
                case "anvil" -> new net.minecraft.client.gui.screens.inventory.AnvilScreen(new net.minecraft.world.inventory.AnvilMenu(1, inv), inv, title);
                case "beacon" -> new net.minecraft.client.gui.screens.inventory.BeaconScreen(new net.minecraft.world.inventory.BeaconMenu(1, inv), inv, title);
                case "brewing" -> new net.minecraft.client.gui.screens.inventory.BrewingStandScreen(new net.minecraft.world.inventory.BrewingStandMenu(1, inv), inv, title);
                case "hopper" -> new net.minecraft.client.gui.screens.inventory.HopperScreen(new net.minecraft.world.inventory.HopperMenu(1, inv), inv, title);
                case "stonecutter" -> new net.minecraft.client.gui.screens.inventory.StonecutterScreen(new net.minecraft.world.inventory.StonecutterMenu(1, inv), inv, title);
                case "loom" -> {
                    // a banner and a dye already in it: its patterns show
                    var lm = new net.minecraft.world.inventory.LoomMenu(1, inv);
                    var ls = new net.minecraft.client.gui.screens.inventory.LoomScreen(lm, inv, title);
                    lm.getSlot(0).set(new net.minecraft.world.item.ItemStack(net.minecraft.world.item.Items.BANNER.pick(net.minecraft.world.item.DyeColor.WHITE)));
                    lm.getSlot(1).set(new net.minecraft.world.item.ItemStack(net.minecraft.world.item.Items.DYE.pick(net.minecraft.world.item.DyeColor.RED)));
                    yield ls;
                }
                case "smithing" -> new net.minecraft.client.gui.screens.inventory.SmithingScreen(new net.minecraft.world.inventory.SmithingMenu(1, inv), inv, title);
                case "grindstone" -> new net.minecraft.client.gui.screens.inventory.GrindstoneScreen(new net.minecraft.world.inventory.GrindstoneMenu(1, inv), inv, title);
                case "cartography" -> new net.minecraft.client.gui.screens.inventory.CartographyTableScreen(new net.minecraft.world.inventory.CartographyTableMenu(1, inv), inv, title);
                case "crafter" -> new net.minecraft.client.gui.screens.inventory.CrafterScreen(new net.minecraft.world.inventory.CrafterMenu(1, inv), inv, title);
                case "dispenser" -> new net.minecraft.client.gui.screens.inventory.DispenserScreen(new net.minecraft.world.inventory.DispenserMenu(1, inv), inv, title);
                case "shulker" -> new net.minecraft.client.gui.screens.inventory.ShulkerBoxScreen(new net.minecraft.world.inventory.ShulkerBoxMenu(1, inv), inv, title);
                case "smoker" -> new net.minecraft.client.gui.screens.inventory.SmokerScreen(new net.minecraft.world.inventory.SmokerMenu(1, inv), inv, title);
                case "blast" -> new net.minecraft.client.gui.screens.inventory.BlastFurnaceScreen(new net.minecraft.world.inventory.BlastFurnaceMenu(1, inv), inv, title);
                case "inv" -> new net.minecraft.client.gui.screens.inventory.InventoryScreen(minecraft.player);
                case "creative" -> new net.minecraft.client.gui.screens.inventory.CreativeModeInventoryScreen(minecraft.player, minecraft.player.connection.enabledFeatures(), true);
                default -> null;
            };
            return sc;
        }
        return switch (name) {
        case "pause" -> new net.minecraft.client.gui.screens.PauseScreen(true);
        case "chat" -> new net.minecraft.client.gui.screens.ChatScreen("", false);
        case "adv" -> new net.minecraft.client.gui.screens.advancements.AdvancementsScreen(p.connection.getAdvancements(), null);
        case "video" -> new net.minecraft.client.gui.screens.options.VideoSettingsScreen(new net.minecraft.client.gui.screens.PauseScreen(true), minecraft, minecraft.options);
        case "stats" -> new net.minecraft.client.gui.screens.achievement.StatsScreen(null, p.getStats());
        case "options" -> new net.minecraft.client.gui.screens.options.OptionsScreen(new net.minecraft.client.gui.screens.PauseScreen(true), minecraft.options);
        case "death" -> new net.minecraft.client.gui.screens.DeathScreen(net.minecraft.network.chat.Component.literal("Lunuca fell from a high place"), false, p);
        case "social" -> new net.minecraft.client.gui.screens.social.SocialInteractionsScreen(null);
        case "confirm" -> new net.minecraft.client.gui.screens.ConfirmScreen(yes -> minecraft.gui.setScreen(null),
                net.minecraft.network.chat.Component.translatable("deathScreen.quit.confirm"), net.minecraft.network.chat.Component.translatable("selectWorld.deleteWarning", "New World"));
        case "disconnected" -> new net.minecraft.client.gui.screens.DisconnectedScreen(new net.minecraft.client.gui.screens.PauseScreen(true), net.minecraft.network.chat.Component.translatable("connect.failed"),
                net.minecraft.network.chat.Component.literal("Connection refused: no further information"));
        case "message" -> new net.minecraft.client.gui.screens.GenericMessageScreen(net.minecraft.network.chat.Component.translatable("menu.savingLevel"));
        case "progress" -> {
            var ps = new net.minecraft.client.gui.screens.ProgressScreen(false);
            ps.progressStartNoAbort(net.minecraft.network.chat.Component.translatable("menu.loadingLevel"));
            ps.progressStage(net.minecraft.network.chat.Component.translatable("menu.generatingTerrain"));
            ps.progressStagePercentage(42);
            yield ps;
        }
        case "worldopts" -> new net.minecraft.client.gui.screens.WorldOptionsScreen(new net.minecraft.client.gui.screens.PauseScreen(true), minecraft.level);
        case "credits" -> new net.minecraft.client.gui.screens.WinScreen(false, () -> minecraft.gui.setScreen(null));
        case "poem" -> new net.minecraft.client.gui.screens.WinScreen(true, () -> minecraft.gui.setScreen(null));
        case "attribution" -> new net.minecraft.client.gui.screens.CreditsAndAttributionScreen(new net.minecraft.client.gui.screens.PauseScreen(true));
        case "debugopts" -> new net.minecraft.client.gui.screens.debug.DebugOptionsScreen();
        case "gamemode" -> new net.minecraft.client.gui.screens.debug.GameModeSwitcherScreen();
        case "friends" -> new net.minecraft.client.gui.screens.friends.FriendsOverlayScreen(null);
        case "packs" -> new net.minecraft.client.gui.screens.packs.PackSelectionScreen(minecraft.getResourcePackRepository(), r -> minecraft.gui.setScreen(null),
                minecraft.getResourcePackDirectory(), net.minecraft.network.chat.Component.translatable("resourcePack.title"));
        case "toasts" -> {
            var tm = minecraft.gui.toastManager();
            tm.addToast(new net.minecraft.client.gui.components.toasts.AdvancementToast(net.minecraft.advancements.Advancement.Builder.advancement()
                    .display(net.minecraft.world.item.Items.DIAMOND, net.minecraft.network.chat.Component.translatable("advancements.story.mine_diamond.title"),
                            net.minecraft.network.chat.Component.translatable("advancements.story.mine_diamond.description"), net.minecraft.advancements.AdvancementType.TASK, true, false, false)
                    .build(net.minecraft.resources.Identifier.withDefaultNamespace("dev/a"))));
            tm.addToast(new net.minecraft.client.gui.components.toasts.AdvancementToast(net.minecraft.advancements.Advancement.Builder.advancement()
                    .display(net.minecraft.world.item.Items.NETHERITE_INGOT, net.minecraft.network.chat.Component.translatable("advancements.nether.netherite_armor.title"),
                            net.minecraft.network.chat.Component.translatable("advancements.nether.netherite_armor.description"), net.minecraft.advancements.AdvancementType.CHALLENGE, true, false, false)
                    .build(net.minecraft.resources.Identifier.withDefaultNamespace("dev/b"))));
            net.minecraft.client.gui.components.toasts.SystemToast.add(tm, net.minecraft.client.gui.components.toasts.SystemToast.SystemToastId.WORLD_BACKUP,
                    net.minecraft.network.chat.Component.translatable("selectWorld.edit.backupCreated", "Новый мир"), net.minecraft.network.chat.Component.translatable("selectWorld.edit.backupSize", 12));
            tm.addToast(new net.minecraft.client.gui.components.toasts.TutorialToast(minecraft.font, net.minecraft.client.gui.components.toasts.TutorialToast.Icons.MOVEMENT_KEYS,
                    net.minecraft.network.chat.Component.translatable("tutorial.move.title", "W", "A", "S", "D"), net.minecraft.network.chat.Component.translatable("tutorial.move.description", "Space"), true));
            yield null;
        }
        // the game's notices whose words it wraps onto two lines of its own (two of its slots each)
        case "toasts-disk" -> {
            net.minecraft.client.gui.components.toasts.SystemToast.onLowDiskSpace(minecraft);
            net.minecraft.client.gui.components.toasts.SystemToast.onChunkSaveFailure(minecraft, new net.minecraft.world.level.ChunkPos(-21, 2));
            yield null;
        }
        // Chams on the stand whatever the menu says (cursed.recode.chams.Chams): "chams 2" on, "chams 0" off
        case "chams" -> {
            cursed.recode.chams.Chams.devLook = label == null ? -1 : Integer.parseInt(label.strip());
            yield null;
        }
        // a module's setting in the menu, by its label: "set chams|Smoke=0" (gui/mod.rs "put")
        case "set" -> {
            String[] mv = label == null ? new String[0] : label.split("\\|", 2);
            if (mv.length == 2) {
                cursed.recode.render.Native.guiCall("put\t" + mv[0] + "\t" + mv[1]);
            }
            yield null;
        }
        // the game's music started, its toast shown even when the stand is silent (options: musicToast "pause_and_toast")
        case "music" -> {
            minecraft.getMusicManager().stopPlaying();
            minecraft.getMusicManager().startPlaying(net.minecraft.sounds.Musics.GAME);
            minecraft.getMusicManager().showNowPlayingToastIfNeeded();
            yield null;
        }
        // the tutorial's toasts whose icons are no item (the page draws its own)
        case "toasts-tutorial" -> {
            var tm = minecraft.gui.toastManager();
            tm.addToast(new net.minecraft.client.gui.components.toasts.TutorialToast(minecraft.font, net.minecraft.client.gui.components.toasts.TutorialToast.Icons.MOUSE,
                    net.minecraft.network.chat.Component.translatable("tutorial.look.title"), net.minecraft.network.chat.Component.translatable("tutorial.look.description"), true));
            minecraft.gui.addSocialInteractionsToast();
            tm.addToast(new net.minecraft.client.gui.components.toasts.TutorialToast(minecraft.font, net.minecraft.client.gui.components.toasts.TutorialToast.Icons.RIGHT_CLICK,
                    net.minecraft.network.chat.Component.literal("Нажмите ПКМ"), net.minecraft.network.chat.Component.literal("Используйте предмет"), false));
            yield null;
        }
        case "rules" -> new net.minecraft.client.gui.screens.options.InWorldGameRulesScreen(p.connection, r -> minecraft.gui.setScreen(null), null);
        case "addserver" -> new net.minecraft.client.gui.screens.ManageServerScreen(null, net.minecraft.network.chat.Component.translatable("manageServer.add.title"), b -> minecraft.gui.setScreen(null),
                new net.minecraft.client.multiplayer.ServerData(net.minecraft.client.resources.language.I18n.get("selectServer.defaultName"), "", net.minecraft.client.multiplayer.ServerData.Type.OTHER));
        case "direct" -> new net.minecraft.client.gui.screens.DirectJoinServerScreen(null, b -> minecraft.gui.setScreen(null),
                new net.minecraft.client.multiplayer.ServerData("", "", net.minecraft.client.multiplayer.ServerData.Type.OTHER));
        case "link" -> new net.minecraft.client.gui.screens.ConfirmLinkScreen(yes -> minecraft.gui.setScreen(null), java.net.URI.create("https://cursedclient.xyz/recode"), true);
        case "alert" -> new net.minecraft.client.gui.screens.AlertScreen(() -> minecraft.gui.setScreen(null), net.minecraft.network.chat.Component.translatable("multiplayer.title"),
                net.minecraft.network.chat.Component.translatable("multiplayer.disconnect.server_shutdown"));
        default -> null;
        };
    }

    public static void tick(Minecraft minecraft) {
        // CURSED_DEV_NEWWORLD="folder:seed": from the main menu, a new creative world of that seed made and entered
        // (worlds for measuring: the same land each time)
        if (NEW_WORLD != null && !newWorldMade && minecraft.player == null && minecraft.level == null
                && minecraft.gui.screen() instanceof cursed.recode.launch.LaunchScreen && ++newWorldTicks > 40) {
            newWorldMade = true;
            String[] w = NEW_WORLD.split(":");
            long seed = w.length > 1 ? Long.parseLong(w[1].trim()) : 0L;
            LOGGER.info("[dev] a new world {} of seed {}", w[0], seed);
            minecraft.createWorldOpenFlows().createFreshLevel(w[0],
                    new net.minecraft.world.level.LevelSettings(w[0], net.minecraft.world.level.GameType.CREATIVE,
                            net.minecraft.world.level.LevelSettings.DifficultySettings.DEFAULT, true, net.minecraft.world.level.WorldDataConfiguration.DEFAULT),
                    new net.minecraft.world.level.levelgen.WorldOptions(seed, true, false),
                    net.minecraft.world.level.levelgen.presets.WorldPresets::createNormalWorldDimensions, minecraft.gui.screen());
        }
        // CURSED_DEV_FLY: from five seconds in the world, flying forward at the sprint (as W and Ctrl held in creative),
        // west and a little down: the chunks come in as they do for a player, not by teleports.
        if (FLY && minecraft.player != null && ++flyTicks > 100) {
            var p = minecraft.player;
            p.getAbilities().mayfly = true;
            p.getAbilities().flying = true;
            p.setYRot(90.0F);
            p.setXRot(10.0F);
            minecraft.options.keyUp.setDown(true);
            minecraft.options.keySprint.setDown(true);
        }
        if (SPIN != 0.0F && minecraft.player != null && ++spinTicks > 100) {
            var p = minecraft.player;
            p.setYRot(p.getYRot() + SPIN);
            // drifting forward slowly in flight, about a block a second (as a player looking round); CURSED_DEV_SPINSTILL: only turning
            if (!Dev.on("CURSED_DEV_SPINSTILL")) {
                p.getAbilities().mayfly = true;
                p.getAbilities().flying = true;
                p.getAbilities().setFlyingSpeed(0.005F);
                minecraft.options.keyUp.setDown(true);
            }
        }
        if (WALK > 0 && minecraft.player != null && ++walkTicks > 100) {
            boolean forward = (walkTicks / WALK) % 2 == 0;
            minecraft.options.keyUp.setDown(forward);
            minecraft.options.keyDown.setDown(!forward);
        }
        if (FULLSCREEN != null && Dev.on("CURSED_DEV_FULLSCREEN")) {
            fullscreenTicks++;
            for (String at : FULLSCREEN.split(",")) {
                if (fullscreenTicks == Math.round(Float.parseFloat(at.trim()) * 20)) {
                    LOGGER.info("[dev] fullscreen toggled");
                    minecraft.options.fullscreen().set(!minecraft.options.fullscreen().get());
                }
            }
        }
        if (HUD && minecraft.player != null) {
            screenTicks++;
            // CURSED_DEV_HUDHITS=s: the target hit again every s seconds (its element comes and goes, over and over)
            int hits = Dev.on("CURSED_DEV_HUDHITS") ? Math.max(1, Integer.parseInt(System.getenv("CURSED_DEV_HUDHITS"))) * 20 : 0;
            if ((screenTicks == 52 || hits > 0 && screenTicks > 52 && (screenTicks - 52) % hits == 0) && minecraft.level != null) {
                var p = minecraft.player;
                net.minecraft.world.entity.LivingEntity best = null;
                for (var e : minecraft.level.entitiesForRendering()) {
                    if (e instanceof net.minecraft.world.entity.LivingEntity l && e != p && !(e instanceof net.minecraft.world.entity.decoration.ArmorStand)
                            && (best == null || p.distanceTo(e) < p.distanceTo(best))) {
                        best = l;
                    }
                }
                if (best != null) {
                    cursed.recode.hud.HudLayer.attacked(best);
                }
                p.getCooldowns().addCooldown(new net.minecraft.world.item.ItemStack(net.minecraft.world.item.Items.ENDER_PEARL), 400);
                p.getCooldowns().addCooldown(new net.minecraft.world.item.ItemStack(net.minecraft.world.item.Items.SHIELD), 160);
                LOGGER.info("[dev] hud: target {}", best);
            }
            if (screenTicks == 64 && !"0".equals(System.getenv("CURSED_DEV_HUDEDIT")) && minecraft.gui.screen() == null) {
                // CURSED_DEV_HUDEDIT=chat: the chat instead of the editor (the HUD edited from the chat)
                minecraft.gui.setScreen("chat".equals(System.getenv("CURSED_DEV_HUDEDIT")) ? new net.minecraft.client.gui.screens.ChatScreen("", false)
                        : new cursed.recode.hud.HudEditor());
            }
        }
        // CURSED_DEV_NOHUD: the HUD hidden (as F1) once in the world, for pictures of the world itself
        if (minecraft.player != null && Dev.on("CURSED_DEV_NOHUD") && !minecraft.gui.hud.isHidden()) {
            minecraft.gui.hud.toggle();
        }
        // CURSED_DEV_CONNECT="6,127.0.0.1:1": from the main menu, connecting to that address at that second (the
        // connecting and disconnected screens)
        if (CONNECT != null && minecraft.player == null && minecraft.level == null) {
            connectTicks++;
            if (connectTicks == Math.round(Float.parseFloat(CONNECT[0]) * 20)) {
                LOGGER.info("[dev] connect {}", CONNECT[1]);
                net.minecraft.client.gui.screens.ConnectScreen.startConnecting(minecraft.gui.screen(), minecraft, net.minecraft.client.multiplayer.resolver.ServerAddress.parseString(CONNECT[1]),
                        new net.minecraft.client.multiplayer.ServerData("Stand", CONNECT[1], net.minecraft.client.multiplayer.ServerData.Type.OTHER), false, null);
            }
        }
        // CURSED_DEV_OPEN="4:pause,9:adv,12:close": the game's screens opened at those seconds in the world (the menus'
        // stand: its window takes no keys)
        if (OPEN != null && minecraft.player != null) {
            openTicks++;
            for (String[] step : OPEN) {
                if (Math.round(Float.parseFloat(step[0]) * 20) == openTicks && step[1].equals("mouse")) {
                    // the pointer put at x,y (window px; the stand's window gets no mouse of its own)
                    minecraft.mouseHandler.onMove(minecraft.getWindow().handle(), Double.parseDouble(step[2]), Double.parseDouble(step[3]), 0.0, 0.0);
                    continue;
                }
                if (Math.round(Float.parseFloat(step[0]) * 20) == openTicks && step[1].startsWith("c-")) {
                    // a container screen of the client's own (its menu not the server's: for looking at it)
                    LOGGER.info("[dev] open {}", step[1]);
                    minecraft.gui.setScreen(screen(minecraft, step[1], step.length > 2 ? step[2] : null));
                    continue;
                }
                if (Math.round(Float.parseFloat(step[0]) * 20) == openTicks && step[1].equals("chatup")) {
                    // the open chat scrolled up three lines (its screen left as it is)
                    minecraft.gui.hud.getChat().scrollChat(3);
                    continue;
                }
                if (Math.round(Float.parseFloat(step[0]) * 20) == openTicks) {
                    LOGGER.info("[dev] open {}", step[1]);
                    minecraft.gui.setScreen(screen(minecraft, step[1], null));
                }
            }
        }
        if (VIDEO && minecraft.player != null && ++screenTicks == 60) {
            minecraft.setScreenAndShow(new net.minecraft.client.gui.screens.options.VideoSettingsScreen(null, minecraft, minecraft.options));
        }
        if (MENU_CYCLE && minecraft.player != null && ++screenTicks % 500 == 100) {
            var key = new net.minecraft.client.input.KeyEvent(com.mojang.blaze3d.platform.InputConstants.KEY_RSHIFT, 0, 0);
            minecraft.keyboardHandler.keyPress(minecraft.getWindow().handle(), 1, key);
            minecraft.keyboardHandler.keyPress(minecraft.getWindow().handle(), 0, key);
            LOGGER.info("[dev] menu cycle: Right Shift, the menu {}", minecraft.gui.screen() instanceof cursed.recode.gui.ClickGui ? "opens" : "closes");
        }
        if (MENU_KEY && minecraft.player != null) {
            screenTicks++;
            var key = new net.minecraft.client.input.KeyEvent(com.mojang.blaze3d.platform.InputConstants.KEY_RSHIFT, 0, 0);
            if (screenTicks == 60 || screenTicks == 140) {
                minecraft.keyboardHandler.keyPress(minecraft.getWindow().handle(), 1, key);
                minecraft.keyboardHandler.keyPress(minecraft.getWindow().handle(), 0, key);
                LOGGER.info("[dev] Right Shift, screen now {}", minecraft.gui.screen());
            }
            String at = System.getenv("CURSED_DEV_CLICK");
            if (screenTicks == 100 && at != null && minecraft.gui.screen() != null) {
                double s = minecraft.getWindow().getGuiScale();
                String[] xy = at.split(",");
                var info = new net.minecraft.client.input.MouseButtonInfo(com.mojang.blaze3d.platform.InputConstants.MOUSE_BUTTON_LEFT, 0);
                var e = new net.minecraft.client.input.MouseButtonEvent(Double.parseDouble(xy[0]) / s, Double.parseDouble(xy[1]) / s, info);
                minecraft.gui.screen().mouseClicked(e, false);
                minecraft.gui.screen().mouseReleased(e);
                LOGGER.info("[dev] click at {}", at);
            }
        }
        if (MENU && minecraft.player != null && ++screenTicks == 60 && minecraft.gui.screen() == null && cursed.recode.render.GuiFrame.active()) {
            minecraft.gui.setScreen(new cursed.recode.gui.ClickGui());
        }
        if (INVENTORY && minecraft.player != null && ++screenTicks >= 60 && minecraft.gui.screen() == null) {
            minecraft.setScreenAndShow(new net.minecraft.client.gui.screens.inventory.InventoryScreen(minecraft.player));
        }
        if (CYCLE && minecraft.player != null && ++screenTicks % 80 == 0) {
            if (minecraft.gui.screen() == null) {
                minecraft.setScreenAndShow(new net.minecraft.client.gui.screens.inventory.InventoryScreen(minecraft.player));
            } else {
                minecraft.setScreenAndShow(null);
            }
        }
        if (done && (!REPEAT.isEmpty() || !AT.isEmpty()) && minecraft.getSingleplayerServer() != null && minecraft.player != null) {
            repeatTicks++;
            IntegratedServer server = minecraft.getSingleplayerServer();
            UUID who = minecraft.player.getUUID();
            java.util.List<String[]> due = new java.util.ArrayList<>();
            for (String[] r : REPEAT) {
                if (repeatTicks % Integer.parseInt(r[0]) == 0) {
                    due.add(r);
                }
            }
            for (String[] r : AT) {
                if (repeatTicks == Integer.parseInt(r[0])) {
                    due.add(r);
                }
            }
            for (String[] r : due) {
                {
                    server.execute(() -> {
                        ServerPlayer player = server.getPlayerList().getPlayer(who);
                        if (player != null) {
                            server.getCommands().performPrefixedCommand(server.createCommandSourceStack().withEntity(player)
                                    .withLevel((ServerLevel) player.level()).withPosition(player.position())
                                    .withRotation(player.getRotationVector()).withSuppressedOutput(), r[1]);
                        }
                    });
                }
            }
        }
        if (F3 && !f3Done && minecraft.player != null) {
            f3Done = true;
            minecraft.debugEntries.setOverlayVisible(true);
        }
        if (!ON || done || minecraft.player == null) {
            return;
        }
        IntegratedServer server = minecraft.getSingleplayerServer();
        if (server == null || ++ticks < 40) {
            return;
        }
        done = true;
        List<String> lines;
        try {
            lines = Files.readAllLines(Path.of(System.getenv("CURSED_DEV_COMMANDS")));
        } catch (Exception e) {
            LOGGER.warn("[dev commands] cannot read", e);
            return;
        }
        UUID id = minecraft.player.getUUID();
        server.execute(() -> {
            ServerPlayer player = server.getPlayerList().getPlayer(id);
            if (player == null) {
                return;
            }
            CommandSourceStack source = server.createCommandSourceStack()
                    .withEntity(player)
                    .withLevel((ServerLevel) player.level())
                    .withPosition(player.position())
                    .withRotation(player.getRotationVector())
                    .withSuppressedOutput();
            int n = 0;
            for (String line : lines) {
                String command = line.strip();
                if (command.startsWith("every ")) {
                    int colon = command.indexOf(':');
                    REPEAT.add(new String[]{command.substring(6, colon).strip(), command.substring(colon + 1).strip()});
                    continue;
                }
                if (command.startsWith("at ")) {
                    int colon = command.indexOf(':');
                    AT.add(new String[]{command.substring(3, colon).strip(), command.substring(colon + 1).strip()});
                    continue;
                }
                if (!command.isEmpty() && !command.startsWith("#")) {
                    server.getCommands().performPrefixedCommand(source, command);
                    n++;
                }
            }
            LOGGER.info("[dev commands] ran {}; the player at {} {} {}, yaw {} pitch {}", n,
                    player.getX(), player.getY(), player.getZ(), player.getYRot(), player.getXRot());
        });
    }
}
