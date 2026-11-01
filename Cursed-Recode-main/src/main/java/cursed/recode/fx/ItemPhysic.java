package cursed.recode.fx;

import com.mojang.blaze3d.vertex.PoseStack;
import cursed.recode.mixin.client.ItemPickupParticleAccess;
import cursed.recode.render.Native;
import it.unimi.dsi.fastutil.ints.IntList;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.Iterator;
import java.util.List;
import java.util.Map;
import java.util.Random;
import net.minecraft.client.Minecraft;
import net.minecraft.client.multiplayer.ClientLevel;
import net.minecraft.client.particle.ItemPickupParticle;
import net.minecraft.client.renderer.Lightmap;
import net.minecraft.client.renderer.SubmitNodeCollector;
import net.minecraft.client.renderer.entity.state.EntityRenderState;
import net.minecraft.client.renderer.entity.state.ItemEntityRenderState;
import net.minecraft.client.renderer.texture.OverlayTexture;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.component.DataComponents;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.sounds.SoundEvent;
import net.minecraft.sounds.SoundEvents;
import net.minecraft.sounds.SoundSource;
import net.minecraft.tags.FluidTags;
import net.minecraft.tags.ItemTags;
import net.minecraft.util.LightCoordsUtil;
import net.minecraft.util.Mth;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.entity.item.ItemEntity;
import net.minecraft.world.item.BlockItem;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.level.block.HopperBlock;
import net.minecraft.world.level.block.SoundType;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.material.FluidState;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.shapes.Shapes;
import net.minecraft.world.phys.shapes.VoxelShape;
import org.joml.Quaternionf;

/**
 * Item Physic (the menu's "item-physic", Visual · Effects): dropped items as bodies (native itemphysic.rs works them
 * out). As the game gathers an item's state: what it is like (flat, solid, round, long) and made of (soft, wood, stone,
 * metal, glass), how fast it goes, the ground's real tops round it (a 5 × 5 grid of the blocks' collision tops, kept a
 * tick), the water's top, how slippery the block under it is, a walker going through; its copies' poses come back, and
 * what happened: a landing (the dust of the block it fell on, a sound of its stuff, or the block's own if it is one), a
 * splash (drops and a ring), afloat (rings now and then), in lava or fire (smoke, embers; burnt up: a burst). Drawn: each
 * copy where its body is.
 */
public final class ItemPhysic {
    private static final String MODULE = "item-physic";
    private static final int IN = 56;
    private static final int OUT = 1 + 5 * 7 + 6;
    private static final int O_IMPACT = 1 + 5 * 7;
    /** After the native numbers: how it is leaving (Leaving's kind), how far it has gone (0..1), where to (x, y, z from
     *  its place). */
    private static final int L_KIND = OUT;
    private static final int L_T = OUT + 1;
    private static final int L_TO = OUT + 2;
    private static final int BURNT = 1;
    private static final int DESPAWNED = 2;
    private static final int MERGED = 3;
    private static final int HOPPER = 4;
    private static final Random RANDOM = new Random();
    private static final Bits BITS = new Bits(900);
    private static final float[] IN_V = new float[IN];
    private static final double[] AT = new double[3];
    private static Map<String, String> set = Map.of();
    private static long polled;

    /** What an item's ground is like, kept a tick (the grid, the slip, a walker in it). */
    private static final class Near {
        long tick = -1;
        final float[] grid = new float[25];
        float slip = 0.6F;
        boolean kick;
        float kx;
        float kz;
        float kvx;
        float kvz;
        int shape;
        int stuff;
        Item item;
    }

    private static final Map<Integer, Near> NEAR = new HashMap<>();

    /** An item in lava or fire, or afloat: where it was, when, how deep. */
    private static final class Seen {
        double x;
        double y;
        double z;
        long at;
        boolean hot;
        boolean afloat;
        float top;
        long ripple;
        int colour;
    }

    private static final Map<Integer, Seen> SEEN = new HashMap<>();
    /** The landings and splashes this frame, for the dust and drops (drawn with the world's effects). */
    private static final List<double[]> BURSTS = new ArrayList<>();

    /** A copy's own shadow: where it is (from the item's place), how wide, what it falls on. */
    public record Shade(float x, float z, float radius, List<EntityRenderState.ShadowPiece> pieces) {
    }

    /** An item the server took away, kept a moment to be seen going: burnt up (it sinks and chars), despawned (it melts
     *  away), merged into another (it hops over to it), into a hopper (it is drawn in). */
    private static final class Leaving {
        /** the very one kept, in its world (another with its id, or another world: not it) */
        ItemEntity entity;
        ClientLevel level;
        int kind;
        long from;
        long until;
        int into = -1;
        double tx;
        double ty;
        double tz;
    }

    private static final Map<Integer, Leaving> LEAVING = new HashMap<>();
    /** The pickup's flight this frame: its arc's height now. */
    private static float arcNow;

    private ItemPhysic() {
    }

    // ------------------------------------------------------------------------------------------ leaving

    /** Whether any of these the server takes away is a dropped item (the rest go as they would). */
    public static boolean any(ClientLevel level, IntList ids) {
        if (!Fx.on(settings(), "on", false)) {
            return false;
        }
        for (int i = 0; i < ids.size(); i++) {
            if (level.getEntity(ids.getInt(i)) instanceof ItemEntity) {
                return true;
            }
        }
        return false;
    }

    /** The server takes the item `id` away: true when it is kept a moment to be seen going (it goes in tick after). */
    public static boolean leaving(ClientLevel level, int id) {
        Map<String, String> v = settings();
        Minecraft mc = Minecraft.getInstance();
        if (!Fx.on(v, "on", false) || LEAVING.containsKey(id) || mc.player == null || !(level.getEntity(id) instanceof ItemEntity e) || e.getItem().isEmpty()
                || e.distanceToSqr(mc.player) > 24.0 * 24.0) {
            return false;
        }
        Leaving l = new Leaving();
        l.entity = e;
        l.level = level;
        float secs;
        if ((e.isInLava() || e.isOnFire()) && Fx.on(v, "Lava and fire", true)) {
            l.kind = BURNT;
            secs = 1.4F;
        } else if (e.getAge() >= 5990) {
            l.kind = DESPAWNED;
            secs = 0.7F;
        } else {
            // merged: the same thing lying by it that is staying; into a hopper: one under it
            for (ItemEntity o : level.getEntitiesOfClass(ItemEntity.class, e.getBoundingBox().inflate(0.75, 0.4, 0.75),
                    o -> o != e && !LEAVING.containsKey(o.getId()) && ItemStack.isSameItemSameComponents(o.getItem(), e.getItem()))) {
                l.kind = MERGED;
                l.into = o.getId();
                l.tx = o.getX();
                l.ty = o.getY();
                l.tz = o.getZ();
                break;
            }
            BlockPos at = e.blockPosition();
            for (BlockPos b : new BlockPos[] {at, at.below()}) {
                if (l.kind == 0 && level.getBlockState(b).getBlock() instanceof HopperBlock) {
                    l.kind = HOPPER;
                    l.tx = b.getX() + 0.5;
                    l.ty = b.getY() + 0.7;
                    l.tz = b.getZ() + 0.5;
                }
            }
            secs = l.kind == MERGED ? 0.32F : 0.3F;
        }
        if (l.kind == 0) {
            return false;
        }
        l.from = System.nanoTime();
        l.until = l.from + (long) (secs * 1e9F);
        LEAVING.put(id, l);
        return true;
    }

    /** Each tick: the items gone their moment go for good. */
    public static void tick(Minecraft mc) {
        if (LEAVING.isEmpty()) {
            return;
        }
        ClientLevel level = mc.level;
        long now = System.nanoTime();
        for (Iterator<Map.Entry<Integer, Leaving>> it = LEAVING.entrySet().iterator(); it.hasNext();) {
            Map.Entry<Integer, Leaving> en = it.next();
            Leaving l = en.getValue();
            if (level != l.level) {
                // another world: not ours to take anything from
                it.remove();
            } else if (now >= l.until || level.getEntity(en.getKey()) != l.entity) {
                it.remove();
                // only the very one kept (the server may have sent it again since, under its id)
                if (level.getEntity(en.getKey()) == l.entity) {
                    level.removeEntity(en.getKey(), Entity.RemovalReason.DISCARDED);
                }
            }
        }
    }

    // ------------------------------------------------------------------------------------------ the pickup's flight

    /** How many ticks a pickup flies (the game's 3; with the flight, twice that). */
    public static int pickupTicks() {
        Map<String, String> v = settings();
        return Fx.on(v, "on", false) && Fx.on(v, "Pickup flight", true) ? 6 : 3;
    }

    /** A pickup's frame: how far it has flown, for its arc and its copies closing in. */
    public static void flying(ItemPickupParticle particle, float partial) {
        ItemPickupParticleAccess a = (ItemPickupParticleAccess) particle;
        int ticks = pickupTicks();
        float f = Math.min(1.0F, (a.cursed$life() + partial) / ticks);
        boolean on = ticks > 3;
        arcNow = on ? (float) Math.sin(f * Math.PI) * 0.32F : 0.0F;
        if (a.cursed$state() instanceof ItemPose h) {
            h.cursed$flight(on ? Math.max(f, 0.001F) : 0.0F);
        }
    }

    /** The pickup's arc's height this frame. */
    public static double arc() {
        return arcNow;
    }

    static Map<String, String> settings() {
        long now = System.nanoTime();
        if (now - polled > 250_000_000L) {
            polled = now;
            set = Fx.settings(MODULE);
        }
        return set;
    }

    // ------------------------------------------------------------------------------------------ the frame's items

    /** As the game gathers a dropped item's state: its bodies' poses this frame, and what happened to it. */
    public static void extract(ItemEntity e, ItemEntityRenderState state, float partial) {
        ItemPose holder = (ItemPose) state;
        holder.cursed$pose(null);
        holder.cursed$shades(null);
        holder.cursed$flight(0.0F);
        Map<String, String> v = settings();
        if (!Fx.on(v, "on", false) || state.item.isEmpty() || state.count <= 0) {
            return;
        }
        Minecraft mc = Minecraft.getInstance();
        ClientLevel level = mc.level;
        if (level == null) {
            return;
        }
        ItemStack stack = e.getItem();
        AABB box = state.item.getModelBoundingBox();
        Near near = NEAR.computeIfAbsent(e.getId(), k -> new Near());
        long tick = level.getGameTime();
        if (near.tick != tick) {
            near.tick = tick;
            ground(level, e, near);
            if (near.item != stack.getItem()) {
                near.item = stack.getItem();
                near.shape = shape(stack, box);
                near.stuff = stuff(stack);
            }
        }
        int flags = 0;
        if (e.onGround()) {
            flags |= 1;
        }
        float top = 0.0F;
        // afloat: water or lava at it or just under it (the game's own "in water" comes and goes as it bobs on the top,
        // and the bodies would drop to the bottom each time)
        double surface = fluidTop(level, state.x, state.y, state.z);
        boolean lava = false;
        if (!Double.isNaN(surface) && state.y < surface + 0.2) {
            flags |= 2;
            lava = FLUID_LAVA[0];
            if (lava) {
                flags |= 4;
            }
            top = (float) (surface - state.y);
        }
        IN_V[0] = e.getId() & 0xFFFF;
        IN_V[50] = e.getId() >>> 16;
        IN_V[1] = flags;
        IN_V[2] = near.shape;
        IN_V[3] = near.stuff;
        IN_V[4] = state.count;
        IN_V[5] = (float) e.getDeltaMovement().x;
        IN_V[6] = (float) e.getDeltaMovement().y;
        IN_V[7] = (float) e.getDeltaMovement().z;
        IN_V[8] = state.ageInTicks;
        IN_V[9] = (float) box.minX;
        IN_V[10] = (float) box.minY;
        IN_V[11] = (float) box.minZ;
        IN_V[12] = (float) box.maxX;
        IN_V[13] = (float) box.maxY;
        IN_V[14] = (float) box.maxZ;
        IN_V[15] = top;
        IN_V[16] = near.slip;
        System.arraycopy(near.grid, 0, IN_V, 17, 25);
        boolean kick = near.kick && Fx.on(v, "Kick", true);
        IN_V[42] = near.kx;
        IN_V[43] = near.kz;
        IN_V[44] = near.kvx;
        IN_V[45] = near.kvz;
        IN_V[46] = kick ? 1.0F : 0.0F;
        IN_V[47] = Fx.number(v, "Bounce", 1.0F);
        IN_V[48] = Fx.number(v, "Spin", 1.0F);
        IN_V[49] = Fx.on(v, "Piles", true) ? 1.0F : 0.0F;
        AT[0] = state.x;
        AT[1] = state.y;
        AT[2] = state.z;
        float[] out = new float[OUT + 6];
        Native.itemPose(AT, IN_V, out);
        if (out[0] < 1.0F) {
            return;
        }
        Leaving going = LEAVING.get(e.getId());
        if (going != null && going.entity != e) {
            going = null;
        }
        if (going != null) {
            out[L_KIND] = going.kind;
            out[L_T] = Math.clamp((System.nanoTime() - going.from) / (float) (going.until - going.from), 0.0F, 1.0F);
            if (going.kind == MERGED && level.getEntity(going.into) instanceof ItemEntity o) {
                going.tx = Mth.lerp(partial, o.xo, o.getX());
                going.ty = Mth.lerp(partial, o.yo, o.getY());
                going.tz = Mth.lerp(partial, o.zo, o.getZ());
            }
            out[L_TO] = (float) (going.tx - state.x);
            out[L_TO + 1] = (float) (going.ty - state.y);
            out[L_TO + 2] = (float) (going.tz - state.z);
        }
        holder.cursed$pose(out);
        holder.cursed$shades(shades(level, state, out, box, going == null ? 0.0F : out[L_T]));
        // a screen's picture of it (the pickup's flight reuses the state): no sounds, no dust from it; nor from one
        // going
        if (partial == 1.0F || going != null && going.kind != BURNT) {
            return;
        }
        Seen s = SEEN.computeIfAbsent(e.getId(), k -> new Seen());
        s.x = state.x;
        s.y = state.y;
        s.z = state.z;
        s.at = System.nanoTime();
        s.hot = lava || e.isOnFire();
        s.afloat = (flags & 2) != 0 && !lava && out[O_IMPACT + 5] < 0.35F;
        s.top = top;
        if (out[O_IMPACT] > 0.0F) {
            landed(mc, level, stack, near, state, out, v);
        }
        if (out[O_IMPACT + 4] > 0.0F && Fx.on(v, "Water", true)) {
            if (BURSTS.size() < 256) {
                BURSTS.add(new double[] {1, state.x, state.y + top, state.z, out[O_IMPACT + 4], lava ? 1 : 0});
            }
        }
    }

    /** Drawn: each copy where its body lies (true), or the game's way (false: off, or not worked out). */
    public static boolean submit(ItemEntityRenderState state, PoseStack pose, SubmitNodeCollector out) {
        ItemPose holder = (ItemPose) state;
        float[] p = holder.cursed$pose();
        if (p == null || state.item.isEmpty()) {
            return false;
        }
        AABB box = state.item.getModelBoundingBox();
        float mx = (float) (box.minX + box.maxX) * 0.5F;
        float my = (float) (box.minY + box.maxY) * 0.5F;
        float mz = (float) (box.minZ + box.maxZ) * 0.5F;
        int n = Math.min(5, (int) p[0]);
        int kind = (int) p[L_KIND];
        float t = p[L_T];
        float gone = smooth(t);
        float flight = holder.cursed$flight();
        float closing = smooth(flight);
        Quaternionf q = new Quaternionf();
        for (int k = 0; k < n; k++) {
            int o = 1 + k * 7;
            float x = p[o];
            float y = p[o + 1];
            float z = p[o + 2];
            float scale = 1.0F;
            int light = state.lightCoords;
            if (flight > 0.0F) {
                // picked up: the copies close in on the middle as they fly, smaller toward the end
                x *= 1.0F - closing;
                z *= 1.0F - closing;
                y = Mth.lerp(closing, y, 0.12F);
                scale = 1.0F - 0.4F * closing * closing;
            }
            switch (kind) {
                case BURNT -> {
                    // sinks into it, smaller, charring black
                    y -= 0.32F * gone;
                    scale *= 1.0F - 0.45F * gone;
                    float left = (1.0F - t) * (1.0F - t);
                    light = LightCoordsUtil.pack(Math.round(LightCoordsUtil.block(light) * left), Math.round(LightCoordsUtil.sky(light) * left));
                }
                case DESPAWNED -> {
                    scale *= 1.0F - gone;
                    y += 0.05F * gone;
                }
                case MERGED, HOPPER -> {
                    // over to where it goes, in a little hop (into a hopper: drawn in, smaller)
                    float spread = 1.0F - gone;
                    x = Mth.lerp(gone, x, p[L_TO] + x * 0.3F * spread);
                    z = Mth.lerp(gone, z, p[L_TO + 2] + z * 0.3F * spread);
                    y = Mth.lerp(gone, y, p[L_TO + 1] + y * 0.5F) + (float) Math.sin(t * Math.PI) * (kind == MERGED ? 0.22F : 0.1F);
                    scale *= kind == HOPPER ? 1.0F - 0.6F * gone : 1.0F;
                }
                default -> {
                }
            }
            if (scale <= 0.01F) {
                continue;
            }
            pose.pushPose();
            pose.translate(x, y, z);
            if (flight > 0.0F) {
                pose.rotate(new Quaternionf().rotationY(flight * flight * 2.4F * (k % 2 == 0 ? 1.0F : -1.0F)));
            }
            q.set(p[o + 3], p[o + 4], p[o + 5], p[o + 6]);
            pose.rotate(q);
            pose.scale(scale, scale, scale);
            pose.translate(-mx, -my, -mz);
            state.item.submit(pose, out, light, OverlayTexture.NO_OVERLAY, state.outlineColor);
            pose.popPose();
        }
        // each copy's own shadow, not the game's one at the middle (none in a pickup's flight)
        List<Shade> shades = holder.cursed$shades();
        if (shades != null && flight == 0.0F) {
            for (Shade s : shades) {
                if (!s.pieces().isEmpty()) {
                    pose.pushPose();
                    pose.translate(s.x(), 0.0F, s.z());
                    out.submitShadow(pose, s.radius(), s.pieces());
                    pose.popPose();
                }
            }
        }
        state.shadowPieces.clear();
        return true;
    }

    private static float smooth(float t) {
        float c = Math.clamp(t, 0.0F, 1.0F);
        return c * c * (3.0F - 2.0F * c);
    }

    /** Each copy's own soft shadow on what is under it, as the game casts one (on any block's top, a slab's too), fainter
     *  the higher the copy is over it and as it goes; none when the game's shadows are off. */
    private static @org.jspecify.annotations.Nullable List<Shade> shades(ClientLevel level, ItemEntityRenderState state, float[] p, AABB box, float going) {
        if (!Minecraft.getInstance().options.entityShadows().get() || state.isInvisible) {
            return null;
        }
        float strength = (float) ((1.0 - state.distanceToCameraSq / 256.0) * 0.75) * (1.0F - going);
        if (strength <= 0.0F) {
            return null;
        }
        float reach = (float) Math.max(box.getXsize(), Math.max(box.getYsize(), box.getZsize())) * 0.5F;
        float thin = (float) Math.min(box.getXsize(), Math.min(box.getYsize(), box.getZsize())) * 0.5F;
        float r = reach * 0.85F + 0.02F;
        BlockPos.MutableBlockPos at = new BlockPos.MutableBlockPos();
        int n = Math.min(5, (int) p[0]);
        List<Shade> all = new ArrayList<>(n);
        for (int k = 0; k < n; k++) {
            int o = 1 + k * 7;
            double x = state.x + p[o];
            double y = state.y + p[o + 1];
            double z = state.z + p[o + 2];
            List<EntityRenderState.ShadowPiece> pieces = new ArrayList<>(4);
            for (int bz = Mth.floor(z - r); bz <= Mth.floor(z + r); bz++) {
                for (int bx = Mth.floor(x - r); bx <= Mth.floor(x + r); bx++) {
                    double top = top(level, Math.clamp(x, bx + 0.001, bx + 0.999), y, Math.clamp(z, bz + 0.001, bz + 0.999));
                    float high = Math.max(0.0F, (float) (y - top) - thin);
                    float a = 1.0F - high * 1.8F;
                    if (a <= 0.0F) {
                        continue;
                    }
                    at.set(bx, Mth.floor(top + 0.01), bz);
                    int light = level.getMaxLocalRawBrightness(at);
                    if (light <= 3) {
                        continue;
                    }
                    float alpha = Math.clamp(strength * 0.45F * a * Lightmap.getBrightness(level.dimensionType(), light), 0.0F, 1.0F);
                    pieces.add(new EntityRenderState.ShadowPiece((float) (bx - x), (float) (top - state.y), (float) (bz - z), Shapes.block(), alpha));
                }
            }
            all.add(new Shade(p[o], p[o + 2], r, pieces));
        }
        return all;
    }

    // ------------------------------------------------------------------------------------------ what an item is

    /** What it is like to handle: round (rolls on its rim), long (a tool, a stick, a bottle), solid (with depth: a
     *  block, a head), flat (a sprite). */
    private static int shape(ItemStack st, AABB box) {
        Item it = st.getItem();
        if (st.has(DataComponents.JUKEBOX_PLAYABLE) || it == Items.CLOCK || it == Items.COMPASS || it == Items.RECOVERY_COMPASS || it == Items.SNOWBALL
                || it == Items.ENDER_PEARL || it == Items.ENDER_EYE || it == Items.SLIME_BALL || it == Items.MAGMA_CREAM || it == Items.FIRE_CHARGE
                || it == Items.HEART_OF_THE_SEA || it == Items.EGG || it == Items.BLUE_EGG || it == Items.BROWN_EGG) {
            return 2;
        }
        if (box.getZsize() > 0.0625 * 1.6) {
            return 1;
        }
        if (st.is(ItemTags.SWORDS) || st.is(ItemTags.AXES) || st.is(ItemTags.PICKAXES) || st.is(ItemTags.SHOVELS) || st.is(ItemTags.HOES)
                || st.has(DataComponents.TOOL) || it == Items.STICK || it == Items.BLAZE_ROD || it == Items.BREEZE_ROD || it == Items.BONE
                || it == Items.GLASS_BOTTLE || it == Items.POTION || it == Items.SPLASH_POTION || it == Items.LINGERING_POTION
                || it == Items.HONEY_BOTTLE || it == Items.EXPERIENCE_BOTTLE || it == Items.TRIDENT || it == Items.FISHING_ROD) {
            return 3;
        }
        return 0;
    }

    /** What it is made of (its bounce, its sound): a block by its sound, the rest by its name. */
    private static int stuff(ItemStack st) {
        if (st.getItem() instanceof BlockItem bi) {
            SoundType t = bi.getBlock().defaultBlockState().getSoundType();
            if (t == SoundType.GLASS || t == SoundType.AMETHYST) {
                return 4;
            }
            if (t == SoundType.METAL || t == SoundType.ANVIL || t == SoundType.CHAIN || t == SoundType.COPPER || t == SoundType.NETHERITE_BLOCK
                    || t == SoundType.LANTERN) {
                return 3;
            }
            if (t == SoundType.WOOD || t == SoundType.BAMBOO_WOOD || t == SoundType.CHERRY_WOOD || t == SoundType.NETHER_WOOD) {
                return 1;
            }
            if (t == SoundType.WOOL || t == SoundType.GRASS || t == SoundType.SAND || t == SoundType.SNOW || t == SoundType.MOSS || t == SoundType.SLIME_BLOCK
                    || t == SoundType.GRAVEL) {
                return 0;
            }
            return 2;
        }
        String name = BuiltInRegistries.ITEM.getKey(st.getItem()).getPath();
        if (name.contains("bottle") || name.contains("potion") || name.contains("diamond") || name.contains("emerald") || name.contains("amethyst")
                || name.contains("quartz") || name.contains("glass") || name.contains("echo_shard") || name.contains("prismarine_crystals")) {
            return 4;
        }
        if (name.contains("iron") || name.contains("gold") || name.contains("netherite") || name.contains("copper") || name.contains("chain")
                || name.contains("ingot") || name.contains("nugget") || name.contains("shears") || name.contains("bucket") || name.contains("compass")
                || name.contains("clock") || name.contains("mace") || name.contains("trident") || name.contains("key") || name.contains("spyglass")) {
            return 3;
        }
        if (name.contains("stone") || name.contains("flint") || name.contains("brick") || name.contains("bone") || name.contains("shard")
                || name.contains("coal") || name.contains("pottery")) {
            return 2;
        }
        if (name.contains("wooden") || name.contains("stick") || name.contains("bow") || name.contains("boat") || name.contains("sign")
                || name.contains("bowl") || name.contains("rod") || name.contains("arrow") || name.contains("disc")) {
            return 1;
        }
        return 0;
    }

    // ------------------------------------------------------------------------------------------ round the item

    /** The ground's tops round the item (5 × 5, a quarter of a block apart, over its height), the block under it's
     *  slip, a walker going through it. */
    private static void ground(ClientLevel level, ItemEntity e, Near near) {
        double x = e.getX();
        double y = e.getY();
        double z = e.getZ();
        for (int k = 0; k < 5; k++) {
            for (int i = 0; i < 5; i++) {
                near.grid[k * 5 + i] = (float) (top(level, x + (i - 2) * 0.25, y, z + (k - 2) * 0.25) - y);
            }
        }
        near.slip = level.getBlockState(BlockPos.containing(x, y - 0.2, z)).getBlock().getFriction();
        near.kick = false;
        double best = 0.0;
        for (Entity w : level.getEntities(e, e.getBoundingBox().inflate(0.35, 0.3, 0.35), t -> t instanceof LivingEntity && !t.isSpectator())) {
            double vx = (w.getX() - w.xo) * 20.0;
            double vz = (w.getZ() - w.zo) * 20.0;
            double sp = vx * vx + vz * vz;
            if (sp > best) {
                best = sp;
                near.kick = true;
                near.kx = (float) (w.getX() - x);
                near.kz = (float) (w.getZ() - z);
                near.kvx = (float) vx;
                near.kvz = (float) vz;
            }
        }
    }

    /** The top of what is solid at (x, z) from a little over `y` down (nothing near: well below). */
    private static double top(ClientLevel level, double x, double y, double z) {
        int bx = (int) Math.floor(x);
        int bz = (int) Math.floor(z);
        BlockPos.MutableBlockPos at = new BlockPos.MutableBlockPos();
        for (int by = (int) Math.floor(y + 0.6); by >= (int) Math.floor(y - 1.6); by--) {
            at.set(bx, by, bz);
            BlockState b = level.getBlockState(at);
            if (b.isAir()) {
                continue;
            }
            VoxelShape s = b.getCollisionShape(level, at);
            if (s.isEmpty()) {
                continue;
            }
            double t = s.max(Direction.Axis.Y, z - bz, x - bx);
            if (t == Double.NEGATIVE_INFINITY) {
                continue;
            }
            double top = by + t;
            if (top <= y + 0.6) {
                return top;
            }
        }
        return y - 1.6;
    }

    /** The water's (or lava's) top at the item, or just under it (to the block over it, if that is wet too); NaN:
     *  none. */
    private static double fluidTop(ClientLevel level, double x, double y, double z) {
        for (double dy : new double[] {0.05, -0.35}) {
            BlockPos b = BlockPos.containing(x, y + dy, z);
            FluidState f = level.getFluidState(b);
            if (f.isEmpty()) {
                continue;
            }
            FLUID_LAVA[0] = f.is(FluidTags.LAVA);
            double top = b.getY() + f.getHeight(level, b);
            BlockPos u = b.above();
            FluidState up = level.getFluidState(u);
            if (!up.isEmpty()) {
                top = u.getY() + up.getHeight(level, u);
            }
            return top;
        }
        return Double.NaN;
    }

    /** Whether the fluid fluidTop found is lava. */
    private static final boolean[] FLUID_LAVA = new boolean[1];

    // ------------------------------------------------------------------------------------------ what happened

    /** A landing: a sound of its stuff (a block's own), the dust of the block it fell on. */
    private static void landed(Minecraft mc, ClientLevel level, ItemStack stack, Near near, ItemEntityRenderState state, float[] out, Map<String, String> v) {
        float hard = out[O_IMPACT];
        double x = state.x + out[O_IMPACT + 1];
        double y = state.y + out[O_IMPACT + 2];
        double z = state.z + out[O_IMPACT + 3];
        if (Fx.on(v, "Sounds", true)) {
            SoundEvent sound;
            float pitch = 1.25F + RANDOM.nextFloat() * 0.3F;
            if (stack.getItem() instanceof BlockItem bi) {
                SoundType t = bi.getBlock().defaultBlockState().getSoundType();
                sound = t.getHitSound();
                pitch *= t.getPitch();
            } else {
                sound = switch (near.stuff) {
                    case 1 -> SoundEvents.WOOD_HIT;
                    case 2 -> SoundEvents.STONE_HIT;
                    case 3 -> SoundEvents.CHAIN_HIT;
                    case 4 -> SoundEvents.AMETHYST_BLOCK_HIT;
                    default -> SoundEvents.WOOL_HIT;
                };
            }
            float volume = Math.clamp((hard - 1.2F) * 0.07F, 0.03F, 0.32F) * Fx.number(v, "Volume", 1.0F);
            level.playLocalSound(x, y, z, sound, SoundSource.NEUTRAL, volume, pitch, false);
        }
        if (Fx.on(v, "Landing dust", true) && hard > 2.2F) {
            if (BURSTS.size() < 256) {
                BURSTS.add(new double[] {0, x, y, z, hard, 0});
            }
        }
    }

    // ------------------------------------------------------------------------------------------ the effects

    static void gone() {
        BITS.clear();
        BURSTS.clear();
        SEEN.clear();
        NEAR.clear();
        RIPPLES.clear();
    }

    /** Each frame, with the world's effects: the dust and drops of what happened, smoke and embers off what burns, rings
     *  round what floats, a burst for what burnt up. */
    static void draw(Minecraft mc, float partial, float dt) {
        ClientLevel level = mc.level;
        if (level == null) {
            return;
        }
        // the items not seen a while (gone, out of sight): their ground's numbers go
        if (NEAR.size() > 64) {
            long tick = level.getGameTime();
            NEAR.values().removeIf(n -> tick - n.tick > 200);
        }
        Map<String, String> v = settings();
        boolean on = Fx.on(v, "on", false);
        for (double[] b : BURSTS) {
            if (b[0] == 0) {
                dust(level, b[1], b[2], b[3], (float) b[4]);
            } else {
                splash(level, b[1], b[2], b[3], (float) b[4], b[5] > 0.5);
            }
        }
        BURSTS.clear();
        long now = System.nanoTime();
        boolean hot = Fx.on(v, "Lava and fire", true);
        boolean water = Fx.on(v, "Water", true);
        for (Iterator<Map.Entry<Integer, Seen>> it = SEEN.entrySet().iterator(); it.hasNext();) {
            Map.Entry<Integer, Seen> en = it.next();
            Seen s = en.getValue();
            long age = now - s.at;
            if (age > 200_000_000L) {
                // gone: burnt up if it was burning and is no more in the world
                if (on && hot && s.hot && age < 600_000_000L && level.getEntity(en.getKey()) == null) {
                    burnt(level, s.x, s.y, s.z);
                }
                it.remove();
                continue;
            }
            if (!on) {
                continue;
            }
            if (hot && s.hot && RANDOM.nextFloat() < dt * 14.0F) {
                BITS.add(s.x + (RANDOM.nextFloat() - 0.5) * 0.2, s.y + 0.15, s.z + (RANDOM.nextFloat() - 0.5) * 0.2, (RANDOM.nextFloat() - 0.5) * 0.15,
                        0.45 + RANDOM.nextFloat() * 0.35, (RANDOM.nextFloat() - 0.5) * 0.15, 1.3F + RANDOM.nextFloat(), 0.1F + RANDOM.nextFloat() * 0.08F,
                        0x4d3f38, Bits.PUFF, -0.25F, 1.2F);
                if (RANDOM.nextFloat() < 0.5F) {
                    BITS.add(s.x, s.y + 0.1, s.z, (RANDOM.nextFloat() - 0.5) * 0.9, 1.2 + RANDOM.nextFloat() * 1.4, (RANDOM.nextFloat() - 0.5) * 0.9,
                            0.5F + RANDOM.nextFloat() * 0.5F, 0.025F + RANDOM.nextFloat() * 0.02F, 0xffb347, Bits.EMBER, 3.0F, 0.8F);
                }
            }
            if (water && s.afloat && now - s.ripple > 1_600_000_000L + RANDOM.nextInt(1400) * 1_000_000L) {
                s.ripple = now;
                RIPPLES.add(new float[] {(float) s.x, (float) (s.y + s.top + 0.005), (float) s.z, 0.0F, 0.45F, 2.2F});
            }
        }
        // the rings on the water: a fine line widening and fading, as a ripple's crest catches the light
        Minecraft m = Minecraft.getInstance();
        double ex = m.gameRenderer.mainCamera().position().x;
        double ez = m.gameRenderer.mainCamera().position().z;
        for (int i = RIPPLES.size() - 1; i >= 0; i--) {
            float[] r = RIPPLES.get(i);
            r[3] += dt;
            float q = r[3] / r[5];
            if (q >= 1.0F) {
                RIPPLES.remove(i);
                continue;
            }
            if (q < 0.0F) {
                continue;
            }
            // drawn on the water's top itself (a ring laid on what is there would fall to the bottom through it)
            double far = Math.sqrt((r[0] - ex) * (r[0] - ex) + (r[2] - ez) * (r[2] - ez));
            float radius = 0.08F + 0.5F * (float) Math.sqrt(q) * (0.6F + r[4] * 0.4F);
            float k = r[4] * 0.32F * (1.0F - q) * (1.0F - q) * (float) Math.clamp(1.0 - (far - 8.0) / 16.0, 0.0, 1.0);
            float half = 0.0045F + 0.004F * (1.0F - q);
            int pieces = 40;
            for (int s = 0; s < pieces; s++) {
                double a0 = s / (double) pieces * Math.PI * 2.0;
                double a1 = (s + 1) / (double) pieces * Math.PI * 2.0;
                double am = (a0 + a1) * 0.5;
                Fx.ribbon(r[0] + Math.cos(a0) * radius, r[1], r[2] + Math.sin(a0) * radius, half, r[0] + Math.cos(a1) * radius, r[1],
                        r[2] + Math.sin(a1) * radius, half, Math.cos(am), 0.0, Math.sin(am), 0.0F, 0.0F, false, 0xd4e9ff, k, 0);
            }
        }
        BITS.step(level, dt, 1.0F);
    }

    /** The rings on the water: where, how long out, how strong, how long they last. */
    private static final List<float[]> RIPPLES = new ArrayList<>();

    /** A landing's dust: the block's colour, a few grains and a soft puff, as hard as it came down. */
    private static void dust(ClientLevel level, double x, double y, double z, float hard) {
        BlockPos b = BlockPos.containing(x, y - 0.05, z);
        BlockState s = level.getBlockState(b);
        if (s.isAir()) {
            return;
        }
        int c = s.getMapColor(level, b).col;
        if (c == 0) {
            return;
        }
        int puff = Bits.mix(c, 0xffffff, 0.55);
        int grain = Bits.mix(c, 0xffffff, 0.2);
        int n = Math.min(10, (int) (hard * 1.4F));
        for (int k = 0; k < n; k++) {
            double a = RANDOM.nextDouble() * Math.PI * 2.0;
            double sp = 0.4 + RANDOM.nextDouble() * 0.6 * Math.min(2.0, hard / 4.0);
            BITS.add(x, y + 0.02, z, Math.cos(a) * sp, 0.6 + RANDOM.nextDouble() * 0.9, Math.sin(a) * sp, 0.45F + RANDOM.nextFloat() * 0.4F,
                    0.022F + RANDOM.nextFloat() * 0.02F, grain, Bits.GRAIN, 9.0F, 2.0F);
        }
        for (int k = 0; k < 2; k++) {
            double a = RANDOM.nextDouble() * Math.PI * 2.0;
            BITS.add(x, y + 0.03, z, Math.cos(a) * 0.35, 0.12, Math.sin(a) * 0.35, 0.7F + RANDOM.nextFloat() * 0.4F, 0.1F + RANDOM.nextFloat() * 0.06F, puff,
                    Bits.PUFF, 0.2F, 3.0F);
        }
    }

    /** A splash: drops thrown up and out, a ring on the top; lava's drops glow. */
    private static void splash(ClientLevel level, double x, double y, double z, float hard, boolean lava) {
        int n = Math.min(16, 4 + (int) (hard * 2.0F));
        int c = lava ? 0xff8a2a : 0xdcefff;
        for (int k = 0; k < n; k++) {
            double a = RANDOM.nextDouble() * Math.PI * 2.0;
            double sp = 0.5 + RANDOM.nextDouble() * 0.9;
            BITS.add(x, y + 0.02, z, Math.cos(a) * sp, 1.6 + RANDOM.nextDouble() * Math.min(2.5, hard * 0.5), Math.sin(a) * sp, 0.5F + RANDOM.nextFloat() * 0.35F,
                    0.02F + RANDOM.nextFloat() * 0.018F, c, lava ? Bits.EMBER : Bits.GRAIN, 12.0F, 0.6F);
        }
        if (!lava) {
            // a splash's rings: one quick and bright, one after it slower
            RIPPLES.add(new float[] {(float) x, (float) (y + 0.005), (float) z, 0.0F, Math.min(1.0F, 0.5F + hard * 0.12F), 1.1F});
            RIPPLES.add(new float[] {(float) x, (float) (y + 0.005), (float) z, -0.18F, Math.min(0.8F, 0.3F + hard * 0.08F), 1.8F});
        }
        level.playLocalSound(x, y, z, lava ? SoundEvents.LAVA_POP : SoundEvents.GENERIC_SPLASH, SoundSource.NEUTRAL,
                Math.clamp(hard * 0.04F, 0.05F, 0.25F), 1.4F + RANDOM.nextFloat() * 0.3F, false);
    }

    /** Burnt up: a burst of embers and a puff of ash where it was, a hiss. */
    private static void burnt(ClientLevel level, double x, double y, double z) {
        for (int k = 0; k < 14; k++) {
            double a = RANDOM.nextDouble() * Math.PI * 2.0;
            double sp = 0.4 + RANDOM.nextDouble() * 1.2;
            BITS.add(x, y + 0.15, z, Math.cos(a) * sp, 1.0 + RANDOM.nextDouble() * 2.0, Math.sin(a) * sp, 0.5F + RANDOM.nextFloat() * 0.7F,
                    0.02F + RANDOM.nextFloat() * 0.025F, RANDOM.nextBoolean() ? 0xffc46b : 0xff7a2e, Bits.EMBER, 3.5F, 0.9F);
        }
        for (int k = 0; k < 3; k++) {
            BITS.add(x, y + 0.2, z, (RANDOM.nextFloat() - 0.5) * 0.3, 0.5 + RANDOM.nextFloat() * 0.3, (RANDOM.nextFloat() - 0.5) * 0.3, 1.6F + RANDOM.nextFloat(),
                    0.16F + RANDOM.nextFloat() * 0.1F, 0x45372f, Bits.PUFF, -0.2F, 1.0F);
        }
        level.playLocalSound(x, y, z, SoundEvents.FIRE_EXTINGUISH, SoundSource.NEUTRAL, 0.18F, 1.6F + RANDOM.nextFloat() * 0.4F, false);
    }
}
