package cursed.recode.play;

import it.unimi.dsi.fastutil.ints.IntList;
import java.util.List;
import net.minecraft.client.Minecraft;
import net.minecraft.client.multiplayer.ClientLevel;
import net.minecraft.core.particles.BlockParticleOption;
import net.minecraft.core.particles.DustParticleOptions;
import net.minecraft.core.particles.ParticleTypes;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.resources.Identifier;
import net.minecraft.sounds.SoundSource;
import net.minecraft.util.RandomSource;
import net.minecraft.world.entity.EntitySpawnReason;
import net.minecraft.world.entity.EntityTypes;
import net.minecraft.world.entity.LightningBolt;
import net.minecraft.world.item.component.FireworkExplosion;
import net.minecraft.world.level.block.Blocks;

/**
 * Kill Effects' part the game shows (native gui/killfx.rs decides which, where, how big): a soul rising, blood or oil
 * bursting, lightning, fireworks, a sound — on the player's screen only (nothing of it goes to the server).
 */
final class KillFx {
    private static final RandomSource RANDOM = RandomSource.create();
    /** The lightning's ids: the client's own, far below the server's (and the doubles'). */
    private static int nextId = -88_000;

    private KillFx() {
    }

    /** "fx soul x y z size", "fx blood x y z size oil", "fx lightning x y z", "fx fireworks x y z rrggbb size". */
    static void fx(Minecraft mc, String rest) {
        ClientLevel level = mc.level;
        String[] f = rest.split(" ");
        if (level == null || f.length < 4) {
            return;
        }
        double x;
        double y;
        double z;
        try {
            x = Double.parseDouble(f[1]);
            y = Double.parseDouble(f[2]);
            z = Double.parseDouble(f[3]);
        } catch (NumberFormatException e) {
            return;
        }
        float size = f.length > 4 && !f[0].equals("fireworks") ? parse(f[4], 1.0F) : f.length > 5 ? parse(f[5], 1.0F) : 1.0F;
        switch (f[0]) {
            case "soul" -> {
                // a spiral of souls going up, a few sculk souls among them
                int n = Math.round(22 * size);
                for (int i = 0; i < n; i++) {
                    double a = i * 0.75;
                    double r = 0.35 * size;
                    level.addParticle(i % 5 == 0 ? ParticleTypes.SCULK_SOUL : ParticleTypes.SOUL,
                            x + Math.cos(a) * r, y - 0.4 + i * 0.05, z + Math.sin(a) * r,
                            Math.cos(a + 1.6) * 0.02, 0.05 + RANDOM.nextDouble() * 0.05, Math.sin(a + 1.6) * 0.02);
                }
            }
            case "blood" -> {
                boolean oil = f.length > 5 && f[5].equals("1");
                var chunk = new BlockParticleOption(ParticleTypes.BLOCK, (oil ? Blocks.COAL_BLOCK : Blocks.REDSTONE_BLOCK).defaultBlockState());
                var drop = new DustParticleOptions(oil ? 0x15130f : 0x9c0a12, 1.6F);
                int n = Math.round(36 * size);
                for (int i = 0; i < n; i++) {
                    double vx = (RANDOM.nextDouble() - 0.5) * 0.5 * size;
                    double vy = RANDOM.nextDouble() * 0.35 * size;
                    double vz = (RANDOM.nextDouble() - 0.5) * 0.5 * size;
                    level.addParticle(i % 2 == 0 ? chunk : drop, x, y, z, vx, vy, vz);
                }
            }
            case "lightning" -> {
                LightningBolt bolt = EntityTypes.LIGHTNING_BOLT.create(level, EntitySpawnReason.TRIGGERED);
                if (bolt != null) {
                    // (an entity of the client's own needs an id before the world takes it)
                    bolt.setId(nextId);
                    nextId = nextId <= -88_999 ? -88_000 : nextId - 1;
                    bolt.setPos(x, y, z);
                    bolt.setVisualOnly(true);
                    level.addEntity(bolt);
                }
            }
            case "fireworks" -> {
                int rgb = 0xffffff;
                try {
                    rgb = Integer.parseInt(f.length > 4 ? f[4] : "ffffff", 16);
                } catch (NumberFormatException e) {
                    // white
                }
                var shape = size > 1.3F ? FireworkExplosion.Shape.LARGE_BALL : FireworkExplosion.Shape.STAR;
                var boom = new FireworkExplosion(shape, IntList.of(rgb), IntList.of(0xffffff), true, true);
                level.createFireworks(x, y, z, 0.0, 0.0, 0.0, List.of(boom), true);
            }
            default -> {
            }
        }
    }

    /** "sound <id> x y z volume pitch": a sound of the game's where the kill was. */
    static void sound(Minecraft mc, String rest) {
        ClientLevel level = mc.level;
        String[] f = rest.split(" ");
        if (level == null || f.length < 6) {
            return;
        }
        var id = Identifier.tryParse(f[0]);
        var ev = id == null ? null : BuiltInRegistries.SOUND_EVENT.getValue(id);
        if (ev == null) {
            return;
        }
        level.playLocalSound(parse(f[1], 0), parse(f[2], 0), parse(f[3], 0), ev, SoundSource.PLAYERS, parse(f[4], 0.6F), parse(f[5], 1.0F), false);
    }

    private static float parse(String s, float or) {
        try {
            return Float.parseFloat(s);
        } catch (NumberFormatException e) {
            return or;
        }
    }
}
