package cursed.recode.menus;

import net.minecraft.client.gui.screens.Screen;

/** Which page draws which of the game's screens, and its page's Java side. */
final class Kinds {
    private Kinds() {
    }

    /** A page of its own for the screens that have one; every other the mirror (its widgets in the menus' way). */
    static String of(Screen s) {
        if (s instanceof net.minecraft.client.gui.screens.PauseScreen) {
            return "pause";
        }
        if (s instanceof net.minecraft.client.gui.screens.worldselection.SelectWorldScreen) {
            return "worlds";
        }
        if (s instanceof net.minecraft.client.gui.screens.worldselection.CreateWorldScreen) {
            return "create";
        }
        if (s instanceof net.minecraft.client.gui.screens.multiplayer.JoinMultiplayerScreen) {
            return "servers";
        }
        if (s instanceof net.minecraft.client.gui.screens.advancements.AdvancementsScreen) {
            return "advancements";
        }
        if (s instanceof net.minecraft.client.gui.screens.achievement.StatsScreen) {
            return "stats";
        }
        if (s instanceof net.minecraft.client.gui.screens.packs.PackSelectionScreen) {
            return "packs";
        }
        if (s instanceof net.minecraft.client.gui.screens.WinScreen) {
            return "credits";
        }
        if (s instanceof net.minecraft.client.gui.screens.debug.GameModeSwitcherScreen) {
            return "gamemode";
        }
        if (s instanceof net.minecraft.client.gui.screens.social.SocialInteractionsScreen) {
            return "social";
        }
        if (s instanceof net.minecraft.client.gui.screens.DeathScreen) {
            return "death";
        }
        if (NoticePage.is(s)) {
            return "notice";
        }
        if (s instanceof net.minecraft.client.gui.screens.options.OptionsScreen
                || s instanceof net.minecraft.client.gui.screens.options.OptionsSubScreen && !(s instanceof net.minecraft.client.gui.screens.options.InWorldGameRulesScreen)) {
            return "settings";
        }
        return "mirror";
    }

    static Page page(String kind, Screen s) {
        return switch (kind) {
            case "settings" -> new SettingsPage(s);
            case "pause" -> new PausePage((net.minecraft.client.gui.screens.PauseScreen) s);
            case "worlds" -> new WorldsPage((net.minecraft.client.gui.screens.worldselection.SelectWorldScreen) s);
            case "create" -> new CreatePage((net.minecraft.client.gui.screens.worldselection.CreateWorldScreen) s);
            case "servers" -> new ServersPage((net.minecraft.client.gui.screens.multiplayer.JoinMultiplayerScreen) s);
            case "advancements" -> new AdvancementsPage((net.minecraft.client.gui.screens.advancements.AdvancementsScreen) s);
            case "stats" -> new StatsPage((net.minecraft.client.gui.screens.achievement.StatsScreen) s);
            case "death" -> new DeathPage((net.minecraft.client.gui.screens.DeathScreen) s);
            case "notice" -> new NoticePage(s);
            case "packs" -> new PacksPage((net.minecraft.client.gui.screens.packs.PackSelectionScreen) s);
            case "credits" -> new CreditsPage((net.minecraft.client.gui.screens.WinScreen) s);
            case "gamemode" -> new GameModePage((net.minecraft.client.gui.screens.debug.GameModeSwitcherScreen) s);
            case "social" -> new SocialPage((net.minecraft.client.gui.screens.social.SocialInteractionsScreen) s);
            case "loader" -> new LicensePage((cursed.recode.LicenseScreen) s);
            default -> new Mirror(s);
        };
    }
}
