package cursed.recode.fx;

import com.mojang.authlib.GameProfile;
import cursed.recode.render.Native;
import java.nio.charset.StandardCharsets;
import java.util.HashMap;
import java.util.Locale;
import java.util.Map;
import java.util.UUID;
import net.minecraft.client.Minecraft;
import net.minecraft.client.player.LocalPlayer;
import net.minecraft.client.player.AbstractClientPlayer;
import net.minecraft.client.player.RemotePlayer;
import net.minecraft.sounds.SoundEvents;
import net.minecraft.sounds.SoundSource;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EquipmentSlot;
import net.minecraft.world.entity.Pose;
import net.minecraft.world.phys.BlockHitResult;
import net.minecraft.world.phys.HitResult;
import net.minecraft.world.phys.Vec3;

/**
 * Fake Player's doubles (native gui/fakeplayer.rs says where they are): players of the client's own, in the world only
 * the player sees — put where they are said to be each tick, the player's armour and hands on them when asked, the
 * skin the player's (FakeSkinMixin) or the nick's. A blow at one never leaves the game: it is told to native (thrown
 * back there) and the double flinches and cries out here.
 */
public final class FakePlayers {
    /** Their ids are the client's own, far below the server's. */
    private static final int BASE = -77_000;
    private static final Map<Integer, RemotePlayer> DOUBLES = new HashMap<>();
    private static Map<String, String> set = Map.of();
    private static long polled;
    /** The player's swing seen last (a new one is a swing begun). */
    private static Object lastSwing;
    private static final Map<Integer, float[]> STUDIO = new HashMap<>();

    public static float[] studio(Entity entity) {
        int n = number(entity);
        return n < 0 ? null : STUDIO.get(n);
    }

    /** Apply the angles decided by the native editor, to the player and its armour alike. */
    public static void studioModel(net.minecraft.client.model.HumanoidModel<?> model, net.minecraft.client.renderer.entity.state.HumanoidRenderState state) {
        if (!(state instanceof cursed.recode.motion.StudioPose holder)) return;
        float[] p = holder.cursed$studio();
        if (p == null || p.length != 18) return;
        net.minecraft.client.model.geom.ModelPart[] parts = {model.body, model.head, model.rightArm, model.leftArm, model.rightLeg, model.leftLeg};
        for (int i = 0; i < parts.length; i++) {
            parts[i].xRot = p[i * 3]; parts[i].yRot = p[i * 3 + 1]; parts[i].zRot = p[i * 3 + 2];
        }
        model.hat.xRot = model.head.xRot; model.hat.yRot = model.head.yRot; model.hat.zRot = model.head.zRot;
    }

    private FakePlayers() {
    }

    /** The native studio's selected live double, with the same skin and pose as in the world. */
    public static void preview(net.minecraft.client.gui.GuiGraphicsExtractor graphics, float scale) {
        String info = Native.guiCall("fake-preview");
        if (info == null || info.isEmpty()) return;
        String[] fields = info.split(" ");
        if (fields.length != 10) return;
        float[] v = new float[10];
        try { for (int i = 0; i < v.length; i++) v[i] = Float.parseFloat(fields[i]); }
        catch (NumberFormatException e) { return; }
        RemotePlayer doublePlayer = DOUBLES.get((int) v[0]);
        if (doublePlayer == null || v[3] < 1.0F || v[4] < 0.01F) return;
        graphics.enableScissor(Math.round(v[5] / scale), Math.round(v[6] / scale), Math.round(v[7] / scale), Math.round(v[8] / scale));
        cursed.recode.menus.ItemStage.MENUS.addEntity(graphics, doublePlayer, v[1], v[2],
                v[3], v[9], 6.0F, v[4], 0xffffff);
        graphics.disableScissor();
    }

    /** Whether an entity is a double; its number. */
    public static int number(Entity e) {
        int id = e.getId();
        return id <= BASE && id > BASE - 64 && DOUBLES.containsKey(BASE - id) ? BASE - id : -1;
    }

    public static boolean isDoubleId(int id) { return id <= BASE && id > BASE - 64 && DOUBLES.containsKey(BASE - id); }

    /** Only doubles holding the same items use the player's current CS loadout. Other outfits
     * keep their own vanilla items rather than showing a cosmetic for a different weapon. */
    public static boolean usesYourCosmetics(int id) {
        Minecraft mc = Minecraft.getInstance();
        RemotePlayer d = DOUBLES.get(BASE - id);
        return d != null && mc.player != null
                && net.minecraft.world.item.ItemStack.isSameItem(d.getMainHandItem(), mc.player.getMainHandItem())
                && net.minecraft.world.item.ItemStack.isSameItem(d.getOffhandItem(), mc.player.getOffhandItem());
    }

    /** The nick whose skin the doubles wear ("" the player's). */
    public static String nick() {
        return set.getOrDefault("Nickname", "").trim();
    }

    private static final Map<String, net.minecraft.world.entity.player.PlayerSkin> SKINS = new java.util.concurrent.ConcurrentHashMap<>();
    private static final java.util.Set<String> ASKED_SKINS = java.util.concurrent.ConcurrentHashMap.newKeySet();

    /** A separate lookup per nickname: several doubles must not repeatedly cancel each other's skins. */
    public static net.minecraft.world.entity.player.@org.jspecify.annotations.Nullable PlayerSkin skin(Minecraft mc, AbstractClientPlayer doublePlayer) {
        String n = doublePlayer.getGameProfile().name();
        if (n.isEmpty() || n.equalsIgnoreCase(mc.getUser().getName())) return mc.player != null ? mc.player.getSkin() : null;
        String key = n.toLowerCase(Locale.ROOT);
        if (ASKED_SKINS.size() < 64 && ASKED_SKINS.add(key)) {
            Thread t = new Thread(() -> {
                try {
                    mc.services().profileResolver().fetchByName(n).ifPresent(gp -> mc.getSkinManager().get(gp)
                            .thenAccept(o -> o.ifPresent(sk -> SKINS.put(key, sk))));
                } catch (RuntimeException e) {
                    // No such nick or no network: leave the game's default skin.
                }
            }, "Cursed double's skin");
            t.setDaemon(true); t.start();
        }
        return SKINS.get(key);
    }

    /** Each game tick (MinecraftMixin): the player told, the doubles put where they are said to be. */
    public static void tick(Minecraft mc) {
        LocalPlayer p = mc.player;
        if (p == null || mc.level == null) {
            DOUBLES.clear();
            STUDIO.clear();
            return;
        }
        long now = System.nanoTime();
        if (now - polled > 250_000_000L) {
            polled = now;
            set = Fx.settings("fake-player");
        }
        if (!"1".equals(set.get("on")) && DOUBLES.isEmpty()) {
            // (asked once a second while off: it says when to clear what is left)
            if (p.tickCount % 20 != 0) {
                return;
            }
        }
        // where the player looks: the block's top within six, else three ahead
        Vec3 look;
        HitResult hit = mc.hitResult;
        if (hit instanceof BlockHitResult b && hit.getType() == HitResult.Type.BLOCK && b.getLocation().distanceTo(p.position()) < 6.0) {
            look = Vec3.atBottomCenterOf(b.getBlockPos().relative(b.getDirection()));
        } else {
            Vec3 f = Vec3.directionFromRotation(0.0F, p.getYRot());
            look = p.position().add(f.scale(3.0));
        }
        var swing = p.getCurrentSwing();
        boolean began = swing != null && swing != lastSwing;
        lastSwing = swing;
        String facts = String.format(Locale.ROOT, "%.3f %.3f %.3f %.1f %.1f %.1f %d %d | %.3f %.3f %.3f", p.getX(), p.getY(), p.getZ(),
                p.getYRot(), p.getXRot(), p.yBodyRot, p.isShiftKeyDown() ? 1 : 0, began ? 1 : 0, look.x, look.y, look.z);
        String said = Native.guiCall("fake\ttick\t" + facts);
        if (said == null || said.isEmpty()) {
            return;
        }
        String[] actions = said.split("\n");
        int wearing = -1;
        for (String action : actions) {
            if (action.startsWith("fakewear ")) wearing = Integer.parseInt(action.substring(9));
        }
        for (String l : actions) {
            if (l.equals("fakeclear")) {
                clear(mc);
                continue;
            }
            String[] f = l.split(" ");
            if (f.length >= 3 && f[0].equals("fakepose")) {
                int n = Integer.parseInt(f[1]);
                if (f[2].equals("live")) STUDIO.remove(n);
                else if (f.length == 20) {
                    float[] angles = new float[18];
                    for (int i = 0; i < angles.length; i++) angles[i] = Float.parseFloat(f[i + 2]);
                    STUDIO.put(n, angles);
                }
                continue;
            }
            if (f.length < 10 || !f[0].equals("fake")) {
                continue;
            }
            int n = Integer.parseInt(f[1]);
            RemotePlayer d = DOUBLES.computeIfAbsent(n, k -> make(mc, k));
            if (d == null) {
                continue;
            }
            float yaw = Float.parseFloat(f[5]);
            float pitch = Float.parseFloat(f[6]);
            d.moveOrInterpolateTo(new Vec3(Double.parseDouble(f[2]), Double.parseDouble(f[3]), Double.parseDouble(f[4])), yaw, pitch);
            d.lerpHeadTo(yaw, 3);
            d.yBodyRot = Float.parseFloat(f[7]);
            boolean sneak = f[8].equals("1");
            d.setShiftKeyDown(sneak);
            d.setPose(sneak ? Pose.CROUCHING : Pose.STANDING);
            if (f[9].equals("1")) {
                d.swing(InteractionHand.MAIN_HAND, d.getMainHandItem().getAttackAnimation(), false);
            }
            if (n == wearing) {
                d.setMainArm(p.getMainArm());
                for (EquipmentSlot s : EquipmentSlot.values()) {
                    if (s.getType() == EquipmentSlot.Type.HUMANOID_ARMOR || s.getType() == EquipmentSlot.Type.HAND) {
                        d.setItemSlot(s, p.getItemBySlot(s).copy());
                    }
                }
            }
        }
    }

    private static RemotePlayer make(Minecraft mc, int n) {
        String nick = nick();
        String name = nick.isEmpty() ? mc.getUser().getName() : nick;
        UUID id = UUID.nameUUIDFromBytes(("cursed-double:" + n + ":" + name).getBytes(StandardCharsets.UTF_8));
        RemotePlayer d = new RemotePlayer(mc.level, new GameProfile(id, name.length() > 16 ? name.substring(0, 16) : name));
        d.setId(BASE - n);
        d.setMainArm(mc.player.getMainArm());
        d.snapTo(mc.player.getX(), mc.player.getY(), mc.player.getZ(), mc.player.getYRot(), 0.0F);
        for (EquipmentSlot slot : EquipmentSlot.values()) {
            if (slot.getType() == EquipmentSlot.Type.HUMANOID_ARMOR || slot.getType() == EquipmentSlot.Type.HAND) {
                d.setItemSlot(slot, mc.player.getItemBySlot(slot).copy());
            }
        }
        mc.level.addEntity(d);
        return d;
    }

    private static void clear(Minecraft mc) {
        for (RemotePlayer d : DOUBLES.values()) {
            if (mc.level != null) {
                mc.level.removeEntity(d.getId(), Entity.RemovalReason.DISCARDED);
            }
        }
        DOUBLES.clear();
        STUDIO.clear();
    }

    /** A blow at a double (MultiPlayerGameModeMixin: held back from the server): thrown back, a flinch, a cry. */
    public static void hit(Entity e) {
        Minecraft mc = Minecraft.getInstance();
        int n = number(e);
        if (n < 0 || mc.player == null || mc.level == null) {
            return;
        }
        RemotePlayer d = DOUBLES.get(n);
        Native.guiCall(String.format(Locale.ROOT, "fake\thit\t%d %.3f %.3f %.3f", n, mc.player.getX(), mc.player.getY(), mc.player.getZ()));
        d.animateHurt(mc.player.getYRot());
        d.hurtTime = d.hurtDuration = 10;
        mc.level.playLocalSound(d.getX(), d.getY(), d.getZ(), SoundEvents.PLAYER_HURT, SoundSource.PLAYERS, 1.0F, 1.0F, false);
        mc.player.swing(InteractionHand.MAIN_HAND, mc.player.getMainHandItem().getAttackAnimation(), false);
    }
}
