package cursed.recode.hud;

import java.time.LocalDate;
import java.time.LocalTime;
import java.util.ArrayDeque;
import java.util.Locale;
import net.minecraft.client.Minecraft;
import net.minecraft.client.player.LocalPlayer;
import net.minecraft.core.BlockPos;
import net.minecraft.network.chat.Component;
import net.minecraft.world.entity.EquipmentSlot;
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.level.Level;

/**
 * The HUD's placeholders' values (gui/hud.rs fills {name} in any text of it): what the game knows, a line each
 * ("name=value"), after the facts. The slower ones (the inventory's counts, the biome, memory) are counted a few times
 * a second; the clicks, kills and deaths are kept here.
 */
public final class HudVars {
    private HudVars() {
    }

    private static final ArrayDeque<Long> LEFT = new ArrayDeque<>();
    private static final ArrayDeque<Long> RIGHT = new ArrayDeque<>();
    private static int kills, deaths, lastKilledId = Integer.MIN_VALUE;
    private static boolean wasDead;
    private static final long SESSION = System.nanoTime();
    private static long slowAt;
    private static final StringBuilder SLOW = new StringBuilder();

    /** A mouse button pressed in the world (MouseHandlerMixin): the clicks a second. */
    public static void click(boolean left) {
        long now = System.nanoTime();
        ArrayDeque<Long> q = left ? LEFT : RIGHT;
        q.addLast(now);
        while (!q.isEmpty() && now - q.peekFirst() > 1_000_000_000L) {
            q.pollFirst();
        }
    }

    private static int cps(ArrayDeque<Long> q) {
        long now = System.nanoTime();
        while (!q.isEmpty() && now - q.peekFirst() > 1_000_000_000L) {
            q.pollFirst();
        }
        return q.size();
    }

    /** The one fought died after the player's hit (HudLayer sees it): a kill, once each. */
    static void target(LivingEntity t, long sinceHitNanos) {
        if (t != null && t.isDeadOrDying() && t.getId() != lastKilledId && sinceHitNanos < 3_000_000_000L) {
            lastKilledId = t.getId();
            kills++;
            cursed.recode.cosmetics.Cosmetics.kill();
            cursed.recode.play.Play.kill(t);
        }
    }

    private static void put(StringBuilder b, String k, Object v) {
        b.append(k).append('=').append(String.valueOf(v).replace('\n', ' ')).append('\n');
    }

    private static String f1(double v) {
        return String.format(Locale.ROOT, "%.1f", v);
    }

    private static int count(LocalPlayer p, Item item) {
        int n = 0;
        var inv = p.getInventory();
        for (int i = 0; i < inv.getContainerSize(); i++) {
            ItemStack s = inv.getItem(i);
            if (s.is(item)) {
                n += s.getCount();
            }
        }
        return n;
    }

    private static String name(ItemStack s) {
        return s.isEmpty() ? "" : s.getHoverName().getString();
    }

    private static String durability(ItemStack s) {
        if (s.isEmpty() || !s.isDamageableItem()) {
            return "";
        }
        return (s.getMaxDamage() - s.getDamageValue()) + "/" + s.getMaxDamage();
    }

    /** What the crosshair is on (Custom F3): the block (its name, id, place, state, the liquid in it), the creature or
     *  thing (its name, kind, health, distance); empty when it is on none. */
    private static void looked(Minecraft mc, StringBuilder b) {
        var hit = mc.hitResult;
        if (hit instanceof net.minecraft.world.phys.BlockHitResult bh && hit.getType() == net.minecraft.world.phys.HitResult.Type.BLOCK) {
            BlockPos at = bh.getBlockPos();
            var st = mc.level.getBlockState(at);
            put(b, "lookblock", st.getBlock().getName().getString());
            put(b, "lookblockid", net.minecraft.core.registries.BuiltInRegistries.BLOCK.getKey(st.getBlock()).toString());
            put(b, "lookbx", at.getX());
            put(b, "lookby", at.getY());
            put(b, "lookbz", at.getZ());
            StringBuilder state = new StringBuilder();
            st.getValues().limit(8).forEach(v -> {
                if (!state.isEmpty()) {
                    state.append('\u0002');
                }
                state.append(v.property().getName()).append('=').append(v.valueName());
            });
            put(b, "lookstate", state);
            var fluid = mc.level.getFluidState(at);
            put(b, "lookfluid", fluid.isEmpty() ? "" : net.minecraft.core.registries.BuiltInRegistries.FLUID.getKey(fluid.getType()).getPath());
        } else {
            put(b, "lookblock", "");
            put(b, "lookblockid", "");
            put(b, "lookstate", "");
            put(b, "lookfluid", "");
        }
        var e = mc.crosshairPickEntity;
        if (e != null && mc.player != null) {
            put(b, "lookentity", net.minecraft.ChatFormatting.stripFormatting(e.getName().getString()));
            put(b, "lookentitytype", net.minecraft.core.registries.BuiltInRegistries.ENTITY_TYPE.getKey(e.getType()).toString());
            if (e instanceof LivingEntity l) {
                put(b, "lookentityhp", f1(l.getHealth()));
                put(b, "lookentitymax", f1(l.getMaxHealth()));
            } else {
                put(b, "lookentityhp", "");
                put(b, "lookentitymax", "");
            }
            put(b, "lookentitydist", f1(mc.player.distanceTo(e)));
        } else {
            put(b, "lookentity", "");
            put(b, "lookentitytype", "");
            put(b, "lookentityhp", "");
            put(b, "lookentitymax", "");
            put(b, "lookentitydist", "");
        }
    }

    /** Every placeholder's value now, into `b` after the marker line. */
    static void append(Minecraft mc, LivingEntity target, StringBuilder b) {
        LocalPlayer p = mc.player;
        if (p == null || mc.level == null) {
            return;
        }
        b.append('\u0001').append('\n');
        boolean dead = p.isDeadOrDying();
        if (dead && !wasDead) {
            deaths++;
        }
        wasDead = dead;
        put(b, "fps", mc.getFps());
        var info = mc.getConnection() != null ? mc.getConnection().getPlayerInfo(p.getUUID()) : null;
        put(b, "ping", info != null ? info.getLatency() : 0);
        put(b, "x", f1(p.getX()));
        put(b, "y", f1(p.getY()));
        put(b, "z", f1(p.getZ()));
        BlockPos pos = p.blockPosition();
        put(b, "bx", pos.getX());
        put(b, "by", pos.getY());
        put(b, "bz", pos.getZ());
        var dim = mc.level.dimension();
        boolean nether = dim == Level.NETHER;
        put(b, "nx", f1(nether ? p.getX() * 8.0 : p.getX() / 8.0));
        put(b, "nz", f1(nether ? p.getZ() * 8.0 : p.getZ() / 8.0));
        put(b, "dimension", nether ? "nether" : dim == Level.END ? "end" : "overworld");
        put(b, "yaw", f1(net.minecraft.util.Mth.wrapDegrees(p.getYRot())));
        put(b, "pitch", f1(p.getXRot()));
        put(b, "health", f1(p.getHealth()));
        put(b, "maxhealth", f1(p.getMaxHealth()));
        put(b, "hearts", f1(p.getHealth() / 2.0));
        put(b, "absorption", f1(p.getAbsorptionAmount()));
        put(b, "food", p.getFoodData().getFoodLevel());
        put(b, "saturation", f1(p.getFoodData().getSaturationLevel()));
        put(b, "armor", p.getArmorValue());
        put(b, "air", p.getAirSupply() / 20);
        put(b, "level", p.experienceLevel);
        put(b, "xp", Math.round(p.experienceProgress * 100.0F));
        var inv = p.getInventory();
        put(b, "slot", inv.getSelectedSlot() + 1);
        ItemStack held = p.getMainHandItem();
        put(b, "held", name(held));
        put(b, "heldcount", held.isEmpty() ? 0 : held.getCount());
        put(b, "helddurability", durability(held));
        put(b, "offhand", name(p.getOffhandItem()));
        put(b, "helmet", durability(p.getItemBySlot(EquipmentSlot.HEAD)));
        put(b, "chestplate", durability(p.getItemBySlot(EquipmentSlot.CHEST)));
        put(b, "leggings", durability(p.getItemBySlot(EquipmentSlot.LEGS)));
        put(b, "boots", durability(p.getItemBySlot(EquipmentSlot.FEET)));
        put(b, "player", p.getName().getString());
        var server = mc.getCurrentServer();
        put(b, "server", server != null ? server.ip : mc.isLocalServer() ? "Singleplayer" : "");
        put(b, "servername", server != null ? server.name : "");
        put(b, "online", mc.getConnection() != null ? mc.getConnection().getOnlinePlayers().size() : 1);
        put(b, "gamemode", mc.gameMode != null ? mc.gameMode.getPlayerMode().getShortDisplayName().getString() : "");
        put(b, "difficulty", mc.level.getDifficulty().getDisplayName().getString());
        LocalTime now = LocalTime.now();
        put(b, "time", String.format(Locale.ROOT, "%02d:%02d", now.getHour(), now.getMinute()));
        put(b, "seconds", String.format(Locale.ROOT, "%02d:%02d:%02d", now.getHour(), now.getMinute(), now.getSecond()));
        put(b, "time12", String.format(Locale.ROOT, "%d:%02d %s", now.getHour() % 12 == 0 ? 12 : now.getHour() % 12, now.getMinute(), now.getHour() < 12 ? "am" : "pm"));
        LocalDate today = LocalDate.now();
        put(b, "date", String.format(Locale.ROOT, "%02d.%02d.%d", today.getDayOfMonth(), today.getMonthValue(), today.getYear()));
        long ticks = mc.level.getOverworldClockTime();
        long dayTicks = (ticks + 6000L) % 24000L;
        put(b, "gametime", String.format(Locale.ROOT, "%02d:%02d", dayTicks / 1000L, dayTicks % 1000L * 60L / 1000L));
        put(b, "day", ticks / 24000L + 1);
        put(b, "weather", mc.level.isThundering() ? "thunder" : mc.level.isRaining() ? "rain" : "clear");
        put(b, "light", mc.level.getMaxLocalRawBrightness(pos));
        put(b, "cps", cps(LEFT));
        put(b, "rcps", cps(RIGHT));
        put(b, "kills", kills);
        put(b, "deaths", deaths);
        long played = (System.nanoTime() - SESSION) / 1_000_000_000L;
        put(b, "playtime", String.format(Locale.ROOT, "%d:%02d:%02d", played / 3600, played / 60 % 60, played % 60));
        put(b, "speedv", f1(p.getDeltaMovement().y * 20.0));
        if (target != null) {
            put(b, "target", net.minecraft.ChatFormatting.stripFormatting(target.getName().getString()));
            put(b, "targethealth", f1(target.getHealth()));
            put(b, "targetdistance", f1(p.distanceTo(target)));
        } else {
            put(b, "target", "");
            put(b, "targethealth", "");
            put(b, "targetdistance", "");
        }
        put(b, "frametime", f1(1000.0 / Math.max(1, mc.getFps())));
        put(b, "entities", mc.level.getEntityCount());
        // Custom F3's: the chunk and where in it, the light of the sky and of the blocks, the moon, what is looked at
        put(b, "cx", pos.getX() >> 4);
        put(b, "cy", pos.getY() >> 4);
        put(b, "cz", pos.getZ() >> 4);
        put(b, "ix", pos.getX() & 15);
        put(b, "iy", pos.getY() & 15);
        put(b, "iz", pos.getZ() & 15);
        put(b, "skylight", mc.level.getBrightness(net.minecraft.world.level.LightLayer.SKY, pos));
        put(b, "blocklight", mc.level.getBrightness(net.minecraft.world.level.LightLayer.BLOCK, pos));
        put(b, "moon", Math.floorMod(ticks / 24000L, 8L));
        put(b, "dimensionid", dim.identifier().toString());
        looked(mc, b);
        // the slower ones: a few times a second
        long t = System.nanoTime();
        if (t - slowAt > 250_000_000L) {
            slowAt = t;
            SLOW.setLength(0);
            put(SLOW, "totems", count(p, Items.TOTEM_OF_UNDYING));
            put(SLOW, "pearls", count(p, Items.ENDER_PEARL));
            put(SLOW, "gapples", count(p, Items.GOLDEN_APPLE));
            put(SLOW, "egapples", count(p, Items.ENCHANTED_GOLDEN_APPLE));
            put(SLOW, "crystals", count(p, Items.END_CRYSTAL));
            put(SLOW, "obsidian", count(p, Items.OBSIDIAN));
            put(SLOW, "arrows", count(p, Items.ARROW) + count(p, Items.SPECTRAL_ARROW) + count(p, Items.TIPPED_ARROW));
            put(SLOW, "xpbottles", count(p, Items.EXPERIENCE_BOTTLE));
            put(SLOW, "heldtotal", held.isEmpty() ? 0 : count(p, held.getItem()));
            String biome = mc.level.getBiome(pos).unwrapKey().map(k -> Component.translatable(k.identifier().toLanguageKey("biome")).getString()).orElse("");
            put(SLOW, "biome", biome);
            Runtime rt = Runtime.getRuntime();
            long used = (rt.totalMemory() - rt.freeMemory()) >> 20, max = rt.maxMemory() >> 20;
            put(SLOW, "memory", used + "/" + max + " MB");
            put(SLOW, "memoryused", used);
            put(SLOW, "memorymax", max);
            put(SLOW, "gpu", com.mojang.blaze3d.systems.RenderSystem.getDevice().getDeviceInfo().name());
            put(SLOW, "memorypercent", max > 0 ? used * 100 / max : 0);
            int free = 0;
            for (int i = 0; i < 36; i++) {
                if (inv.getItem(i).isEmpty()) {
                    free++;
                }
            }
            put(SLOW, "freeslots", free);
        }
        b.append(SLOW);
    }
}
