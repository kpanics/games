package cursed.recode.menus;

import com.google.gson.JsonArray;
import com.google.gson.JsonObject;
import cursed.recode.CursedRecode;
import java.util.List;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.screens.worldselection.CreateWorldScreen;
import net.minecraft.client.gui.screens.worldselection.WorldCreationGameRulesScreen;
import net.minecraft.client.gui.screens.worldselection.WorldCreationUiState;
import net.minecraft.network.chat.Component;
import net.minecraft.world.Difficulty;

/**
 * A new world (the game's Create World) for the menus' page: its name and folder, game mode, difficulty, world type,
 * seed, structures, bonus chest, commands — set on the game's own state for the new world (its tabs' widgets follow
 * it); game rules, experiments, data packs open the game's screens; creating is the game's. The valley behind shows
 * the terrain (the page's knobs, by the world type's preset and the seed).
 */
final class CreatePage implements Page {
    final CreateWorldScreen screen;
    final WorldCreationUiState ui;
    private static final WorldCreationUiState.SelectedGameMode[] MODES = {WorldCreationUiState.SelectedGameMode.SURVIVAL, WorldCreationUiState.SelectedGameMode.HARDCORE,
            WorldCreationUiState.SelectedGameMode.CREATIVE};

    CreatePage(CreateWorldScreen screen) {
        this.screen = screen;
        this.ui = screen.getUiState();
    }

    @Override
    public JsonObject data() {
        JsonObject o = new JsonObject();
        o.addProperty("name", this.ui.getName());
        o.addProperty("folder", this.ui.getTargetFolder());
        JsonArray modes = new JsonArray(), infos = new JsonArray();
        int mode = 0;
        for (int i = 0; i < MODES.length; i++) {
            modes.add(MODES[i].displayName.getString());
            infos.add(MODES[i].getInfo().getString());
            if (MODES[i] == this.ui.getGameMode()) {
                mode = i;
            }
        }
        o.add("modes", modes);
        o.add("modeInfo", infos);
        o.addProperty("mode", mode);
        JsonArray diffs = new JsonArray(), dinfo = new JsonArray();
        for (Difficulty d : Difficulty.values()) {
            diffs.add(d.getDisplayName().getString());
            dinfo.add(d.getInfo().getString());
        }
        o.add("diffs", diffs);
        o.add("diffInfo", dinfo);
        o.addProperty("diff", this.ui.getDifficulty().ordinal());
        o.addProperty("hardcore", this.ui.isHardcore());
        List<WorldCreationUiState.WorldTypeEntry> types = this.ui.getNormalPresetList();
        JsonArray tnames = new JsonArray(), tkeys = new JsonArray();
        int type = -1;
        for (int i = 0; i < types.size(); i++) {
            tnames.add(types.get(i).describePreset().getString());
            tkeys.add(types.get(i).preset() != null ? types.get(i).preset().unwrapKey().map(k -> k.identifier().getPath()).orElse("") : "");
            if (types.get(i).equals(this.ui.getWorldType())) {
                type = i;
            }
        }
        o.add("types", tnames);
        o.add("typeKeys", tkeys);
        o.addProperty("type", type);
        o.addProperty("typeName", this.ui.getWorldType().describePreset().getString());
        o.addProperty("typeKey", this.ui.getWorldType().preset() != null ? this.ui.getWorldType().preset().unwrapKey().map(k -> k.identifier().getPath()).orElse("") : "");
        o.addProperty("amplified", this.ui.getWorldType().isAmplified());
        o.addProperty("customize", this.ui.getPresetEditor() != null);
        o.addProperty("seed", this.ui.getSeed());
        o.addProperty("seedHint", Component.translatable("selectWorld.seedInfo").getString());
        o.addProperty("structures", this.ui.isGenerateStructures());
        o.addProperty("bonus", this.ui.isBonusChest());
        o.addProperty("commands", this.ui.isAllowCommands());
        o.addProperty("debug", this.ui.isDebug());
        // the game's own words for what is set here
        JsonObject words = new JsonObject();
        for (String k : new String[] {"selectWorld.gameMode", "options.difficulty", "selectWorld.mapType", "selectWorld.enterSeed", "selectWorld.mapFeatures",
                "selectWorld.bonusItems", "selectWorld.allowCommands", "selectWorld.gameRules", "selectWorld.experiments", "selectWorld.dataPacks", "selectWorld.customizeType",
                "selectWorld.create", "gui.cancel", "selectWorld.enterName", "generator.minecraft.amplified.info", "selectWorld.mapFeatures.info", "selectWorld.allowCommands.info"}) {
            words.addProperty(k, Component.translatable(k).getString());
        }
        o.add("words", words);
        return o;
    }

    @Override
    public void act(String verb, String rest) {
        String a = rest.strip();
        switch (verb) {
            case "name" -> this.ui.setName(rest);
            case "mode" -> this.ui.setGameMode(MODES[Math.max(0, Math.min(MODES.length - 1, Integer.parseInt(a)))]);
            case "diff" -> {
                if (!this.ui.isHardcore()) {
                    this.ui.setDifficulty(Difficulty.values()[Math.max(0, Math.min(3, Integer.parseInt(a)))]);
                }
            }
            case "type" -> {
                int i = Integer.parseInt(a);
                var types = this.ui.getNormalPresetList();
                if (i >= 0 && i < types.size()) {
                    this.ui.setWorldType(types.get(i));
                }
            }
            case "seed" -> this.ui.setSeed(rest);
            case "random" -> this.ui.setSeed(Long.toString(new java.util.Random().nextLong()));
            case "structures" -> this.ui.setGenerateStructures(!this.ui.isGenerateStructures());
            case "bonus" -> this.ui.setBonusChest(!this.ui.isBonusChest());
            case "commands" -> {
                if (!this.ui.isHardcore()) {
                    this.ui.setAllowCommands(!this.ui.isAllowCommands());
                }
            }
            case "rules" -> Minecraft.getInstance().gui.setScreen(new WorldCreationGameRulesScreen(
                    this.ui.getGameRules().copy(this.ui.getSettings().dataConfiguration().enabledFeatures()), rules -> {
                        Minecraft.getInstance().gui.setScreen(this.screen);
                        rules.ifPresent(this.ui::setGameRules);
                    }));
            case "experiments" -> this.screen.openExperimentsScreen(this.ui.getSettings().dataConfiguration());
            case "packs" -> this.screen.openDataPackSelectionScreen(this.ui.getSettings().dataConfiguration());
            case "customize" -> {
                var editor = this.ui.getPresetEditor();
                if (editor != null) {
                    Minecraft.getInstance().gui.setScreen(editor.createEditScreen(this.screen, this.ui.getSettings()));
                }
            }
            case "create" -> this.screen.onCreate();
            case "cancel" -> this.screen.popScreen();
            default -> CursedRecode.LOGGER.warn("[menus] a new world was asked «{} {}»", verb, rest);
        }
    }
}
