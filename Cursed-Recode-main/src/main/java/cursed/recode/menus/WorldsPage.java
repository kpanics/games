package cursed.recode.menus;

import com.google.gson.JsonArray;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import cursed.recode.CursedRecode;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.concurrent.ConcurrentHashMap;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.screens.worldselection.CreateWorldScreen;
import net.minecraft.client.gui.screens.worldselection.EditWorldScreen;
import net.minecraft.client.gui.screens.worldselection.SelectWorldScreen;
import net.minecraft.client.gui.screens.worldselection.WorldSelectionList;
import net.minecraft.util.Util;
import net.minecraft.world.level.storage.LevelStorageSource;
import net.minecraft.world.level.storage.LevelSummary;
import org.jspecify.annotations.Nullable;

/**
 * Singleplayer (the game's world list) for the menus' page: her worlds with what they are (mode, difficulty, when
 * last played, the version, commands), how long she played and how big they are (counted aside), their pictures (the
 * client's own shot of where she left off, else the world's icon); playing, renaming in place, a backup, the folder,
 * deleting held (the world goes to the client's bin first: it can come back), a new world.
 */
public final class WorldsPage implements Page {
    final SelectWorldScreen screen;
    final Minecraft mc = Minecraft.getInstance();
    private final List<WorldSelectionList.WorldListEntry> entries = new ArrayList<>();
    /** Counted aside, by folder: size in bytes, ticks played. */
    private static final Map<String, long[]> COUNTED = new ConcurrentHashMap<>();
    /** The last world sent to the bin (folder, where it is now), for Undo; and what the page is told about it. */
    private @Nullable String[] binned;
    private String said = "";

    WorldsPage(SelectWorldScreen screen) {
        this.screen = screen;
    }

    static Path bin() {
        return Minecraft.getInstance().gameDirectory.toPath().resolve("cursed-recode").resolve("bin");
    }

    /** The bin of worlds deleted last time is emptied (they could be brought back only until the game closed). */
    public static void emptyBin() {
        Path b = bin();
        if (Files.isDirectory(b)) {
            Util.backgroundExecutor().execute(() -> {
                try (var walk = Files.walk(b)) {
                    walk.sorted(java.util.Comparator.reverseOrder()).forEach(p -> p.toFile().delete());
                } catch (IOException ignored) {
                    // what is left goes next time
                }
            });
        }
    }

    /** A world's own shot of where she left off (cursed.recode.menus.WorldShot), else its icon. */
    static @Nullable Path picture(Path worldDir, LevelSummary s) {
        Path shot = worldDir.resolve(WorldShot.FILE);
        if (Files.isRegularFile(shot)) {
            return shot;
        }
        return s.getIcon() != null && Files.isRegularFile(s.getIcon()) ? s.getIcon() : null;
    }

    /** Counted aside by folder and when it was last played (played again, it is counted again). */
    private void count(String folder, Path dir) {
        if (COUNTED.containsKey(folder)) {
            return;
        }
        COUNTED.put(folder, new long[] {-1, -1});
        Util.backgroundExecutor().execute(() -> {
            long size = 0, ticks = 0;
            try (var walk = Files.walk(dir)) {
                size = walk.filter(Files::isRegularFile).mapToLong(p -> p.toFile().length()).sum();
            } catch (IOException ignored) {
                // unknown
            }
            // the players' statistics: players/stats since 26.x, stats in the worlds before
            Path statsDir = Files.isDirectory(dir.resolve("players").resolve("stats")) ? dir.resolve("players").resolve("stats") : dir.resolve("stats");
            try (var stats = Files.list(statsDir)) {
                for (Path f : stats.filter(p -> p.toString().endsWith(".json")).toList()) {
                    var root = JsonParser.parseString(Files.readString(f)).getAsJsonObject();
                    var custom = root.getAsJsonObject("stats").getAsJsonObject("minecraft:custom");
                    if (custom != null && custom.has("minecraft:play_time")) {
                        ticks = Math.max(ticks, custom.get("minecraft:play_time").getAsLong());
                    }
                }
            } catch (Exception ignored) {
                // never played
            }
            COUNTED.put(folder, new long[] {size, ticks});
        });
    }

    @Override
    public JsonObject data() {
        this.entries.clear();
        JsonObject o = new JsonObject();
        JsonArray worlds = new JsonArray();
        var list = this.screen.list;
        boolean loading = list == null;
        int sel = -1;
        Path base = this.mc.getLevelSource().getBaseDir();
        if (list != null) {
            // the game's list takes its loaded worlds as it draws: it is never drawn here
            var levels = list.pollLevelsIgnoreErrors();
            if (levels != list.currentlyDisplayedLevels) {
                list.handleNewLevels(levels);
            }
            for (var e : list.children()) {
                if (!(e instanceof WorldSelectionList.WorldListEntry we)) {
                    if (e instanceof WorldSelectionList.LoadingHeader) {
                        loading = true;
                    }
                    continue;
                }
                LevelSummary s = we.getLevelSummary();
                JsonObject w = new JsonObject();
                String folder = s.getLevelId();
                Path dir = base.resolve(folder);
                w.addProperty("folder", folder);
                w.addProperty("name", s.getLevelName());
                w.addProperty("mode", s.isHardcore() ? Widgets.text(net.minecraft.network.chat.Component.translatable("selectWorld.gameMode.hardcore"))
                        : s.getGameMode().getLongDisplayName().getString());
                w.addProperty("hardcore", s.isHardcore());
                try {
                    w.addProperty("difficulty", s.getSettings().difficultySettings().difficulty().getDisplayName().getString());
                } catch (RuntimeException ignored) {
                    // a world too old to say
                }
                w.addProperty("cheats", s.hasCommands());
                w.addProperty("version", s.getWorldVersionName().getString());
                w.addProperty("last", s.getLastPlayed());
                w.addProperty("locked", s.isLocked());
                w.addProperty("ok", s.primaryActionActive());
                w.addProperty("info", s.getInfo().getString());
                String counted = folder + "@" + s.getLastPlayed();
                this.count(counted, dir);
                long[] c = COUNTED.get(counted);
                w.addProperty("size", c[0]);
                w.addProperty("played", c[1]);
                Path pic = picture(dir, s);
                if (pic != null) {
                    String key = "world:" + folder + ":" + pic.toFile().lastModified();
                    if (Pics.file(key, pic)) {
                        w.addProperty("pic", key);
                        w.addProperty("pixel", !pic.getFileName().toString().equals(WorldShot.FILE));
                    }
                }
                if (e == list.getSelected()) {
                    sel = this.entries.size();
                }
                this.entries.add(we);
                worlds.add(w);
            }
        }
        o.add("worlds", worlds);
        o.addProperty("sel", sel);
        o.addProperty("loading", loading);
        o.addProperty("said", this.said);
        o.addProperty("undo", this.binned != null);
        return o;
    }

    private WorldSelectionList.@Nullable WorldListEntry at(String n) {
        try {
            int i = Integer.parseInt(n.trim());
            return i >= 0 && i < this.entries.size() ? this.entries.get(i) : null;
        } catch (NumberFormatException e) {
            return null;
        }
    }

    @Override
    public void act(String verb, String rest) {
        String[] a = rest.split(" ", 2);
        var e = this.at(a[0]);
        var list = this.screen.list;
        switch (verb) {
            case "select" -> {
                if (e != null && list != null) {
                    list.setSelected(e);
                }
            }
            case "play" -> {
                if (e != null) {
                    cursed.recode.launch.Transition.beginEnter(e.getLevelSummary().getLevelName(), 0);
                    e.joinWorld();
                }
            }
            case "create" -> CreateWorldScreen.openFresh(this.mc, () -> this.mc.gui.setScreen(this.screen));
            case "edit" -> {
                if (e != null) {
                    e.editWorld();
                }
            }
            case "rename" -> {
                if (e != null && a.length > 1 && !a[1].isBlank()) {
                    String folder = e.getLevelSummary().getLevelId();
                    try (LevelStorageSource.LevelStorageAccess access = this.mc.getLevelSource().validateAndCreateAccess(folder)) {
                        access.renameLevel(a[1].strip());
                    } catch (Exception ex) {
                        CursedRecode.LOGGER.warn("[menus] can't rename {}", folder, ex);
                    }
                    this.reload();
                }
            }
            case "backup" -> {
                if (e != null) {
                    String folder = e.getLevelSummary().getLevelId();
                    try {
                        LevelStorageSource.LevelStorageAccess access = this.mc.getLevelSource().validateAndCreateAccess(folder);
                        EditWorldScreen.makeBackupAndShowToast(access).whenComplete((ok, err) -> {
                            try {
                                access.close();
                            } catch (IOException ignored) {
                                // closed
                            }
                        });
                        this.said = "backup";
                    } catch (Exception ex) {
                        CursedRecode.LOGGER.warn("[menus] can't back up {}", folder, ex);
                    }
                }
            }
            case "folder" -> {
                if (e != null) {
                    com.mojang.blaze3d.Blaze3D.openPath(this.mc.getLevelSource().getBaseDir().resolve(e.getLevelSummary().getLevelId()));
                }
            }
            case "delete" -> {
                if (e != null && e.getLevelSummary().canDelete()) {
                    String folder = e.getLevelSummary().getLevelId();
                    try {
                        Path to = bin().resolve(folder + "-" + System.currentTimeMillis());
                        Files.createDirectories(to.getParent());
                        Files.move(this.mc.getLevelSource().getBaseDir().resolve(folder), to, StandardCopyOption.ATOMIC_MOVE);
                        this.binned = new String[] {folder, to.toString(), e.getLevelSummary().getLevelName()};
                        this.said = "deleted";
                    } catch (IOException ex) {
                        CursedRecode.LOGGER.warn("[menus] can't delete {}", folder, ex);
                    }
                    this.reload();
                }
            }
            case "undo" -> {
                if (this.binned != null) {
                    try {
                        Files.move(Path.of(this.binned[1]), this.mc.getLevelSource().getBaseDir().resolve(this.binned[0]), StandardCopyOption.ATOMIC_MOVE);
                    } catch (IOException ex) {
                        CursedRecode.LOGGER.warn("[menus] can't bring {} back", this.binned[0], ex);
                    }
                    this.binned = null;
                    this.said = "";
                    this.reload();
                }
            }
            case "said" -> this.said = "";
            case "back" -> this.screen.onClose();
            default -> CursedRecode.LOGGER.warn("[menus] worlds were asked «{} {}»", verb, rest);
        }
    }

    private void reload() {
        if (this.screen.list != null) {
            this.screen.list.reloadWorldList();
        }
    }
}
