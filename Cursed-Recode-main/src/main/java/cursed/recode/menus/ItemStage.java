package cursed.recode.menus;

import com.mojang.blaze3d.ProjectionType;
import com.mojang.blaze3d.platform.Lighting;
import com.mojang.blaze3d.systems.RenderSystem;
import com.mojang.blaze3d.vertex.PoseStack;
import com.mojang.math.Axis;
import com.mojang.renderpearl.api.GpuFormat;
import com.mojang.renderpearl.api.commands.RenderPass;
import com.mojang.renderpearl.api.device.GpuDevice;
import com.mojang.renderpearl.api.textures.FilterMode;
import com.mojang.renderpearl.api.textures.GpuTexture;
import com.mojang.renderpearl.api.textures.GpuTextureView;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.OptionalDouble;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.navigation.ScreenRectangle;
import net.minecraft.client.gui.render.GuiRenderer;
import net.minecraft.client.gui.render.TextureSetup;
import net.minecraft.client.multiplayer.ClientLevel;
import net.minecraft.client.renderer.Projection;
import net.minecraft.client.renderer.ProjectionMatrixBuffer;
import net.minecraft.client.renderer.RenderPipelines;
import net.minecraft.client.renderer.SubmitNodeStorage;
import net.minecraft.client.renderer.entity.state.EntityRenderState;
import net.minecraft.client.renderer.entity.state.LivingEntityRenderState;
import net.minecraft.client.renderer.feature.FeatureRenderDispatcher;
import net.minecraft.client.renderer.item.ItemStackRenderState;
import net.minecraft.client.renderer.state.gui.BlitRenderState;
import net.minecraft.client.renderer.state.gui.GuiRenderState;
import net.minecraft.client.renderer.state.gui.pip.PictureInPictureRenderState;
import net.minecraft.client.renderer.state.level.CameraRenderState;
import net.minecraft.client.renderer.texture.OverlayTexture;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EntitySpawnReason;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.item.ItemDisplayContext;
import net.minecraft.world.item.ItemStack;
import org.joml.Matrix3x2f;
import org.joml.Quaternionf;
import org.jspecify.annotations.Nullable;

/**
 * Real models for the client's own pictures, turned any way and fading for real: items, and creatures as the
 * inventory draws the player. The game's own renderer draws each frame's into one texture (each in a cell the size it
 * is on the screen, as the game's item atlas does), and every one is a blit of its cell with its own alpha — the
 * game's GUI items cannot fade (they were made smaller instead) nor turn, and it draws no creature but the player.
 * The same as Cursed 1.21.11's item stage, through 26.3's pictures in pictures: a stage's things of a frame are one
 * picture-in-picture state (added where the first of them was asked for), drawn when the GUI is prepared
 * (GuiRendererMixin), its blits landing in that layer.
 *
 * <p>Two stages, for two layers: the menus' pages ({@link #MENUS}) and what is over everything, the notifications
 * ({@link #OVER}) — one state each, so that each lands in its own layer.
 *
 * <p>A stage's picture is kept while nothing in it changes (the same models in the same poses and cells, nothing
 * animated): things that only move or fade cost a blit each. Creatures breathe: drawn again every frame.
 */
public final class ItemStage {
    public static final ItemStage MENUS = new ItemStage("menus");
    public static final ItemStage OVER = new ItemStage("over");

    /** The texture's width, and its tallest. */
    private static final int WIDTH = 1024;
    private static final int MAX_HEIGHT = 2048;
    /** A cell is this much bigger than its thing's slot: room to turn in. */
    public static final float ROOM = 1.3F;
    /** What moves on its own (a glint, a compass) is drawn again at most this often (ns). */
    private static final long MOVING = 1_000_000_000L / 60;
    private static final boolean STALLS = cursed.recode.dev.Dev.on("CURSED_DEV_STALLS");
    /** One creature of each kind to draw (made, never added to the world), for the world they were made in. */
    private static final Map<EntityType<?>, Entity> MOBS = new HashMap<>();
    private static @Nullable ClientLevel mobsLevel;
    /** Kinds that could not be made or drawn: not tried again (the rest of the stage goes on). */
    private static final java.util.Set<EntityType<?>> FAILED = new java.util.HashSet<>();

    private final String label;
    private @Nullable GpuTexture color;
    private @Nullable GpuTextureView colorView;
    private @Nullable GpuTexture depth;
    private @Nullable GpuTextureView depthView;
    private int height;
    private final Projection projection = new Projection();
    private @Nullable ProjectionMatrixBuffer projectionBuffer;
    private final SubmitNodeStorage storage = new SubmitNodeStorage();
    private boolean broken;
    /** The frame's state, until the game's GUI state is begun again. */
    private @Nullable Frame pending;
    /** What the picture holds now: each cell's model and pose. */
    private final List<Object> drawn = new ArrayList<>();
    private long drawnAt;
    private final List<Model> models = new ArrayList<>();

    private ItemStage(String label) {
        this.label = label;
    }

    /**
     * One thing: an item or a creature; its slot's middle and side (GUI units), its cell (texture px), how it is
     * turned (degrees), how seen.
     */
    record Shot(@Nullable ItemStack stack, @Nullable EntityType<?> mob, @Nullable Entity live, float cx, float cy, float side, int u, int v, int cell, float yaw, float pitch, float alpha, int tint) {
    }

    /** An item's state that keeps what names its model (as the game's TrackingItemStackRenderState, emptied with it). */
    private static final class Model extends ItemStackRenderState {
        final List<Object> identity = new ArrayList<>();

        @Override
        public void appendModelIdentityElement(Object element) {
            this.identity.add(element);
        }

        @Override
        public void clear() {
            super.clear();
            this.identity.clear();
        }
    }

    /** A stage's things of a frame: one picture in picture over the whole screen. */
    public static final class Frame implements PictureInPictureRenderState {
        final ItemStage stage;
        final List<Shot> shots = new ArrayList<>();
        private final int w;
        private final int h;
        // the shelves the cells are laid on
        private int x;
        private int y;
        private int row;

        Frame(ItemStage stage, int w, int h) {
            this.stage = stage;
            this.w = w;
            this.h = h;
        }

        @Override
        public int x0() {
            return 0;
        }

        @Override
        public int x1() {
            return this.w;
        }

        @Override
        public int y0() {
            return 0;
        }

        @Override
        public int y1() {
            return this.h;
        }

        @Override
        public float scale() {
            return 1.0F;
        }

        @Override
        public @Nullable ScreenRectangle scissorArea() {
            return null;
        }

        @Override
        public @Nullable ScreenRectangle bounds() {
            return new ScreenRectangle(0, 0, this.w, this.h);
        }

        int used() {
            return this.y + this.row;
        }
    }

    /** The game's GUI state is begun again: a new frame for every stage. */
    public static void newFrame() {
        MENUS.pending = null;
        OVER.pending = null;
    }

    /** The things of a frame into its stage's texture, and a blit of each (GuiRenderer.prepare). */
    public static void prepare(Frame f, GuiRenderState gui, FeatureRenderDispatcher features) {
        f.stage.prepareFrame(f, gui, features);
    }

    /** An item turned in an inspection (Custom Inventory): as {@link #add}, on the menus' stage. */
    public static boolean inspect(GuiGraphicsExtractor g, ItemStack stack, float x, float y, float size, float yaw, float pitch, float alpha) {
        return MENUS.add(g, stack, x, y, size, yaw, pitch, alpha);
    }

    /**
     * An item at window px (`size` its slot's side), turned `yaw` round and tilted `pitch` (degrees), seen `alpha`.
     * False when it cannot be drawn so (the stage failed, or is full): the caller draws it the game's way.
     */
    boolean add(GuiGraphicsExtractor g, @Nullable ItemStack stack, float x, float y, float size, float yaw, float pitch, float alpha) {
        if (this.broken) {
            return false;
        }
        if (stack == null || stack.isEmpty() || alpha <= 0.004F) {
            return true;
        }
        return this.shot(g, stack, null, null, x, y, size, yaw, pitch, alpha, 0xFFFFFF);
    }

    /**
     * A creature of `type` at window px, standing in a square `size` wide, turned `yaw` (its face to the viewer at 0)
     * and seen from `pitch` above. False when it cannot be (no world to make one in, the stage failed or is full).
     */
    boolean addMob(GuiGraphicsExtractor g, EntityType<?> type, float x, float y, float size, float yaw, float pitch, float alpha) {
        if (this.broken || mob(type) == null) {
            return false;
        }
        if (alpha <= 0.004F) {
            return true;
        }
        return this.shot(g, null, type, null, x, y, size, yaw, pitch, alpha, 0xFFFFFF);
    }

    /**
     * A creature that is in the world, as it is (its armour, what it holds), standing in a square `size` wide, turned
     * and seen as {@link #addMob}, its light tinted `tint` (RGB).
     */
    public boolean addEntity(GuiGraphicsExtractor g, Entity entity, float x, float y, float size, float yaw, float pitch, float alpha, int tint) {
        if (this.broken || Minecraft.getInstance().level == null) {
            return false;
        }
        if (alpha <= 0.004F) {
            return true;
        }
        return this.shot(g, null, entity.getType(), entity, x, y, size, yaw, pitch, alpha, tint);
    }

    private boolean shot(GuiGraphicsExtractor g, @Nullable ItemStack stack, @Nullable EntityType<?> mob, @Nullable Entity live, float x, float y, float size, float yaw, float pitch, float alpha, int tint) {
        Minecraft mc = Minecraft.getInstance();
        float s = (float) mc.getWindow().getGuiScale();
        int cell = Math.clamp((int) Math.ceil(size * ROOM), 16, mob != null ? 512 : 320);
        Frame f = this.pending;
        if (f == null) {
            f = new Frame(this, mc.getWindow().getGuiScaledWidth(), mc.getWindow().getGuiScaledHeight());
            g.guiRenderState.addPicturesInPictureState(f);
            this.pending = f;
        }
        if (f.x + cell > WIDTH) {
            f.x = 0;
            f.y += f.row;
            f.row = 0;
        }
        if (f.y + cell > MAX_HEIGHT) {
            return false;
        }
        f.shots.add(new Shot(stack, mob, live, (x + size / 2.0F) / s, (y + size / 2.0F) / s, size / s, f.x, f.y, cell, yaw, pitch, alpha, tint));
        f.x += cell;
        f.row = Math.max(f.row, cell);
        return true;
    }

    /** The one creature of `type` to draw (made once in this world), or null when there is no world or it cannot be. */
    private static @Nullable Entity mob(EntityType<?> type) {
        Minecraft mc = Minecraft.getInstance();
        if (mc.level == null) {
            return null;
        }
        if (mobsLevel != mc.level) {
            MOBS.clear();
            mobsLevel = mc.level;
        }
        if (FAILED.contains(type)) {
            return null;
        }
        return MOBS.computeIfAbsent(type, t -> {
            try {
                Entity e = t.create(mc.level, EntitySpawnReason.LOAD);
                if (e != null) {
                    // a number of its own, out of the world's way (its renderer asks for one)
                    e.setId(-1_000_000 - MOBS.size());
                }
                return e;
            } catch (RuntimeException e) {
                FAILED.add(t);
                return null;
            }
        });
    }

    private void prepareFrame(Frame f, GuiRenderState gui, FeatureRenderDispatcher features) {
        if (f.shots.isEmpty()) {
            return;
        }
        try {
            Minecraft mc = Minecraft.getInstance();
            int need = Math.max(256, Integer.highestOneBit(Math.max(1, f.used() - 1)) << 1);
            boolean fresh = false;
            if (this.color == null || this.height < need) {
                this.close();
                GpuDevice device = RenderSystem.getDevice();
                this.height = Math.min(MAX_HEIGHT, need);
                this.color = device.createTexture(() -> "Cursed stage " + this.label, 13, GpuFormat.RGBA8_UNORM, WIDTH, this.height, 1, 1);
                this.colorView = device.createTextureView(this.color);
                this.depth = device.createTexture(() -> "Cursed stage depth " + this.label, 9, GpuFormat.D32_FLOAT, WIDTH, this.height, 1, 1);
                this.depthView = device.createTextureView(this.depth);
                this.projectionBuffer = new ProjectionMatrixBuffer("Cursed stage " + this.label);
                fresh = true;
            }
            // what each cell is to hold; the same as the picture holds (and nothing animated a sixtieth ago): kept
            while (this.models.size() < f.shots.size()) {
                this.models.add(new Model());
            }
            List<Object> wanted = new ArrayList<>(f.shots.size());
            boolean animated = false;
            for (int i = 0; i < f.shots.size(); i++) {
                Shot shot = f.shots.get(i);
                Model model = this.models.get(i);
                if (shot.stack() != null) {
                    mc.getItemModelResolver().updateForTopItem(model, shot.stack(), ItemDisplayContext.GUI, mc.level, null, i);
                    animated |= model.isAnimated();
                } else {
                    model.clear();
                    // a creature breathes: drawn again each frame
                    animated = true;
                }
                wanted.add(List.of(new ArrayList<>(model.identity), String.valueOf(shot.mob()), shot.u(), shot.v(), shot.cell(), shot.yaw(), shot.pitch()));
            }
            long now = System.nanoTime();
            if (fresh || !wanted.equals(this.drawn) || animated && now - this.drawnAt >= MOVING) {
                long t0 = System.nanoTime();
                this.draw(f, features, mc);
                if (STALLS && System.nanoTime() - t0 > 8_000_000L) {
                    cursed.recode.CursedRecode.LOGGER.info("[stall] stage {}: {} things in {} ms (fresh {})", this.label, f.shots.size(), (System.nanoTime() - t0) / 1_000_000, fresh);
                }
                this.drawn.clear();
                this.drawn.addAll(wanted);
                this.drawnAt = now;
            }
            // each thing: its cell over its slot, the cell's room around it
            TextureSetup texture = TextureSetup.singleTexture(this.colorView, RenderSystem.getSamplerCache().getClampToEdge(FilterMode.LINEAR));
            for (Shot shot : f.shots) {
                float k = shot.side() * ROOM / 16.0F;
                Matrix3x2f pose = new Matrix3x2f().translate(shot.cx(), shot.cy()).scale(k, k);
                float u0 = shot.u() / (float) WIDTH;
                float u1 = (shot.u() + shot.cell()) / (float) WIDTH;
                float v0 = 1.0F - shot.v() / (float) this.height;
                float v1 = 1.0F - (shot.v() + shot.cell()) / (float) this.height;
                int a = Math.round(Math.clamp(shot.alpha(), 0.0F, 1.0F) * 255.0F);
                // premultiplied: the colour fades with the alpha (and takes the shot's tint)
                int tr = (shot.tint() >> 16 & 255) * a / 255, tg = (shot.tint() >> 8 & 255) * a / 255, tb = (shot.tint() & 255) * a / 255;
                int tint = a << 24 | tr << 16 | tg << 8 | tb;
                gui.addBlitToCurrentLayer(new BlitRenderState(RenderPipelines.GUI_TEXTURED_PREMULTIPLIED_ALPHA, texture, pose, -8, -8, 8, 8, u0, u1, v0, v1, tint, null));
            }
        } catch (Throwable t) {
            this.broken = true;
            this.drawn.clear();
            cursed.recode.CursedRecode.LOGGER.error("[menus] the {} stage failed and is off (things drawn the game's way)", this.label, t);
        }
    }

    /** Every thing into its cell: items lit as blocks in one pass, the flat ones in another, creatures in a third. */
    private void draw(Frame f, FeatureRenderDispatcher features, Minecraft mc) {
        GpuDevice device = RenderSystem.getDevice();
        int used = Math.min(this.height, f.used());
        // the rows in use, from the bottom of the texture (its rows count up)
        device.createCommandEncoder().clearColorAndDepthTextures(this.color, GuiRenderer.CLEAR_COLOR, this.depth, 0.0, 0, this.height - used, WIDTH, used, 0);
        this.projection.setupOrtho(-1000.0F, 1000.0F, WIDTH, this.height, true);
        RenderSystem.setProjectionMatrix(this.projectionBuffer.getBuffer(this.projection), ProjectionType.ORTHOGRAPHIC);
        PoseStack pose = new PoseStack();
        for (int pass = 0; pass < 3; pass++) {
            boolean any = false;
            for (int i = 0; i < f.shots.size(); i++) {
                Shot shot = f.shots.get(i);
                if (pass == 2) {
                    any |= shot.mob() != null && this.submitMob(shot, pose, mc);
                    continue;
                }
                Model model = this.models.get(i);
                if (shot.stack() == null || model.isEmpty() || model.usesBlockLight() != (pass == 0)) {
                    continue;
                }
                pose.pushPose();
                float unit = shot.cell() / ROOM;
                pose.translate(shot.u() + shot.cell() / 2.0F, shot.v() + shot.cell() / 2.0F, 0.0F);
                pose.scale(unit, -unit, unit);
                if (shot.pitch() != 0.0F) {
                    pose.rotate(Axis.XP.rotationDegrees(shot.pitch()));
                }
                if (shot.yaw() != 0.0F) {
                    pose.rotate(Axis.YP.rotationDegrees(shot.yaw()));
                }
                model.submit(pose, this.storage, 15728880, OverlayTexture.NO_OVERLAY, 0);
                pose.popPose();
                any = true;
            }
            if (!any) {
                continue;
            }
            mc.gameRenderer.lighting().setupFor(pass == 0 ? Lighting.Entry.ITEMS_3D : pass == 1 ? Lighting.Entry.ITEMS_FLAT : Lighting.Entry.ENTITY_IN_UI);
            try (
                FeatureRenderDispatcher.PreparedFrame frame = features.prepareFrame(this.storage);
                RenderPass pass_ = device.createCommandEncoder().createRenderPass(() -> "Cursed stage " + this.label, this.colorView, Optional.empty(), this.depthView, OptionalDouble.empty())
            ) {
                RenderSystem.bindDefaultUniforms(pass_);
                FeatureRenderDispatcher.renderAllFeatures(pass_, frame);
            }
        }
    }

    /**
     * A creature into its cell as the inventory draws the player (GuiEntityRenderer): its whole box fits the cell
     * whichever way it turns, it breathes with the clock.
     */
    private boolean submitMob(Shot shot, PoseStack pose, Minecraft mc) {
        Entity e = shot.live() != null ? shot.live() : mob(shot.mob());
        if (e == null) {
            return false;
        }
        var dispatcher = mc.getEntityRenderDispatcher();
        @SuppressWarnings("unchecked")
        var renderer = (net.minecraft.client.renderer.entity.EntityRenderer<Entity, EntityRenderState>) dispatcher.getRenderer(e);
        EntityRenderState state;
        try {
            state = renderer.createRenderState(e, 1.0F);
        } catch (RuntimeException ex) {
            FAILED.add(shot.mob());
            cursed.recode.CursedRecode.LOGGER.warn("[menus] a {} cannot be drawn in the stage", shot.mob(), ex);
            return false;
        }
        state.shadowPieces.clear();
        state.outlineColor = 0;
        // shown, not in the world: no fire on it (a zombie burns in the sun) and no glow round it
        state.displayFireAnimation = false;
        state.ageInTicks = (System.nanoTime() % 3_600_000_000_000L) / 5.0E7F;
        if (state instanceof LivingEntityRenderState living) {
            living.bodyRot = 180.0F;
            living.yRot = 0.0F;
            living.xRot = 0.0F;
            living.boundingBoxWidth = living.boundingBoxWidth / living.scale;
            living.boundingBoxHeight = living.boundingBoxHeight / living.scale;
            living.scale = 1.0F;
        }
        float pitch = shot.pitch() * ((float) Math.PI / 180.0F);
        CameraRenderState camera = new CameraRenderState();
        camera.orientation = new Quaternionf().rotateX(pitch).conjugate(new Quaternionf()).rotateY((float) Math.PI);
        // a model reaches past its box (a wolf is longer than it is wide, a witch's hat stands above it)
        float fit = Math.max(0.6F, Math.max(state.boundingBoxHeight, state.boundingBoxWidth * 1.6F));
        float size = shot.cell() / ROOM * 0.86F / fit;
        pose.pushPose();
        pose.translate(shot.u() + shot.cell() / 2.0F, shot.v() + shot.cell() / 2.0F, 0.0F);
        pose.scale(size, size, -size);
        pose.translate(0.0F, state.boundingBoxHeight / 2.0F, 0.0F);
        pose.rotate(new Quaternionf().rotateZ((float) Math.PI).mul(new Quaternionf().rotateX(pitch)));
        pose.rotate(Axis.YP.rotationDegrees(shot.yaw()));
        cursed.recode.chams.ChamsHooks.guiStart();
        try {
            dispatcher.submit(state, camera, 0.0, 0.0, 0.0, pose, this.storage);
        } finally {
            cursed.recode.chams.ChamsHooks.guiDone();
        }
        pose.popPose();
        return true;
    }

    private void close() {
        if (this.color != null) {
            this.colorView.close();
            this.color.close();
            this.depthView.close();
            this.depth.close();
            this.projectionBuffer.close();
        }
        this.color = null;
        this.colorView = null;
        this.depth = null;
        this.depthView = null;
        this.projectionBuffer = null;
        this.drawn.clear();
    }
}
