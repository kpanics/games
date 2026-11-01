package cursed.recode.engine;

import com.mojang.serialization.Codec;
import cursed.recode.CursedRecode;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;
import java.util.Locale;
import java.util.Properties;
import net.minecraft.client.Minecraft;
import net.minecraft.client.OptionInstance;
import net.minecraft.client.Options;
import net.minecraft.network.chat.Component;

/**
 * What the engine draws, chosen in Video Settings (VideoSettingsScreenMixin): a quality from off to ultra, and how big
 * the world is drawn against the window. Kept in cursed-recode/engine.properties of the game folder; a change takes
 * effect with the next frame.
 */
public final class EngineSettings {
    public enum Quality {
        // Bits: 1 shadows, 2 coloured light, 4 reflections, 8 moving water, 32 light in the air, 64 soft shadows,
        // 128 glow, 256 lit clouds, 512 wind, 1024 the sky shaped by the air, 2048/4096/8192 sections rising, falling or growing in
        // (Chunk Animator; Engine sends them, engine/gpu.rs reads them).
        OFF(0, 100),
        LOW(2 | 128 | 256 | 512 | 1024, 67),
        MEDIUM(1 | 2 | 4 | 8 | 128 | 256 | 512 | 1024, 80),
        HIGH(1 | 2 | 4 | 8 | 32 | 64 | 128 | 256 | 512 | 1024, 100),
        ULTRA(1 | 2 | 4 | 8 | 32 | 64 | 128 | 256 | 512 | 1024, 100);

        final int flags;
        /** The world's size in percent this quality starts with. */
        final int scale;

        Quality(int flags, int scale) {
            this.flags = flags;
            this.scale = scale;
        }

        Component caption() {
            return Component.translatable("cursed.options.engine." + this.name().toLowerCase(Locale.ROOT));
        }
    }

    private static final Codec<Quality> QUALITY_CODEC = Codec.STRING.xmap(s -> {
        try {
            return Quality.valueOf(s.toUpperCase(Locale.ROOT));
        } catch (IllegalArgumentException e) {
            return Quality.OFF;
        }
    }, q -> q.name().toLowerCase(Locale.ROOT));

    // (off until the player turns it on: her word, 22.10)
    private static Quality quality = Quality.OFF;
    private static int scale = 100;
    private static boolean loaded;

    public static final OptionInstance<Quality> QUALITY = new OptionInstance<>(
            "cursed.options.engine",
            OptionInstance.cachedConstantTooltip(Component.translatable("cursed.options.engine.tooltip")),
            (caption, value) -> value.caption(),
            new OptionInstance.Enum<>(List.of(Quality.values()), QUALITY_CODEC),
            quality(),
            value -> {
                quality = value;
                save();
                // The world's size starts where this quality starts (the slider moves with it).
                EngineSettings.SCALE.set(value.scale);
            });

    public static final OptionInstance<Integer> SCALE = new OptionInstance<>(
            "cursed.options.worldScale",
            OptionInstance.cachedConstantTooltip(Component.translatable("cursed.options.worldScale.tooltip")),
            (caption, value) -> Options.genericValueLabel(caption, Component.literal(value + "%")),
            new OptionInstance.IntRange(10, 20).<Integer>xmap(v -> v * 5, v -> v / 5, true),
            Codec.intRange(50, 100),
            scalePercent(),
            value -> {
                scale = value;
                save();
            });

    private EngineSettings() {
    }

    private static Path file() {
        return Minecraft.getInstance().gameDirectory.toPath().resolve("cursed-recode").resolve("engine.properties");
    }

    private static void load() {
        if (loaded) {
            return;
        }
        loaded = true;
        try {
            Path f = file();
            if (Files.isRegularFile(f)) {
                Properties p = new Properties();
                try (var in = Files.newBufferedReader(f)) {
                    p.load(in);
                }
                quality = QUALITY_CODEC.parse(com.mojang.serialization.JavaOps.INSTANCE, p.getProperty("quality", "off")).result().orElse(Quality.OFF);
                scale = Math.clamp(Integer.parseInt(p.getProperty("scale", String.valueOf(quality.scale)).strip()), 50, 100);
            }
        } catch (Exception e) {
            CursedRecode.LOGGER.warn("[engine] settings unreadable, the defaults stay", e);
        }
    }

    private static void save() {
        try {
            Path f = file();
            Files.createDirectories(f.getParent());
            Properties p = new Properties();
            p.setProperty("quality", quality.name().toLowerCase(Locale.ROOT));
            p.setProperty("scale", String.valueOf(scale));
            try (var out = Files.newBufferedWriter(f)) {
                p.store(out, "Cursed Recode engine (Video Settings)");
            }
        } catch (IOException e) {
            CursedRecode.LOGGER.warn("[engine] settings not saved", e);
        }
    }

    public static Quality quality() {
        load();
        return quality;
    }

    private static int scalePercent() {
        load();
        return scale;
    }

    /** The world's size against the window, 0.5..1. */
    public static float scale() {
        load();
        return scale / 100.0F;
    }

    public static int flags() {
        return quality().flags;
    }
}
