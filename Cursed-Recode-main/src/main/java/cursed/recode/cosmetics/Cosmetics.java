package cursed.recode.cosmetics;

import cursed.recode.render.EngineFrame;
import cursed.recode.render.Native;
import net.minecraft.client.Minecraft;
import net.minecraft.client.player.LocalPlayer;
import net.minecraft.core.BlockPos;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.entity.projectile.arrow.AbstractArrow;
import net.minecraft.world.item.BowItem;
import net.minecraft.world.item.CrossbowItem;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.LightLayer;

/**
 * Cosmetics in the hands (native/src/cosmetics/: what is shown, its animations, its drawing, a rifle's scope and kick —
 * all there): the facts of the game handed over — the main hand's item each tick, the swing, the bow drawn and let go,
 * each frame the view, the walk and the light — and the game's own hand left out while the hands are the DLL's; the
 * rifle's field of view, camera kick and mouse speed taken back.
 */
public final class Cosmetics {
    private static String held = "";
    /** Ticks to the next telling of the game's controls (native/src/gui/cosmkeys.rs). */
    private static int keysIn;
    private static boolean dirSaid;
    private static boolean useDown;
    private static float lastVolume = -1.0F;
    /** A rifle's scope up (the bow drawn, the crossbow charging or loaded), as last told. */
    private static boolean aiming;
    /** This frame's camera kick (pitch, yaw), the mouse's speed, whether scoped — asked once a frame. */
    private static final float[] KICK = new float[2];
    private static float sensitivity = 1.0F;
    private static boolean scoped;
    /** The game times of the last shots: the arrows they let fly are not drawn. */
    private static final long[] SHOTS = new long[8];
    private static int shotAt;

    private Cosmetics() {
    }

    /**
     * The game's controls told (each its name, its key by the client's name — none when unbound or a mouse button —
     * its label in the game's language); the ones native moves off CS's inspect key, or gives back, set and saved.
     */
    private static void controls(Minecraft mc) {
        StringBuilder b = new StringBuilder();
        for (net.minecraft.client.KeyMapping k : mc.options.keyMappings) {
            var key = com.mojang.blaze3d.platform.InputConstants.getKey(k.saveString());
            String name = !k.isUnbound() && key.getType() == com.mojang.blaze3d.platform.InputConstants.Type.KEYBOARD ? cursed.recode.gui.ClickGui.keyName(key.getValue()) : "";
            if (!b.isEmpty()) {
                b.append('\u001e');
            }
            b.append(k.getName()).append('\u001f').append(name).append('\u001f').append(net.minecraft.network.chat.Component.translatable(k.getName()).getString());
        }
        String set = Native.guiCall("gamekeys\t" + b);
        if (set == null || set.isEmpty()) {
            return;
        }
        for (String line : set.split("\n")) {
            String[] f = line.split("\t", 2);
            net.minecraft.client.KeyMapping k = net.minecraft.client.KeyMapping.get(f[0]);
            if (k == null) {
                continue;
            }
            int code = f.length > 1 && !f[1].isEmpty() ? org.lwjgl.sdl.SDLKeyboard.SDL_GetScancodeFromName(f[1]) : 0;
            k.setKey(code > 0 ? com.mojang.blaze3d.platform.InputConstants.Type.KEYBOARD.getOrCreate(code) : com.mojang.blaze3d.platform.InputConstants.UNKNOWN);
        }
        net.minecraft.client.KeyMapping.resetMapping();
        mc.options.save();
    }

    /** Each game tick (MinecraftMixin). */
    public static void tick(Minecraft mc) {
        if (!dirSaid) {
            dirSaid = true;
            Native.guiCall("cosm\tdir\t" + mc.gameDirectory.toPath().resolve("cursed-recode").resolve("cosmetics"));
        }
        LocalPlayer p = mc.player;
        if (p != null && mc.level != null) {
            cursed.recode.hud.HudLayer.watch(mc);
        }
        // in a world, a few seconds in (it has loaded: the note of a control moved is seen)
        if (mc.player == null) {
            keysIn = 60;
        } else if (--keysIn <= 0) {
            keysIn = 20;
            controls(mc);
        }
        // the main hand's item, the off hand's (shown when the main hand is empty), which side the main hand is
        String said = p == null ? "" : BuiltInRegistries.ITEM.getKey(p.getMainHandItem().getItem()) + "\t"
                + BuiltInRegistries.ITEM.getKey(p.getOffhandItem().getItem()) + "\t"
                + (mc.options.mainHand().get() == net.minecraft.world.entity.HumanoidArm.LEFT ? "1" : "0");
        if (!said.equals(held)) {
            held = said;
            Native.guiCall("cosm\theld\t" + said);
        }
        // CS's sounds at the game's volume (as its players' are)
        float volume = mc.options.getSoundSourceVolume(net.minecraft.sounds.SoundSource.MASTER) * mc.options.getSoundSourceVolume(net.minecraft.sounds.SoundSource.PLAYERS);
        if (volume != lastVolume) {
            lastVolume = volume;
            Native.guiCall("cosm\tvolume\t" + volume);
        }
        // the right button pressed: a knife's heavy strike (only the look of it: the game's own use goes on)
        boolean use = p != null && mc.gui.screen() == null && mc.options.keyUse.isDown();
        if (LOG && use != useDown) {
            com.mojang.logging.LogUtils.getLogger().info("[cosm] use key {} (using {})", use, p != null && p.isUsingItem());
        }
        if (use && !useDown) {
            event(struck(mc) ? "heavy hit" : "heavy");
        }
        useDown = use;
        // a rifle's scope: the bow drawn; the crossbow charging, or loaded
        boolean aim = false;
        if (p != null) {
            ItemStack using = p.isUsingItem() ? p.getUseItem() : ItemStack.EMPTY;
            ItemStack main = p.getMainHandItem();
            aim = using.getItem() instanceof BowItem || using.getItem() instanceof CrossbowItem
                    || (main.getItem() instanceof CrossbowItem && CrossbowItem.isCharged(main));
        }
        if (aim != aiming) {
            aiming = aim;
            event(aim ? "use" : "unuse");
        }
    }

    /** Whether the crosshair is on something a blade would strike (a creature, a block): CS's hit clips then. */
    public static boolean struck(Minecraft mc) {
        var hit = mc.hitResult;
        return hit != null && hit.getType() != net.minecraft.world.phys.HitResult.Type.MISS;
    }

    /** The stand's log of the hands' events (CURSED_DEV_COSM_LOG). */
    private static final boolean LOG = cursed.recode.dev.Dev.on("CURSED_DEV_COSM_LOG");

    /** What the hands do (swing, heavy, use, unuse, inspect, shoot, zoom). */
    public static void event(String name) {
        if (LOG) {
            com.mojang.logging.LogUtils.getLogger().info("[cosm] {}", name);
        }
        Native.guiCall("cosm\tevent\t" + name);
    }

    /** The item in a hand about to be used (CosmeticsUseMixin): -1 not now (a rifle's bolt still coming back); a
     *  loaded crossbow's shot told. */
    public static int use(Player player, InteractionHand hand) {
        if (hand != InteractionHand.MAIN_HAND || !(player instanceof LocalPlayer)) {
            return 0;
        }
        ItemStack stack = player.getItemInHand(hand);
        boolean rifle = stack.getItem() instanceof BowItem || stack.getItem() instanceof CrossbowItem;
        if (!rifle) {
            return 0;
        }
        String ready = Native.guiCall("cosm\tready");
        if (LOG) {
            com.mojang.logging.LogUtils.getLogger().info("[cosm] use {} ready {}", stack, ready);
        }
        if (!"1".equals(ready)) {
            return -1;
        }
        if (stack.getItem() instanceof CrossbowItem && CrossbowItem.isCharged(stack)) {
            shot(player);
        }
        return 0;
    }

    /** The use key let go (CosmeticsUseMixin): a bow drawn long enough to let an arrow fly — the shot. */
    public static void released(Player player) {
        if (player instanceof LocalPlayer && player.getUseItem().getItem() instanceof BowItem && player.getTicksUsingItem() >= 3) {
            shot(player);
        }
    }

    private static void shot(Player player) {
        // the tracer's ends: the muzzle (in the scope, just under the eye), the first block on the way (or 200 on)
        float partial = Minecraft.getInstance().getDeltaTracker().getGameTimeDeltaPartialTick(true);
        var eye = player.getEyePosition(partial);
        var look = player.getViewVector(partial);
        var right = look.cross(new net.minecraft.world.phys.Vec3(0, 1, 0)).normalize();
        var up = right.cross(look).normalize();
        var from = scoped ? eye.add(look.scale(0.5)).subtract(up.scale(0.06)) : eye.add(look.scale(0.6)).add(right.scale(0.14)).subtract(up.scale(0.12));
        var hit = player.pick(200.0, partial, false);
        var to = hit.getType() == net.minecraft.world.phys.HitResult.Type.MISS ? eye.add(look.scale(200.0)) : hit.getLocation();
        String shoot = "shoot " + from.x + " " + from.y + " " + from.z + " " + to.x + " " + to.y + " " + to.z;
        if (LOG) {
            com.mojang.logging.LogUtils.getLogger().info("[cosm] {}", shoot);
        }
        // a CS rifle's shot only: all of it off, or the game's own bow in the hands — its arrow as ever
        if (!"1".equals(Native.guiCall("cosm\tevent\t" + shoot))) {
            return;
        }
        SHOTS[shotAt++ % SHOTS.length] = player.level().getGameTime();
        // the creature on the shot's line, the one fought now (it dying soon after is a kill: the HUD's, StatTrak's)
        var box = player.getBoundingBox().expandTowards(to.subtract(eye)).inflate(1.0);
        var met = net.minecraft.world.entity.projectile.ProjectileUtil.getEntityHitResult(player, eye, to, box, e -> !e.isSpectator() && e.isPickable(), eye.distanceToSqr(to));
        if (met != null) {
            cursed.recode.hud.HudLayer.attacked(met.getEntity());
        }
    }

    /** A kill (HudVars: the one fought died after the player's hit or shot): StatTrak counts it for what is in the hands. */
    public static void kill() {
        Native.guiCall("cosm\tkill");
    }

    /** Whether an entity is not drawn (CosmeticsArrowMixin): the player's own arrow, let fly by a rifle's shot. */
    public static boolean hidden(Entity e) {
        // (the client's own: in a world of one's own the server's arrow is the same class, its crits are not ours)
        if (!(e instanceof AbstractArrow arrow) || shotAt == 0 || !e.level().isClientSide()) {
            return false;
        }
        Minecraft mc = Minecraft.getInstance();
        long born = e.level().getGameTime() - arrow.tickCount;
        Entity owner = arrow.getOwner();
        if (mc.player == null || owner == null || owner.getId() != mc.player.getId()) {
            return false;
        }
        for (long s : SHOTS) {
            if (s != 0 && born >= s - 1 && born <= s + 10) {
                return true;
            }
        }
        return false;
    }

    /** The field of view (Camera.calculateFov): a rifle's scope's, the game's own else. */
    public static float fov(float base) {
        String r = Native.guiCall("cosm\tfov\t" + base);
        try {
            return r.isEmpty() ? base : Float.parseFloat(r);
        } catch (NumberFormatException e) {
            return base;
        }
    }

    /** The camera's kick this frame (Camera.alignWithEntity, before the field of view): pitch and yaw in degrees; the
     *  mouse's speed and the scope asked with it. */
    public static float[] kick() {
        String r = Native.guiCall("cosm\tcamera");
        String[] f = r.split(" ");
        try {
            KICK[0] = f.length > 0 && !f[0].isEmpty() ? Float.parseFloat(f[0]) : 0.0F;
            KICK[1] = f.length > 1 ? Float.parseFloat(f[1]) : 0.0F;
            sensitivity = f.length > 2 ? Float.parseFloat(f[2]) : 1.0F;
            scoped = f.length > 3 && "1".equals(f[3]);
        } catch (NumberFormatException e) {
            KICK[0] = KICK[1] = 0.0F;
            sensitivity = 1.0F;
            scoped = false;
        }
        return KICK;
    }

    /** The mouse's speed in a scope (as told with the frame's kick). */
    public static double sensitivity() {
        return sensitivity;
    }

    /** Whether a rifle's scope is up (the wheel is its zoom then). */
    public static boolean scoped() {
        return scoped;
    }

    /** Whether the game's crosshair gives way (HudMixin): CS's is drawn (a CS knife in the hands), or a scope is up. */
    public static boolean crosshair() {
        return scoped || "1".equals(Native.guiCall("cosm\tcrosshair"));
    }

    /** The player seen from behind (F5): the hold where the game would draw the item CS shows (from the camera),
     *  caught as the level's bodies are given (CosmeticsThirdItemMixin); whether one was this frame; whether the level's
     *  bodies are being given now (not a picture of the player in a menu). */
    private static final java.util.List<float[]> THIRDS = new java.util.ArrayList<>();
    private static boolean inLevel;
    private static boolean extracting;

    /** The level's bodies begin (true) or end being given (CosmeticsThirdLevelMixin). */
    public static void level(boolean in) {
        inLevel = in;
        if (in) THIRDS.clear();
    }

    /** The level's bodies begin (true) or end being read off the world (CosmeticsThirdExtractMixin). */
    public static void extracting(boolean in) {
        extracting = in;
    }

    /** Which arm CS's item is in, for the player from behind (none: CS shows nothing, or the view is the player's own);
     *  `rifle` when only a rifle counts. */
    private static net.minecraft.world.entity.@org.jspecify.annotations.Nullable HumanoidArm thirdArm(Minecraft mc, boolean rifle, boolean doublePlayer) {
        if (mc.player == null || (!doublePlayer && mc.options.getCameraType().isFirstPerson()) || !EngineFrame.active()) {
            return null;
        }
        String h = Native.guiCall("cosm\tthird");
        if (h == null || h.isEmpty() || (rifle && !h.endsWith("rifle"))) {
            return null;
        }
        net.minecraft.world.entity.HumanoidArm main = mc.player.getMainArm();
        return h.startsWith("off") ? main.getOpposite() : main;
    }

    /**
     * The game about to draw the item in a hand (CosmeticsThirdItemMixin): the player's own seen from behind, the arm CS
     * shows its item in — its hold taken (the engine draws CS's item there) and true: the game's own left out.
     */
    public static boolean third(net.minecraft.client.renderer.entity.state.ArmedEntityRenderState state, net.minecraft.world.entity.HumanoidArm arm, com.mojang.blaze3d.vertex.PoseStack pose) {
        if (!inLevel || !(state instanceof net.minecraft.client.renderer.entity.state.AvatarRenderState a)) {
            return false;
        }
        Minecraft mc = Minecraft.getInstance();
        boolean doublePlayer = cursed.recode.fx.FakePlayers.usesYourCosmetics(a.id);
        if (mc.player == null || (a.id != mc.player.getId() && !doublePlayer) || thirdArm(mc, false, doublePlayer) != arm) {
            return false;
        }
        float[] matrix = new float[16];
        pose.last().pose().get(matrix);
        THIRDS.add(matrix);
        return true;
    }

    /** The player's own arms from behind (CosmeticsThirdPoseMixin), in the level (not a picture of the player in a
     *  menu): a CS rifle held as CS's agents hold theirs — both hands on it, ahead (the game's loaded crossbow's hold);
     *  drawn back, the bow's own. */
    public static void thirdPose(Entity entity, net.minecraft.client.renderer.entity.state.AvatarRenderState state) {
        Minecraft mc = Minecraft.getInstance();
        boolean doublePlayer = cursed.recode.fx.FakePlayers.usesYourCosmetics(entity.getId());
        if (!extracting || (entity != mc.player && !doublePlayer) || state.isUsingItem) {
            return;
        }
        var arm = thirdArm(mc, true, doublePlayer);
        if (arm == net.minecraft.world.entity.HumanoidArm.RIGHT) {
            state.rightArmPose = net.minecraft.client.model.HumanoidModel.ArmPose.CROSSBOW_HOLD;
        } else if (arm == net.minecraft.world.entity.HumanoidArm.LEFT) {
            state.leftArmPose = net.minecraft.client.model.HumanoidModel.ArmPose.CROSSBOW_HOLD;
        }
    }

    /** The level drawn (GameRendererMixin, before it is made whole): CS's item at the hold taken this frame, into the
     *  level's own picture and depth. */
    public static void thirdDraw(com.mojang.blaze3d.pipeline.RenderTarget main) {
        if (THIRDS.isEmpty()) {
            return;
        }
        var poses = new java.util.ArrayList<>(THIRDS);
        THIRDS.clear();
        Minecraft mc = Minecraft.getInstance();
        LocalPlayer p = mc.player;
        if (p == null || mc.level == null) {
            return;
        }
        var level = cursed.recode.engine.Upscale.target();
        var target = level != null ? level : main;
        if (target.getDepthTexture() == null) {
            return;
        }
        BlockPos at = BlockPos.containing(p.getEyePosition(mc.getDeltaTracker().getGameTimeDeltaPartialTick(true)));
        float sky = mc.level.getBrightness(LightLayer.SKY, at) / 15.0F;
        float block = mc.level.getBrightness(LightLayer.BLOCK, at) / 15.0F;
        for (float[] matrix : poses) EngineFrame.third(target.getColorTexture(), target.getDepthTexture(), matrix, sky, block);
    }

    /** The hands this frame (GameRendererMixin, where the game draws its hand): true when the DLL draws them. */
    public static boolean hands(com.mojang.blaze3d.pipeline.RenderTarget main) {
        Minecraft mc = Minecraft.getInstance();
        LocalPlayer p = mc.player;
        if (p == null || mc.level == null || !EngineFrame.active() || mc.options.getCameraType() != net.minecraft.client.CameraType.FIRST_PERSON
                || !"1".equals(Native.guiCall("cosm\tactive"))) {
            return false;
        }
        float partial = mc.getDeltaTracker().getGameTimeDeltaPartialTick(true);
        BlockPos eye = BlockPos.containing(p.getEyePosition(partial));
        float sky = mc.level.getBrightness(LightLayer.SKY, eye) / 15.0F;
        float block = mc.level.getBrightness(LightLayer.BLOCK, eye) / 15.0F;
        var v = p.getDeltaMovement();
        int flags = (p.onGround() ? 1 : 0) | (p.isSprinting() ? 2 : 0) | (p.isShiftKeyDown() ? 4 : 0);
        EngineFrame.viewmodel(main.getColorTexture(), p.getViewYRot(partial), p.getViewXRot(partial), (float) v.x, (float) v.y, (float) v.z, flags, sky, block);
        return true;
    }
}
