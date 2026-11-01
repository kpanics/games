package cursed.recode.render;

import com.mojang.blaze3d.pipeline.RenderTarget;
import com.mojang.renderpearl.api.textures.GpuTexture;
import java.util.Arrays;
import java.util.HashMap;
import java.util.Map;
import net.minecraft.client.Minecraft;
import net.minecraft.client.entity.ClientMannequin;
import net.minecraft.client.gui.components.debug.DebugScreenEntries;
import net.minecraft.client.gui.components.debug.DebugScreenEntryStatus;
import net.minecraft.client.multiplayer.ClientLevel;
import net.minecraft.client.player.LocalPlayer;
import net.minecraft.client.renderer.state.level.CameraRenderState;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.EntityTypes;
import net.minecraft.world.entity.ExperienceOrb;
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.entity.MobCategory;
import net.minecraft.world.entity.NeutralMob;
import net.minecraft.world.entity.decoration.ArmorStand;
import net.minecraft.world.entity.item.ItemEntity;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.entity.projectile.Projectile;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.Vec3;
import org.jspecify.annotations.Nullable;

/**
 * Hitboxes (the menu's module "hitboxes", native/src/engine/hitboxes.rs): what is round the player, of the kinds chosen
 * and as far as the setting looks, to the engine each frame: what each is, its box where the frame shows it (between
 * its last tick's place and this one's), its eyes' height and look, its health and hurt, whether aimed at. The game's
 * F3+B is the module's switch, the same as the menu's (KeyboardHandlerMixin; its own lines are not drawn:
 * EntityHitboxDebugRendererMixin).
 */
public final class Hitboxes {
    /** What a box is (engine/hitboxes.rs KINDS). */
    static final int KIND_PLAYER = 0;
    static final int KIND_NPC = 1;
    static final int KIND_HOSTILE = 2;
    static final int KIND_NEUTRAL = 3;
    static final int KIND_PEACEFUL = 4;
    static final int KIND_VILLAGER = 5;
    static final int KIND_ITEM = 6;
    static final int KIND_PROJECTILE = 7;
    static final int KIND_OTHER = 8;
    static final int KIND_SELF = 9;
    static final int AIMED = 1;
    /** No eyes: a thing, not a creature (an item, an arrow, a boat): no eye line, no look. */
    static final int NO_EYES = 2;
    private static final int MOST = 128;
    /** The kinds' switches in the menu, by kind. */
    private static final String[] SWITCHES = {"Players", "NPCs and mannequins", "Hostile mobs", "Neutral mobs", "Peaceful mobs", "Villagers and golems",
            "Items on the ground", "Arrows and projectiles", "Boats, minecarts and the rest", "Yourself (F5)"};
    private static final boolean[] SWITCHED = {true, true, true, true, false, false, false, false, false, false};

    /** The module's settings, asked of the menu four times a second. */
    private static final class Settings {
        boolean on;
        double distance = 24.0;
        final boolean[] kinds = SWITCHED.clone();
    }

    private static Settings settings = new Settings();
    private static long polled;
    private static final int[] INTS = new int[MOST * 3];
    private static final float[] FLOATS = new float[MOST * 12];
    private static final Entity[] ALL = new Entity[4096];
    private static final double[] FAR = new double[4096];
    private static final Integer[] ORDER = new Integer[4096];
    /** Whether boxes went last frame: once none go, the engine is told once more (they fade out). */
    private static boolean sent;
    private static boolean broken;
    /** CURSED_DEV_HITBOXES: the boxes sent, in the log every two seconds. */
    private static final boolean LOG = cursed.recode.dev.Dev.on("CURSED_DEV_HITBOXES");
    private static long logged;

    private Hitboxes() {
    }

    private static Settings settings() {
        long now = System.nanoTime();
        if (now - polled > 250_000_000L) {
            polled = now;
            settings = read(Native.guiCall("set\thitboxes"));
        }
        return settings;
    }

    private static Settings read(@Nullable String s) {
        Settings o = new Settings();
        if (s == null || s.isEmpty()) {
            return o;
        }
        String[] f = s.split("\t");
        o.on = f[0].equals("1");
        Map<String, String> v = new HashMap<>();
        for (int i = 1; i < f.length; i++) {
            int eq = f[i].indexOf('=');
            if (eq > 0) {
                v.put(f[i].substring(0, eq), f[i].substring(eq + 1));
            }
        }
        for (int k = 0; k < SWITCHES.length; k++) {
            String x = v.get(SWITCHES[k]);
            o.kinds[k] = x == null ? SWITCHED[k] : x.equals("1");
        }
        try {
            o.distance = Double.parseDouble(v.getOrDefault("Distance", "24"));
        } catch (NumberFormatException e) {
            o.distance = 24.0;
        }
        return o;
    }

    /** Whether the client draws the boxes (the engine's frame, and they have not failed this launch): the game's own
     *  lines give way and F3+B switches the module only then. */
    public static boolean working() {
        return !broken && EngineFrame.active();
    }

    /** F3+B: the module switched, the same as from the menu; whether it is on now. */
    public static boolean flip() {
        Minecraft mc = Minecraft.getInstance();
        boolean on = !(settings().on || debugShown(mc));
        turn(mc, on);
        return on;
    }

    /** Whether the game's own F3+B is on (from before, its options' screen): the module is switched on for it. */
    private static boolean debugShown(Minecraft mc) {
        return mc.debugEntries != null && mc.debugEntries.isCurrentlyEnabled(DebugScreenEntries.ENTITY_HITBOXES);
    }

    /** The module on or off, the game's own F3+B kept off: one switch shows and hides them, the menu's. */
    private static void turn(Minecraft mc, boolean on) {
        if (mc.debugEntries != null && mc.debugEntries.getStatus(DebugScreenEntries.ENTITY_HITBOXES) != DebugScreenEntryStatus.NEVER) {
            mc.debugEntries.setStatus(DebugScreenEntries.ENTITY_HITBOXES, DebugScreenEntryStatus.NEVER);
        }
        Native.guiCall("switch\thitboxes\t" + (on ? 1 : 0));
        // asked anew
        polled = 0L;
    }

    /** What a thing is, by the module's kinds. */
    private static int kindOf(Entity e, LocalPlayer self, Minecraft mc) {
        if (e == self) {
            return KIND_SELF;
        }
        if (e instanceof ClientMannequin) {
            return KIND_NPC;
        }
        if (e instanceof Player p) {
            // in the tab list a player; a body of no one's there an NPC
            return mc.getConnection() != null && mc.getConnection().getPlayerInfo(p.getUUID()) != null ? KIND_PLAYER : KIND_NPC;
        }
        if (e instanceof ItemEntity || e instanceof ExperienceOrb) {
            return KIND_ITEM;
        }
        if (e instanceof Projectile) {
            return KIND_PROJECTILE;
        }
        if (e instanceof ArmorStand || !(e instanceof LivingEntity)) {
            return KIND_OTHER;
        }
        EntityType<?> t = e.getType();
        if (t == EntityTypes.VILLAGER || t == EntityTypes.WANDERING_TRADER || t == EntityTypes.IRON_GOLEM || t == EntityTypes.SNOW_GOLEM || t == EntityTypes.COPPER_GOLEM) {
            return KIND_VILLAGER;
        }
        if (e instanceof NeutralMob) {
            return KIND_NEUTRAL;
        }
        return t.getCategory() == MobCategory.MONSTER ? KIND_HOSTILE : KIND_PEACEFUL;
    }

    /**
     * After Target ESP (GameRendererMixin): this frame's boxes to the engine, over `main`'s picture (`depth` the level's,
     * when it was drawn into a smaller target).
     */
    public static void flush(RenderTarget main, @Nullable GpuTexture depth, CameraRenderState camera) {
        if (!working()) {
            return;
        }
        Minecraft mc = Minecraft.getInstance();
        ClientLevel level = mc.level;
        LocalPlayer player = mc.player;
        try {
            if (debugShown(mc)) {
                turn(mc, true);
            }
            Settings set = settings();
            if (level == null || player == null || !camera.initialized || !set.on) {
                if (sent) {
                    sent = false;
                    send(main, depth, 0);
                }
                return;
            }
            float partial = mc.getDeltaTracker().getGameTimeDeltaPartialTick(false);
            // from the eye: the engine casts the rays from there (NativeTerrain.eye)
            Vec3 c = NativeTerrain.eye(camera.pos);
            double far = set.distance;
            boolean thirdPerson = !mc.options.getCameraType().isFirstPerson();
            Entity aimed = mc.crosshairPickEntity;
            int found = 0;
            for (Entity e : level.entitiesForRendering()) {
                if (found == ALL.length) {
                    break;
                }
                int kind = kindOf(e, player, mc);
                if (kind == KIND_SELF ? !(thirdPerson && set.kinds[KIND_SELF]) : !set.kinds[kind]) {
                    continue;
                }
                // what is not to be seen is not shown
                if (e.isInvisibleTo(player) || e.isSpectator()) {
                    continue;
                }
                Vec3 at = e.getPosition(partial);
                double dx = at.x - c.x;
                double dy = at.y - c.y;
                double dz = at.z - c.z;
                double d2 = dx * dx + dy * dy + dz * dz;
                if (d2 > far * far) {
                    continue;
                }
                ALL[found] = e;
                FAR[found] = d2;
                found++;
            }
            int n = Math.min(found, MOST);
            if (found > MOST) {
                // the nearest
                for (int i = 0; i < found; i++) {
                    ORDER[i] = i;
                }
                Arrays.sort(ORDER, 0, found, (a, b) -> Double.compare(FAR[a], FAR[b]));
            }
            for (int k = 0; k < n; k++) {
                Entity e = ALL[found > MOST ? ORDER[k] : k];
                Vec3 at = e.getPosition(partial);
                Vec3 now = e.position();
                // the box where the frame shows the body (between its ticks' places)
                AABB box = e.getBoundingBox().move(at.x - now.x, at.y - now.y, at.z - now.z);
                Vec3 look = e.getViewVector(partial);
                INTS[k * 3] = e.getId();
                INTS[k * 3 + 1] = kindOf(e, player, mc);
                INTS[k * 3 + 2] = (e == aimed ? AIMED : 0) | (e instanceof LivingEntity ? 0 : NO_EYES);
                int o = k * 12;
                FLOATS[o] = (float) (box.minX - c.x);
                FLOATS[o + 1] = (float) (box.minY - c.y);
                FLOATS[o + 2] = (float) (box.minZ - c.z);
                FLOATS[o + 3] = (float) (box.maxX - c.x);
                FLOATS[o + 4] = (float) (box.maxY - c.y);
                FLOATS[o + 5] = (float) (box.maxZ - c.z);
                FLOATS[o + 6] = (float) (at.y + e.getEyeHeight() - c.y);
                FLOATS[o + 7] = (float) look.x;
                FLOATS[o + 8] = (float) look.y;
                FLOATS[o + 9] = (float) look.z;
                if (e instanceof LivingEntity living) {
                    FLOATS[o + 10] = living.getMaxHealth() > 0.0F ? Math.clamp(living.getHealth() / living.getMaxHealth(), 0.0F, 1.0F) : 1.0F;
                    FLOATS[o + 11] = living.hurtTime > 0 ? living.hurtTime / (float) Math.max(1, living.hurtDuration) : 0.0F;
                } else {
                    FLOATS[o + 10] = 1.0F;
                    FLOATS[o + 11] = 0.0F;
                }
            }
            if (LOG && System.nanoTime() - logged > 2_000_000_000L) {
                logged = System.nanoTime();
                for (int k = 0; k < n; k++) {
                    int o = k * 12;
                    org.slf4j.LoggerFactory.getLogger("Hitboxes").info("[hitboxes] {} kind {} lo {} {} {} hi {} {} {} eye {} look {} {} {}", ALL[found > MOST ? ORDER[k] : k], INTS[k * 3 + 1],
                            FLOATS[o], FLOATS[o + 1], FLOATS[o + 2], FLOATS[o + 3], FLOATS[o + 4], FLOATS[o + 5], FLOATS[o + 6], FLOATS[o + 7], FLOATS[o + 8], FLOATS[o + 9]);
                }
            }
            Arrays.fill(ALL, 0, found, null);
            sent = true;
            send(main, depth, n);
        } catch (Throwable t) {
            broken = true;
            System.err.println("[cursed] Hitboxes are off for this launch: " + t);
        }
    }

    private static void send(RenderTarget main, @Nullable GpuTexture depth, int n) {
        EngineFrame.hitboxes(main.getColorTexture(), depth != null ? depth : main.getDepthTexture(), INTS, FLOATS, n);
    }
}
