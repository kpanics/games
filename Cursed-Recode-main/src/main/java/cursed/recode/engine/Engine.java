package cursed.recode.engine;

import cursed.recode.dev.Dev;
import cursed.recode.render.EngineFrame;
import net.minecraft.client.Minecraft;
import net.minecraft.client.multiplayer.ClientLevel;
import net.minecraft.client.renderer.state.level.LevelRenderState;
import net.minecraft.client.renderer.state.level.SkyRenderState;
import net.minecraft.util.Mth;
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.level.dimension.DimensionType;

/**
 * The engine's frame, sent before the level is drawn (GameRendererMixin): where the camera is, the world's height,
 * where the sun and the moon are and how strong, the rain. Native: engine/gpu.rs.
 * CURSED_DEV_NOSHADOWS draws terrain with the engine's shaders but without shadows.
 */
public final class Engine {
    private static final boolean NO_SHADOWS = Dev.on("CURSED_DEV_NOSHADOWS");
    /** CURSED_DEV_NOLIGHT: no coloured light. */
    private static final boolean NO_LIGHT = Dev.on("CURSED_DEV_NOLIGHT");
    /** CURSED_DEV_NOREFLECT: water and glass without reflections. */
    private static final boolean NO_REFLECT = Dev.on("CURSED_DEV_NOREFLECT");
    /** CURSED_DEV_NOWATER: still water (no waves, no wakes, no rain on it). */
    private static final boolean NO_WATER = Dev.on("CURSED_DEV_NOWATER");
    /** CURSED_DEV_NOAIR: no light in the air (rays, the glow round the sun). */
    private static final boolean NO_AIR = Dev.on("CURSED_DEV_NOAIR");
    /** CURSED_DEV_NOSOFT: hard-edged shadows. */
    private static final boolean NO_SOFT = Dev.on("CURSED_DEV_NOSOFT");
    /** CURSED_DEV_NOGLOW: no glow round glowing blocks. */
    private static final boolean NO_GLOW = Dev.on("CURSED_DEV_NOGLOW");
    /** CURSED_DEV_TRACE=n: shadow rays go at most n blocks (for measuring). */
    private static final int TRACE = Dev.on("CURSED_DEV_TRACE") ? Integer.parseInt(System.getenv("CURSED_DEV_TRACE")) : 0;
    /** The share of the sky's light that stays in shade at noon. */
    private static final float SHADE = 0.62F;
    private static final float[] SUN = new float[4];
    private static final float[] SUN_COLOR = new float[4];
    private static final float[] MOON = new float[4];
    private static final float[] SKY = new float[4];
    /** The player in water: feet round the camera (xyz), how hard they push it (w, 0 when not in water). */
    private static final float[] MOVER = new float[4];
    /** The creatures and players that cast shadows: centre (from the camera's block) and half size, 6 floats each. */
    private static final int MOST_SHADOWS = 32;
    private static final float[] CASTERS = new float[MOST_SHADOWS * 6];
    private static final java.util.ArrayList<LivingEntity> NEAR = new java.util.ArrayList<>();
    private static net.minecraft.world.phys.@org.jspecify.annotations.Nullable Vec3 lastFeet;
    private static long lastFeetTime;

    private Engine() {
    }

    private static final boolean NO_CASTERS = Dev.on("CURSED_DEV_NOCASTERS");
    private static volatile boolean shadowsCast;
    private static float wetness;
    private static long lastWetTime;

    /** Whether creatures cast the engine's shadows now (shadows on, the sun or the moon up): EntityRendererMixin. */
    public static boolean castsShadows() {
        return shadowsCast;
    }
    private static final boolean NO_CLOUD_SHADOWS = Dev.on("CURSED_DEV_NOCLOUDSHADOWS");

    public static void frame(LevelRenderState state) {
        if (!EngineFrame.active()) {
            return;
        }
        ClientLevel level = Minecraft.getInstance().level;
        if (level == null) {
            return;
        }
        SkyRenderState sky = state.skyRenderState;
        boolean overworld = sky.skybox == DimensionType.Skybox.OVERWORLD;
        float rain = overworld ? Mth.clamp(1.0F - sky.rainBrightness, 0.0F, 1.0F) : 0.0F;
        // SkyRenderer: the sun is drawn at +y turned by its angle round x, all turned -90 degrees round y.
        float sx = -Mth.sin(sky.sunAngle);
        float sy = Mth.cos(sky.sunAngle);
        float sunStrength = overworld ? smooth(-0.02F, 0.12F, sy) : 0.0F;
        SUN[0] = sx;
        SUN[1] = sy;
        SUN[2] = 0.0F;
        SUN[3] = sunStrength;
        // Warm when low, white at noon.
        float noon = smooth(0.0F, 0.45F, sy);
        SUN_COLOR[0] = Mth.lerp(noon, 1.18F, 1.06F);
        SUN_COLOR[1] = Mth.lerp(noon, 0.82F, 1.0F);
        SUN_COLOR[2] = Mth.lerp(noon, 0.58F, 0.92F);
        SUN_COLOR[3] = SHADE;
        float mx = -Mth.sin(sky.moonAngle);
        float my = Mth.cos(sky.moonAngle);
        MOON[0] = mx;
        MOON[1] = my;
        MOON[2] = 0.0F;
        MOON[3] = overworld ? 0.45F * smooth(-0.02F, 0.12F, my) * (1.0F - sunStrength) : 0.0F;
        if (sky.skyColor != null) {
            SKY[0] = sky.skyColor.x();
            SKY[1] = sky.skyColor.y();
            SKY[2] = sky.skyColor.z();
        }
        SKY[3] = rain;
        var cam = state.cameraRenderState.pos;
        var player = Minecraft.getInstance().player;
        MOVER[3] = 0.0F;
        if (player != null && player.isInWater()) {
            var feet = player.getPosition(state.worldPartialTicks);
            MOVER[0] = (float) (feet.x - cam.x);
            MOVER[1] = (float) (feet.y - cam.y);
            MOVER[2] = (float) (feet.z - cam.z);
            // How fast the feet go through the water (blocks a second), from where they were last frame.
            long now = System.nanoTime();
            double dt = Math.max(1e-3, (now - lastFeetTime) / 1e9);
            double speed = lastFeet == null ? 0.0 : Math.hypot(feet.x - lastFeet.x, feet.z - lastFeet.z) / dt;
            lastFeet = feet;
            lastFeetTime = now;
            MOVER[3] = speed > 0.3 && speed < 40.0 ? (float) Math.min(1.5, speed * 0.25) : 0.0F;
        } else {
            lastFeet = null;
        }
        // What the chosen quality draws, less what a dev switch turns off; soft shadows only with the frames put together.
        int flags = EngineSettings.flags();
        int off = (NO_SHADOWS ? 1 | 64 : 0) | (NO_LIGHT ? 2 : 0) | (NO_REFLECT ? 4 : 0) | (NO_WATER ? 8 : 0) | (NO_AIR ? 32 : 0)
                | (NO_SOFT ? 64 : 0) | (NO_GLOW ? 128 : 0) | (Dev.on("CURSED_DEV_NOWIND") ? 512 : 0) | (Dev.on("CURSED_DEV_NOSKY") ? 1024 : 0);
        flags &= ~off;
        // Chunk Animator: the way sections come in (terrain.vsh; the engine's shaders only: none with its quality Off)
        if (flags != 0) {
            flags |= cursed.recode.fx.ChunkAnimator.flags();
        }
        if (Upscale.target() == null) {
            flags &= ~64;
        }
        if (Dev.on("CURSED_DEV_WATERDEBUG")) {
            flags |= 16;
        }
        if (TRACE > 0) {
            flags |= TRACE << 16;
        }
        if (Dev.on("CURSED_DEV_FLATSHADE")) {
            flags |= 1 << 28;
        }
        int casters = (flags & 1) != 0 && !NO_CASTERS ? casters(level, cam, state.worldPartialTicks) : 0;
        shadowsCast = (flags & 1) != 0 && !NO_CASTERS && Math.max(SUN[3], MOON[3]) * (1.0F - 0.85F * rain) > 0.05F;
        // The clouds as CloudRenderer moves them: 0.03 blocks a tick along x, the map (256 cells of 12 blocks) round.
        boolean clouds = Minecraft.getInstance().options.getCloudStatus() != net.minecraft.client.CloudStatus.OFF
                && net.minecraft.util.ARGB.alpha(state.cloudColor) > 0 && !NO_CLOUD_SHADOWS;
        float drift = ((float) (state.gameTime % (256 * 400L)) + state.worldPartialTicks) * 0.030000001F;
        // The ground gets wet over half a minute of rain where it rains (not in deserts, not in snow) and dries over two.
        long now = System.nanoTime();
        float dt = lastWetTime == 0L ? 0.0F : Math.min((now - lastWetTime) / 1e9F, 0.25F);
        lastWetTime = now;
        boolean raining = rain > 0.05F && level.getPrecipitationAt(net.minecraft.core.BlockPos.containing(cam)) == net.minecraft.world.level.biome.Biome.Precipitation.RAIN;
        wetness = raining ? Math.min(rain, wetness + dt / 30.0F) : Math.max(0.0F, wetness - dt / 120.0F);
        EngineFrame.send(cam.x, cam.y, cam.z, level.getMinY(), level.getHeight(), SUN, SUN_COLOR, MOON, SKY,
                (float) (state.gameTime % 24000L) + state.worldPartialTicks, flags, Upscale.scale(), MOVER, CASTERS, casters,
                drift, state.cloudHeight, clouds, wetness);
    }

    /**
     * Creatures and players within 40 blocks, the nearest 32, into CASTERS: each an ellipsoid in its box, its shadow
     * traced with the blocks' (trace.glsl). The player's own too, also in first person.
     */
    private static int casters(ClientLevel level, net.minecraft.world.phys.Vec3 cam, float partial) {
        NEAR.clear();
        for (net.minecraft.world.entity.Entity e : level.entitiesForRendering()) {
            if (e instanceof LivingEntity living && !e.isInvisible() && !e.isSpectator() && e.distanceToSqr(cam) < 40.0 * 40.0) {
                NEAR.add(living);
            }
        }
        if (NEAR.size() > MOST_SHADOWS) {
            NEAR.sort(java.util.Comparator.comparingDouble(e -> e.distanceToSqr(cam)));
        }
        double bx = Math.floor(cam.x);
        double by = Math.floor(cam.y);
        double bz = Math.floor(cam.z);
        int n = Math.min(NEAR.size(), MOST_SHADOWS);
        for (int i = 0; i < n; i++) {
            LivingEntity e = NEAR.get(i);
            var at = e.getPosition(partial);
            float hw = e.getBbWidth() * 0.5F;
            float hh = e.getBbHeight() * 0.5F;
            CASTERS[i * 6] = (float) (at.x - bx);
            CASTERS[i * 6 + 1] = (float) (at.y - by) + hh;
            CASTERS[i * 6 + 2] = (float) (at.z - bz);
            CASTERS[i * 6 + 3] = hw * 0.85F;
            CASTERS[i * 6 + 4] = hh;
            CASTERS[i * 6 + 5] = hw * 0.85F;
        }
        return n;
    }

    private static float smooth(float a, float b, float x) {
        float t = Mth.clamp((x - a) / (b - a), 0.0F, 1.0F);
        return t * t * (3.0F - 2.0F * t);
    }
}
