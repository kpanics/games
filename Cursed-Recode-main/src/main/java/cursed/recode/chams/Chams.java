package cursed.recode.chams;

import com.mojang.renderpearl.api.pipeline.ColorTargetState;
import com.mojang.renderpearl.api.pipeline.DepthStencilState;
import com.mojang.renderpearl.api.pipeline.RenderPipeline;
import com.mojang.renderpearl.api.pipeline.ShaderType;
import cursed.recode.render.EntityEngine;
import cursed.recode.render.Native;
import java.util.HashMap;
import java.util.IdentityHashMap;
import java.util.Map;
import java.util.Optional;
import net.minecraft.client.Minecraft;
import net.minecraft.client.renderer.ShaderDefines;
import net.minecraft.client.renderer.entity.state.ArrowRenderState;
import net.minecraft.client.renderer.entity.state.AvatarRenderState;
import net.minecraft.client.renderer.entity.state.BoatRenderState;
import net.minecraft.client.renderer.entity.state.EndCrystalRenderState;
import net.minecraft.client.renderer.entity.state.EntityRenderState;
import net.minecraft.client.renderer.entity.state.ExperienceOrbRenderState;
import net.minecraft.client.renderer.entity.state.FallingBlockRenderState;
import net.minecraft.client.renderer.entity.state.FireworkRocketRenderState;
import net.minecraft.client.renderer.entity.state.ItemEntityRenderState;
import net.minecraft.client.renderer.entity.state.ItemFrameRenderState;
import net.minecraft.client.renderer.entity.state.LivingEntityRenderState;
import net.minecraft.client.renderer.entity.state.LlamaSpitRenderState;
import net.minecraft.client.renderer.entity.state.MinecartRenderState;
import net.minecraft.client.renderer.entity.state.ShulkerBulletRenderState;
import net.minecraft.client.renderer.entity.state.ThrownItemRenderState;
import net.minecraft.client.renderer.entity.state.ThrownTridentRenderState;
import net.minecraft.client.renderer.entity.state.TntRenderState;
import net.minecraft.client.renderer.entity.state.WitherSkullRenderState;
import net.minecraft.client.renderer.rendertype.RenderSetup;
import net.minecraft.client.renderer.rendertype.RenderType;
import net.minecraft.resources.Identifier;
import net.minecraft.core.Holder;
import net.minecraft.tags.EntityTypeTags;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.EntityTypes;
import net.minecraft.world.entity.MobCategory;
import net.minecraft.world.entity.player.Player;
import org.jspecify.annotations.Nullable;

/**
 * Chams (the menu's module "chams", native/src/gui/data.rs): bodies in ink let into water — whose (oneself in the
 * third person, one's hands, players, teammates, NPCs and mannequins, hostile and peaceful mobs, within a distance),
 * which of their parts (the body, armour, what they hold, the cape and elytra, the skin's outer layer), and what of the
 * ink (the pattern on the body, the smoke, the threads), each as the player sets it. Nothing new is drawn for a body:
 * while one is submitted (ChamsSubmitMixin, round EntityRenderDispatcher.submit and the first person's hands), each
 * render type of a part in the look is swapped for its twin — the same textures, the same pipeline with CURSED_CHAMS
 * defined — and the engine's twin compiler gives that pipeline the matter (native/shaders/engine/chams.glsl through
 * EntityEngine). The twins keep the game's depth test: only what is seen. The smoke and threads are the engine's
 * (ChamsInk, native/src/engine/ink.rs).
 */
public final class Chams {
    /** The shader's kinds (chams.glsl CURSED_CHAMS): the ink, the curse, the polymorphic liquid. */
    public static final int INK = 2;
    public static final int CURSED = 3;
    public static final int LIQUID = 4;
    public static final int SHADOW = 5;
    /** The menu's looks (its "Look", in its order). */
    public static final int LOOK_INK = 0;
    public static final int LOOK_LIQUID = 1;
    public static final int LOOK_CURSED = 2;
    public static final int LOOK_SHADOW = 3;
    /** The parts of a body, as its layers are submitted (ChamsPartMixin): what is not in a layer is the body. */
    public static final int BODY = 0;
    public static final int ARMOUR = 1;
    public static final int ITEM = 2;
    public static final int CAPE = 3;

    /** The module's settings, asked of the menu four times a second. */
    static final class Settings {
        boolean on;
        int look = LOOK_INK;
        boolean self = true;
        boolean hands = true;
        boolean players = true;
        boolean team = true;
        boolean npcs;
        boolean villagers;
        boolean hostile;
        boolean peaceful;
        boolean items;
        boolean projectiles;
        boolean orbs;
        boolean things;
        double distance = 64.0;
        final boolean[] parts = {true, true, true, true};
        boolean outerLayer;
        boolean pattern = true;
        boolean smoke = true;
        boolean threads = true;
        boolean onlyMe;
        int bodies = 8;
        boolean drops = true;
        boolean traces = true;

        /** The shader's kind a body in the look is drawn with (0: as the game draws it: the ink without its pattern). */
        int bodyKind() {
            return switch (look) {
                case LOOK_CURSED -> CURSED;
                case LOOK_LIQUID -> LIQUID;
                case LOOK_SHADOW -> SHADOW;
                default -> pattern ? INK : 0;
            };
        }

        /** Whether the engine wants the bodies (ChamsInk): the ink's smoke or threads, the drops, the curse's strands and
         *  ash, the traces. */
        boolean engineBodies() {
            return traces || look == LOOK_INK && (smoke || threads) || look == LOOK_LIQUID || look == LOOK_CURSED || look == LOOK_SHADOW;
        }
    }

    static Settings settings = new Settings();
    private static long polled;
    /** Dev (DevCommands "chams N"): on (2) or off (0) whatever the menu says, below 0 the menu's. */
    public static int devLook = -1;
    /** The kind of what is submitted now (0 nothing of the chams'): set round a body's submit. */
    static int kind;
    /** The part being submitted (its layer's, ChamsPartMixin), and the ones round it. */
    private static final int[] PARTS = new int[16];
    private static int depth;
    /**
     * Each kind's twins (a render type's twin is of the kind it was made for: the looks change); null for one not drawn
     * in that kind (the enchantments' shine; in the shadow the game's glowing eyes).
     */
    private static final Map<Integer, Map<RenderType, RenderType>> TWINS = new HashMap<>();
    /** Every twin made: what is drawn with the look goes with the opaque world (ChamsSubmitMixin). */
    private static final Map<RenderType, Boolean> MADE = new IdentityHashMap<>();
    /** This frame's items of a chams' body and their kinds (ItemFeatureRendererMixin swaps their types). */
    private static final Map<Object, Integer> ITEMS = new IdentityHashMap<>();

    private Chams() {
    }

    /** The settings, fresh at most every quarter of a second. */
    static Settings settings() {
        long now = System.nanoTime();
        if (now - polled > 250_000_000L) {
            polled = now;
            settings = read(Native.guiCall("set\tchams"));
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
        String look = v.get("Look");
        o.look = "Polymorph".equals(look) ? LOOK_LIQUID : "Cursed".equals(look) ? LOOK_CURSED : "Shadow".equals(look) ? LOOK_SHADOW : LOOK_INK;
        o.drops = on(v, "Drops", o.drops);
        o.traces = on(v, "Traces on the ground", o.traces);
        o.self = on(v, "Yourself (F5)", o.self);
        o.hands = on(v, "Your hands", o.hands);
        o.players = on(v, "Players", o.players);
        o.team = on(v, "Teammates", o.team);
        o.npcs = on(v, "NPCs and mannequins", o.npcs);
        o.hostile = on(v, "Hostile mobs", o.hostile);
        o.peaceful = on(v, "Peaceful mobs", o.peaceful);
        o.villagers = on(v, "Villagers and golems", o.villagers);
        o.items = on(v, "Items on the ground", o.items);
        o.projectiles = on(v, "Arrows and projectiles", o.projectiles);
        o.orbs = on(v, "Experience orbs", o.orbs);
        o.things = on(v, "Boats, minecarts and the rest", o.things);
        o.distance = number(v, "Distance", o.distance);
        o.parts[BODY] = on(v, "Body", true);
        o.parts[ARMOUR] = on(v, "Armour", true);
        o.parts[ITEM] = on(v, "Held item", true);
        o.parts[CAPE] = on(v, "Cape and elytra", true);
        o.outerLayer = on(v, "Skin's outer layer", o.outerLayer);
        o.pattern = on(v, "Pattern on the body", o.pattern);
        o.smoke = on(v, "Smoke", o.smoke);
        o.threads = on(v, "Threads", o.threads);
        o.onlyMe = "Only me".equals(v.get("Effects from"));
        o.bodies = (int) Math.round(number(v, "Bodies with effects", o.bodies));
        // dev: a kind whatever the menu says (2 the ink, 3 the curse, 4 the liquid; anything else off)
        String dev = System.getenv("CURSED_DEV_CHAMS");
        int forced = devLook >= 0 ? devLook : dev == null ? -1 : parse(dev.strip());
        if (forced >= 0) {
            o.on = forced == INK || forced == CURSED || forced == LIQUID || forced == SHADOW;
            o.look = forced == CURSED ? LOOK_CURSED : forced == LIQUID ? LOOK_LIQUID : forced == SHADOW ? LOOK_SHADOW : LOOK_INK;
        }
        return o;
    }

    private static int parse(String s) {
        try {
            return Integer.parseInt(s);
        } catch (NumberFormatException e) {
            return -1;
        }
    }

    private static boolean on(Map<String, String> v, String key, boolean def) {
        String s = v.get(key);
        return s == null ? def : s.equals("1");
    }

    private static double number(Map<String, String> v, String key, double def) {
        try {
            String s = v.get(key);
            return s == null ? def : Double.parseDouble(s);
        } catch (NumberFormatException e) {
            return def;
        }
    }

    /**
     * Whether a body being submitted is in the look: whose it is (one's own, an NPC or a mannequin — something drawn as
     * a player that is not one, or a player the server does not list —, a teammate, another player, a hostile or a
     * peaceful mob) and how far it is.
     */
    static boolean wants(EntityRenderState s) {
        Settings o = settings();
        if (!o.on) {
            return false;
        }
        Minecraft mc = Minecraft.getInstance();
        // one's own body whatever the distance (the camera behind it is further than a short one)
        if (s instanceof AvatarRenderState a && mc.player != null && a.id == mc.player.getId()) {
            return o.self;
        }
        if (s.distanceToCameraSq > o.distance * o.distance) {
            return false;
        }
        if (s instanceof AvatarRenderState a) {
            Entity e = mc.level == null ? null : mc.level.getEntity(a.id);
            if (!(e instanceof Player p) || mc.getConnection() == null || mc.getConnection().getPlayerInfo(p.getUUID()) == null) {
                return o.npcs;
            }
            if (mc.player != null && mc.player.isAlliedTo(p)) {
                return o.team;
            }
            return o.players;
        }
        if (s.entityType == null) {
            return false;
        }
        if (s instanceof LivingEntityRenderState) {
            EntityType<?> t = s.entityType;
            if (t == EntityTypes.VILLAGER || t == EntityTypes.WANDERING_TRADER || t == EntityTypes.IRON_GOLEM || t == EntityTypes.SNOW_GOLEM || t == EntityTypes.COPPER_GOLEM) {
                return o.villagers;
            }
            MobCategory c = t.getCategory();
            if (c == MobCategory.MONSTER) {
                return o.hostile;
            }
            // the armour stand and the like: things, not mobs
            return c == MobCategory.MISC ? o.things : o.peaceful;
        }
        if (s instanceof ItemEntityRenderState) {
            return o.items;
        }
        if (s instanceof ExperienceOrbRenderState) {
            return o.orbs;
        }
        Holder<EntityType<?>> h = s.entityType.builtInRegistryHolder();
        if (s instanceof ArrowRenderState || s instanceof ThrownItemRenderState || s instanceof ThrownTridentRenderState || s instanceof FireworkRocketRenderState
                || s instanceof WitherSkullRenderState || s instanceof ShulkerBulletRenderState || s instanceof LlamaSpitRenderState
                || h.is(EntityTypeTags.IMPACT_PROJECTILES) || h.is(EntityTypeTags.ARROWS)) {
            return o.projectiles;
        }
        // boats, minecarts, falling blocks, TNT, end crystals, frames; not what has no body (displays, clouds, markers)
        return o.things && (s instanceof BoatRenderState || s instanceof MinecartRenderState || s instanceof FallingBlockRenderState || s instanceof TntRenderState
                || s instanceof EndCrystalRenderState || s instanceof ItemFrameRenderState);
    }

    /** The kind being submitted (0 none). */
    public static int kind() {
        return kind;
    }

    /** Whether `type` is a twin made here. */
    public static boolean isTwin(RenderType type) {
        return MADE.containsKey(type);
    }

    /** An item submitted for a chams' body, with its kind. */
    public static void markItem(Object submit, int k) {
        ITEMS.put(submit, k);
    }

    /** The kind an item's submit was made for, or 0. */
    public static int itemKind(Object submit) {
        Integer k = ITEMS.isEmpty() ? null : ITEMS.get(submit);
        return k == null ? 0 : k;
    }

    /** A new frame's submits: last frame's items are drawn. */
    public static void frame() {
        ITEMS.clear();
    }

    /** Round a body's submit: its kind, then 0. */
    public static void begin(int k) {
        kind = k;
        depth = 0;
        firstModel = true;
    }

    public static void end() {
        kind = 0;
        depth = 0;
    }

    /** A layer of a body begins (`part` what it draws) and ends (ChamsPartMixin). */
    public static void partBegin(int part) {
        if (depth < PARTS.length) {
            PARTS[depth] = part;
        }
        depth++;
    }

    public static void partEnd() {
        depth = Math.max(0, depth - 1);
    }

    /** The part being submitted now; `item` an item's (outside any layer, what a hand holds). */
    public static int part(boolean item) {
        int p = depth == 0 ? BODY : PARTS[Math.min(depth, PARTS.length) - 1];
        return item && p == BODY ? ITEM : p;
    }

    /** Whether what is submitted now goes in the look: a body in it, its part one the player wants. */
    /** The first person's hands are being submitted (ChamsHooks): drawn as polygons whatever the look. */
    static boolean inHands;

    /** Whether a model submitted now is a body in the liquid: its boxes go to the engine instead of its polygons. */
    public static boolean liquidBody() {
        return kind == LIQUID && boxedBody();
    }

    /** Whether a model submitted now is a body whose boxes the engine wants (the liquid's, the curse's, the shadow's). */
    public static boolean boxedBody() {
        return (kind == LIQUID || kind == CURSED || kind == SHADOW) && !inHands && part(false) == BODY && settings.parts[BODY];
    }

    /** Whether a model submitted now is a body in the shadow: its texture's eyes are found (EyeMaps). */
    public static boolean shadowBody() {
        return kind == SHADOW && !inHands && part(false) == BODY && settings.parts[BODY];
    }

    /** The body's first model this submit (its own: what comes after is worn over it, a drowned's seaweed), once. */
    private static boolean firstModel;

    public static boolean firstModel() {
        boolean f = firstModel;
        firstModel = false;
        return f;
    }

    /** The texture a render type draws with (its Sampler0), or null. */
    public static @Nullable Identifier texture(RenderType type) {
        RenderSetup.TextureBinding b = type.state.textures.get("Sampler0");
        return b == null ? null : b.location();
    }

    public static boolean swapping(boolean item) {
        return kind != 0 && settings.parts[part(item)];
    }

    /**
     * `type` as the current kind draws it: its twin, the type itself when it is not a creature's (it stays as it is), or
     * null when it is not drawn at all (the enchantments' shine over another matter).
     */
    public static @Nullable RenderType swap(RenderType type) {
        return kind == 0 ? type : twin(type, kind);
    }

    public static @Nullable RenderType twin(RenderType type, int k) {
        Map<RenderType, RenderType> twins = TWINS.computeIfAbsent(k, kk -> new HashMap<>());
        RenderType t = twins.get(type);
        if (t != null || twins.containsKey(type)) {
            return t;
        }
        t = make(type, k);
        twins.put(type, t);
        if (t != null && t != type) {
            MADE.put(t, Boolean.TRUE);
        }
        return t;
    }

    /** The shadow's twins made again: the copies of their textures (EyeMaps) were let go. */
    static void forgetShadow() {
        Map<RenderType, RenderType> twins = TWINS.remove(SHADOW);
        if (twins != null) {
            twins.forEach((type, t) -> {
                if (t != null && t != type) {
                    MADE.remove(t);
                }
            });
        }
    }

    private static boolean defines(ShaderDefines d, String name) {
        return d.flags().contains(name) || d.values().containsKey(name);
    }

    private static @Nullable RenderType make(RenderType type, int k) {
        RenderSetup s = type.state;
        RenderPipeline p = s.pipeline;
        Identifier vertex = p.getShaders().get(ShaderType.VERTEX);
        String v = vertex == null ? "" : vertex.toString();
        if (!v.equals(EntityEngine.SHADER) && !v.equals(EntityEngine.ITEM)) {
            return type;
        }
        ShaderDefines d = p.getShaderDefines();
        if (defines(d, "GLINT")) {
            return null;
        }
        if (defines(d, "EMISSIVE") || defines(d, "DISSOLVE") || type.isOutline()) {
            // the shadow's eyes are its own: the game's glowing ones not over them
            return k == SHADOW && defines(d, "EMISSIVE") ? null : type;
        }
        ColorTargetState first = p.getColorTargetStates().isEmpty() ? null : p.getColorTargetStates().get(0);
        ColorTargetState target = first == null ? ColorTargetState.DEFAULT : new ColorTargetState(Optional.empty(), first.format(), first.writeMask());
        DepthStencilState depth = p.getDepthStencilState();
        DepthStencilState mine = depth == null ? null
                : new DepthStencilState(depth.depthTest(), depth.depthTest() != com.mojang.renderpearl.api.pipeline.CompareOp.EQUAL,
                        depth.depthBiasScaleFactor(), depth.depthBiasConstant());
        ShaderDefines.Builder defs = ShaderDefines.builder();
        d.values().forEach(defs::define);
        d.flags().forEach(defs::define);
        RenderPipeline.Snippet same = new RenderPipeline.Snippet(
                p.getShaders(),
                Optional.of(defs.build()),
                Optional.of(p.getBindGroupLayouts()),
                p.getColorTargetStates().toArray(new ColorTargetState[0]),
                p.getColorTargetStates().size(),
                Optional.empty(),
                Optional.of(p.getPolygonMode()),
                Optional.of(p.isCull()),
                p.getVertexFormatBindings().toArray(new com.mojang.renderpearl.api.vertex.VertexFormat[0]),
                Optional.of(p.getPrimitiveTopology()),
                p.pushConstantSize());
        RenderPipeline.Builder b = RenderPipeline.builder().withColorTargetState(0, target).withSnippet(same);
        if (mine != null) {
            b.withDepthStencilState(mine);
        }
        RenderPipeline pipeline = b.withShaderDefine(EntityEngine.CHAMS, k)
                .withLocation(Identifier.fromNamespaceAndPath("cursed-recode", "chams/" + p.getLocation().getPath() + "/" + k))
                .build();
        RenderSetup.RenderSetupBuilder setup = RenderSetup.builder(pipeline);
        // the shadow binds its texture's copy with the eyes marked (EyeMaps), once made
        s.textures.forEach((name, binding) -> setup.withTexture(name, k == SHADOW && name.equals("Sampler0") ? EyeMaps.marked(binding.location()) : binding.location(),
                binding.sampler()));
        if (s.useLightmap) {
            setup.useLightmap();
        }
        if (s.useOverlay) {
            setup.useOverlay();
        }
        setup.setLayeringTransform(s.layeringTransform);
        setup.setTextureTransform(s.textureTransform);
        if (s.outlineProperty != RenderSetup.OutlineProperty.NONE) {
            if (s.outlineTextureName != null) {
                setup.setOutline(s.outlineProperty, s.outlineTextureName);
            } else {
                setup.setOutline(s.outlineProperty);
            }
        }
        if (s.affectsCrumbling) {
            setup.affectsCrumbling();
        }
        // with the opaque world, whatever the game's Improved Transparency (its phases need pipelines of their own)
        setup.withForcedSolidModelPhase();
        return RenderType.create("cursed_chams_" + k + "_" + p.getLocation().getPath(), setup.createRenderSetup());
    }
}
