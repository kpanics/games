package cursed.recode.alts;

import com.mojang.authlib.GameProfile;
import com.mojang.authlib.exceptions.AuthenticationException;
import com.mojang.authlib.minecraft.UserApiService;
import com.mojang.authlib.services.MinecraftServicesDiscoveryService;
import cursed.recode.CursedRecode;
import cursed.recode.mixin.client.MinecraftSessionAccess;
import cursed.recode.render.Native;
import java.util.Map;
import java.util.Optional;
import java.util.Set;
import java.util.UUID;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.ConcurrentHashMap;
import net.minecraft.client.Minecraft;
import net.minecraft.client.User;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.components.PlayerFaceExtractor;
import net.minecraft.client.multiplayer.ProfileKeyPairManager;
import net.minecraft.client.multiplayer.chat.report.ReportEnvironment;
import net.minecraft.client.multiplayer.chat.report.ReportingContext;
import net.minecraft.client.resources.DefaultPlayerSkin;
import net.minecraft.util.ARGB;
import net.minecraft.core.UUIDUtil;
import net.minecraft.util.Util;
import net.minecraft.world.entity.player.PlayerSkin;

/**
 * Alt Manager's Java half (native alts.rs keeps the list and draws it): the account in use put to the game — its session,
 * a nickname's offline one or a Microsoft account's own (Msa) — when the menus ask (native "poll"), at the start the one
 * last used; the faces of the list by their names (Mojang's profiles, as a player's head shows one).
 */
public final class Alts {
    private Alts() {
    }

    private static boolean loaded;
    /** The faces by name (lower case): the skin found, the default one till then; those asked for once. */
    private static final Map<String, PlayerSkin> SKINS = new ConcurrentHashMap<>();
    private static final Set<String> ASKED = ConcurrentHashMap.newKeySet();

    /** Once, before the main menu shows the nickname (and from the first tick, whichever comes first). */
    public static void load(Minecraft mc) {
        if (loaded) {
            return;
        }
        loaded = true;
        String name = Native.guiCall("alts\tload\t" + mc.getUser().getName() + "\t" + mc.gameDirectory.getAbsolutePath());
        if (name != null && !name.isEmpty()) {
            use(mc, name);
        }
    }

    /** Each tick: a session that waited for the world to be left, then what the menus asked for. */
    public static void tick(Minecraft mc) {
        load(mc);
        if (pending != null && !inWorld(mc)) {
            User u = pending;
            pending = null;
            set(mc, u, pendingLicensed);
        }
        String asks = Native.guiCall("alts\tpoll");
        if (asks == null || asks.isEmpty()) {
            return;
        }
        for (String line : asks.split("\n")) {
            if (line.startsWith("use ")) {
                use(mc, line.substring(4).trim());
            } else if (line.equals("login")) {
                Msa.login(mc);
            }
        }
    }

    /** The account put to use: a Microsoft one's session renewed in the background (its nickname stands in till it
     *  comes), a nickname's offline session at once. */
    static void use(Minecraft mc, String name) {
        offline(mc, name);
        String uuid = Native.guiCall("alts\tmsuuid\t" + name);
        if (uuid != null && !uuid.isEmpty()) {
            Msa.renew(mc, name);
        }
    }

    static void offline(Minecraft mc, String name) {
        set(mc, new User(name, UUIDUtil.createOfflinePlayerUUID(name), "0", Optional.empty(), Optional.empty()), false);
    }

    /** Mojang's services, for a licence's own (once: the game keeps its own to itself). */
    private static MinecraftServicesDiscoveryService discovery;
    /**
     * A session waiting for the world to be left, and whether it is a licence's: the one joined keeps its own (chat's
     * "me", the player being the local one, the key chat is signed with, the reports' log), a new one is the next join's.
     */
    private static User pending;
    private static boolean pendingLicensed;

    private static boolean inWorld(Minecraft mc) {
        return mc.getConnection() != null || mc.level != null;
    }

    /** The account to be played: the one waiting for the next join, else the session's. */
    static String wanted(Minecraft mc) {
        return pending != null ? pending.getName() : mc.getUser().getName();
    }

    /**
     * The game's session now (in a world: once it is left): who plays (a licence's token or "0"), their profile asked
     * anew for a licence; and what the game keeps by the account made anew with it — its services and their flags, the
     * key chat is signed with (none for a nickname: the last licence's would be sent for it), reports.
     */
    static void set(Minecraft mc, User user, boolean licensed) {
        if (inWorld(mc)) {
            pending = user;
            pendingLicensed = licensed;
            CursedRecode.LOGGER.info("[alts] the account for the next join: {}{}", user.getName(), licensed ? " (licensed)" : "");
            return;
        }
        pending = null;
        var acc = (MinecraftSessionAccess) mc;
        acc.cursed$setUser(user);
        acc.cursed$setProfileFuture(licensed
                ? CompletableFuture.supplyAsync(() -> mc.services().sessionService().fetchProfile(user.getProfileId(), true), Util.nonCriticalIoPool())
                : CompletableFuture.completedFuture(null));
        if (licensed && discovery == null) {
            discovery = MinecraftServicesDiscoveryService.create(mc.getProxy(), true);
        }
        UserApiService api = licensed ? discovery.createUserApiService(user.getAccessToken()) : UserApiService.OFFLINE;
        acc.cursed$setUserApiService(api);
        acc.cursed$setUserPropertiesFuture(licensed ? CompletableFuture.supplyAsync(() -> {
            try {
                return api.fetchProperties();
            } catch (AuthenticationException e) {
                CursedRecode.LOGGER.warn("[alts] the account's flags could not be fetched", e);
                return UserApiService.OFFLINE_PROPERTIES;
            }
        }, Util.nonCriticalIoPool()) : CompletableFuture.completedFuture(UserApiService.OFFLINE_PROPERTIES));
        acc.cursed$setProfileKeyPairManager(licensed ? ProfileKeyPairManager.create(api, user, mc.gameDirectory.toPath()) : ProfileKeyPairManager.EMPTY_KEY_MANAGER);
        acc.cursed$setReportingContext(ReportingContext.create(ReportEnvironment.local(), api));
        CursedRecode.LOGGER.info("[alts] the account in use: {}{}", user.getName(), licensed ? " (licensed)" : "");
    }

    /** "status" to the menus: 0 working, 1 done, 2 wrong; the words (English, put in the game's language there), what
     *  goes after them as it is. */
    static void status(int kind, String words, String detail) {
        Native.guiCall("alts\tstatus\t" + kind + "\t" + words.replace('\t', ' ') + "\t" + (detail == null ? "" : detail.replace('\t', ' ')));
    }

    private static String listedText = "";
    private static java.util.List<String[]> listed = java.util.List.of();

    /** The accounts as the list has them: each name and its Microsoft profile's id ("" for a nickname). */
    public static java.util.List<String[]> listed() {
        String s = Native.guiCall("alts\tlist");
        if (s == null || s.equals(listedText)) {
            return listed;
        }
        listedText = s;
        java.util.List<String[]> out = new java.util.ArrayList<>();
        try {
            for (var e : com.google.gson.JsonParser.parseString(s).getAsJsonObject().getAsJsonArray("accounts")) {
                var o = e.getAsJsonObject();
                out.add(new String[] {o.get("name").getAsString(), o.get("uuid").getAsString()});
            }
        } catch (RuntimeException e) {
            CursedRecode.LOGGER.warn("[alts] the list could not be read", e);
        }
        listed = out;
        return out;
    }

    /** The face of the account in use (the corner's head; in a world the next join's): a licence's by its id, a
     *  nickname's by its name. */
    public static PlayerSkin skinNow(Minecraft mc) {
        User u = pending != null ? pending : mc.getUser();
        boolean licensed = !"0".equals(u.getAccessToken()) && !u.getAccessToken().isEmpty();
        return skin(mc, u.getName(), licensed ? u.getProfileId().toString().replace("-", "") : "");
    }

    /** A name's face: the skin of the profile of that name (Mojang's), the default skin till it is found or if there is
     *  none; a Microsoft account's by its own id. */
    public static PlayerSkin skin(Minecraft mc, String name, String uuid) {
        String key = name.toLowerCase(java.util.Locale.ROOT);
        PlayerSkin known = SKINS.get(key);
        if (known != null) {
            return known;
        }
        PlayerSkin fallback = DefaultPlayerSkin.get(uuid.isEmpty() ? UUIDUtil.createOfflinePlayerUUID(name) : undashed(uuid));
        if (ASKED.add(key)) {
            CompletableFuture.supplyAsync(() -> {
                Optional<GameProfile> p = uuid.isEmpty() ? mc.services().profileResolver().fetchByName(name) : Optional.empty();
                UUID id = uuid.isEmpty() ? p.map(GameProfile::id).orElse(null) : undashed(uuid);
                return id == null ? Optional.<GameProfile>empty() : mc.services().profileResolver().fetchById(id);
            }, Util.nonCriticalIoPool()).thenCompose(p -> p.isPresent() ? mc.getSkinManager().get(p.get()) : CompletableFuture.completedFuture(Optional.empty()))
                    .thenAccept(s -> s.ifPresent(found -> SKINS.put(key, found)))
                    .exceptionally(e -> null);
        }
        return fallback;
    }

    /**
     * The faces of the client's menu's card of accounts (native altsui.rs, "altfaces"): each where it says, cut to the
     * list's part as it scrolls.
     */
    public static void faces(GuiGraphicsExtractor g, Minecraft mc, float scale) {
        String all = Native.guiCall("altfaces");
        if (all == null || all.isEmpty()) {
            return;
        }
        java.util.List<String[]> list = listed();
        for (String line : all.split("\n")) {
            String[] f = line.split(" ");
            if (f.length < 9) {
                continue;
            }
            int i;
            float x, y, w, alpha, c0x, c0y, c1x, c1y;
            try {
                i = Integer.parseInt(f[0]);
                x = Float.parseFloat(f[1]) / scale;
                y = Float.parseFloat(f[2]) / scale;
                w = Float.parseFloat(f[3]) / scale;
                alpha = Float.parseFloat(f[4]);
                c0x = Float.parseFloat(f[5]) / scale;
                c0y = Float.parseFloat(f[6]) / scale;
                c1x = Float.parseFloat(f[7]) / scale;
                c1y = Float.parseFloat(f[8]) / scale;
            } catch (NumberFormatException e) {
                continue;
            }
            if (i < 0 || i >= list.size() || alpha <= 0.02F || y >= c1y || y + w <= c0y) {
                continue;
            }
            boolean whole = y >= c0y && y + w <= c1y;
            if (!whole) {
                g.enableScissor((int) Math.ceil(c0x), (int) Math.ceil(c0y), (int) Math.floor(c1x), (int) Math.floor(c1y));
            }
            PlayerFaceExtractor.extractRenderState(g, skin(mc, list.get(i)[0], list.get(i)[1]), Math.round(x), Math.round(y), Math.max(1, Math.round(w)), ARGB.white(alpha));
            if (!whole) {
                g.disableScissor();
            }
        }
    }

    private static UUID undashed(String id) {
        try {
            return id.length() == 32 ? com.mojang.util.UndashedUuid.fromStringLenient(id) : UUID.fromString(id);
        } catch (IllegalArgumentException e) {
            return new UUID(0L, 0L);
        }
    }
}
