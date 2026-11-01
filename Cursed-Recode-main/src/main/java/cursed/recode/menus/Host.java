package cursed.recode.menus;

import com.google.gson.JsonObject;
import com.mojang.blaze3d.systems.RenderSystem;
import com.mojang.renderpearl.api.GpuFormat;
import com.mojang.renderpearl.api.textures.FilterMode;
import com.mojang.renderpearl.api.textures.GpuTexture;
import com.mojang.renderpearl.api.textures.GpuTextureView;
import cursed.recode.CursedRecode;
import cursed.recode.launch.LaunchScreen;
import cursed.recode.render.GuiFrame;
import cursed.recode.render.Native;
import java.nio.charset.StandardCharsets;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.input.KeyEvent;
import net.minecraft.client.input.MouseButtonEvent;
import org.jspecify.annotations.Nullable;

/**
 * One of the game's screens drawn by the menus: its page opened with its data, the data sent again when it changes,
 * a frame each time the game draws the screen (the menu's valley under it out of a world), what the page asks for done.
 */
public final class Host {
    final Screen screen;
    final String kind;
    /** The menus' number for it (a new one each time the game shows it again: a new page). */
    int id = MenuHost.nextId();
    private static final boolean STALLS = cursed.recode.dev.Dev.on("CURSED_DEV_STALLS");
    final Page page;
    private boolean opened;
    private @Nullable String sent;
    private long sentAt;
    private final float[] said = new float[64];
    private boolean typing;
    /** The menus' picture (one for every page: they come one at a time). */
    private static @Nullable GpuTexture texture;
    private static @Nullable GpuTextureView view;

    Host(Screen screen, String kind) {
        this.screen = screen;
        this.kind = kind;
        this.page = Kinds.page(kind, screen);
    }

    private static float[] pointer() {
        Minecraft mc = Minecraft.getInstance();
        return new float[] {(float) mc.mouseHandler.xpos(), (float) mc.mouseHandler.ypos()};
    }

    /** The data to the page: all of it the first time, after that only when it changed (at most 20 times a second). */
    private void send(boolean now) {
        long t = System.nanoTime();
        if (this.opened && !now && t - this.sentAt < 50_000_000L) {
            return;
        }
        this.sentAt = t;
        JsonObject d;
        long t0 = System.nanoTime();
        try {
            d = this.page.data();
            if (cursed.recode.dev.Dev.on("CURSED_DEV_STALLS") && System.nanoTime() - t0 > 8_000_000L) {
                CursedRecode.LOGGER.info("[stall] the {} page's data in {} ms", this.kind, (System.nanoTime() - t0) / 1_000_000);
            }
        } catch (RuntimeException e) {
            CursedRecode.LOGGER.error("[menus] the {} page's data", this.kind, e);
            d = new JsonObject();
        }
        String s = d.toString();
        if (!this.opened) {
            this.opened = true;
            MenuHost.syncLanguage();
            if (cursed.recode.dev.Dev.on("CURSED_DEV_MENUS")) {
                CursedRecode.LOGGER.info("[menus dev] open {} #{} ({})", this.kind, this.id, this.screen.getClass().getSimpleName());
            }
            Native.menusOpen(this.id, this.kind, s.getBytes(StandardCharsets.UTF_8));
        } else if (!s.equals(this.sent)) {
            Native.menusData(this.id, s.getBytes(StandardCharsets.UTF_8));
        }
        this.sent = s;
    }

    void extract(GuiGraphicsExtractor graphics) {
        Minecraft mc = Minecraft.getInstance();
        int w = mc.getWindow().getWidth(), h = mc.getWindow().getHeight();
        if (w <= 0 || h <= 0) {
            return;
        }
        this.send(false);
        float[] p = pointer();
        Native.menusMouse(p[0], p[1], 0, 0);
        // (going into a world its level is there before it is played: the page is over the launch's picture, not over it)
        boolean world = mc.level != null && !cursed.recode.launch.Transition.enterBackground();
        long tf = System.nanoTime();
        int frame = Native.menusFrame(w, h, LaunchScreen.menusClock(), inp(this.page.inp()), world, this.said);
        if (STALLS && System.nanoTime() - tf > 8_000_000L) {
            CursedRecode.LOGGER.info("[stall] the menus' frame ({}) in {} ms", this.kind, (System.nanoTime() - tf) / 1_000_000);
        }
        Toasts.framed = true;
        java.util.List<String> asked = new java.util.ArrayList<>();
        MenuHost.poll(asked::add);
        this.typing(this.said[49] > 0.5F);
        if (!world || cursed.recode.launch.Transition.enterBackground()) {
            MenuWorld.draw(graphics, w, h, this.said);
        }
        blit(graphics, frame, w, h);
        try {
            Extras.page = this.id;
            this.page.extras(graphics, this.said);
            // the pages going away still draw their items and faces as they sink
            Menus.leavingExtras(this, graphics, this.said);
        } finally {
            Extras.page = 0;
        }
        head(graphics, this.said);
        // what the page asked for: done by the game's loop before its next frame, as the game's own clicks are (not in
        // the middle of this one: leaving the world here would leave the frame drawing a world that is gone)
        for (String a : asked) {
            mc.schedule(() -> {
                if (mc.gui.screen() == this.screen) {
                    this.act(a);
                }
            });
        }
    }

    static float[] inp(float[] page) {
        return MenuHost.inp(page);
    }

    /** The menus' frame `frame` over the game's GUI. */
    static void blit(GuiGraphicsExtractor graphics, int frame, int w, int h) {
        if (frame == 0) {
            return;
        }
        if (texture == null || texture.getWidth(0) != w || texture.getHeight(0) != h) {
            if (view != null) {
                view.close();
            }
            if (texture != null) {
                texture.close();
            }
            texture = RenderSystem.getDevice().createTexture("Cursed menus", GpuTexture.USAGE_COPY_DST | GpuTexture.USAGE_TEXTURE_BINDING, GpuFormat.RGBA8_UNORM, w, h, 1, 1);
            view = RenderSystem.getDevice().createTextureView(texture);
        }
        GuiFrame.sendMenus(texture, frame);
        Minecraft mc = Minecraft.getInstance();
        int gw = mc.getWindow().getGuiScaledWidth(), gh = mc.getWindow().getGuiScaledHeight();
        graphics.blit(view, RenderSystem.getSamplerCache().getClampToEdge(FilterMode.NEAREST), 0, 0, gw, gh, 0.0F, 1.0F, 0.0F, 1.0F);
    }

    /** The last page sinking away over the game (its screen is gone): frames with no screen. */
    static void leftover(GuiGraphicsExtractor graphics) {
        Minecraft mc = Minecraft.getInstance();
        int w = mc.getWindow().getWidth(), h = mc.getWindow().getHeight();
        float[] said = new float[64];
        int frame = Native.menusFrame(w, h, LaunchScreen.menusClock(), inp(Page.NONE), mc.level != null, said);
        Toasts.framed = true;
        MenuHost.poll(a -> { });
        blit(graphics, frame, w, h);
        try {
            Menus.leavingExtras(null, graphics, said);
        } finally {
            Extras.page = 0;
        }
        head(graphics, said);
    }

    /**
     * The account's head where the pause or the main menu says (the game draws it): here for whichever screen is up,
     * so it fades with its page as that page goes. The account in use, as the nickname by it (Alt Manager's: in a
     * world it may be another than the one playing, the next join's).
     */
    private static void head(GuiGraphicsExtractor graphics, float[] said) {
        Minecraft mc = Minecraft.getInstance();
        if (said[18] > 0.01F && said[17] > 0) {
            double s = mc.getWindow().getGuiScale();
            net.minecraft.world.entity.player.PlayerSkin skin = cursed.recode.alts.Alts.skinNow(mc);
            net.minecraft.client.gui.components.PlayerFaceExtractor.extractRenderState(graphics, skin, (int) Math.round(said[15] / s), (int) Math.round(said[16] / s),
                    Math.max(1, (int) Math.round(said[17] / s)), net.minecraft.util.ARGB.white(said[18]));
        }
    }

    /** The text field of the page has the keys: the window takes text (SDL's text input on). */
    private void typing(boolean on) {
        if (on != this.typing) {
            this.typing = on;
            if (on) {
                Minecraft.getInstance().textInputManager().startTextInput(this);
            } else {
                Minecraft.getInstance().textInputManager().stopTextInput(this);
            }
        }
    }

    boolean mouse(MouseButtonEvent event, int action, boolean doubleClick) {
        if (action == 1 && this.page.mouse(event.button())) {
            this.send(true);
            return true;
        }
        float[] p = pointer();
        Native.menusMouse(p[0], p[1], MenuHost.button(event.button()) + (doubleClick ? 16 : 0), action);
        return true;
    }

    boolean key(KeyEvent event) {
        if (this.page.key(event)) {
            this.send(true);
            return true;
        }
        MenuHost.key(event);
        return true;
    }

    /** What the page asked for: a verb and the rest of the line. */
    void act(String line) {
        int sp = line.indexOf(' ');
        String verb = sp < 0 ? line : line.substring(0, sp), rest = sp < 0 ? "" : line.substring(sp + 1);
        try {
            if (verb.equals("esc")) {
                if (this.screen.shouldCloseOnEsc()) {
                    this.screen.onClose();
                }
            } else {
                this.page.act(verb, rest);
            }
        } catch (RuntimeException e) {
            CursedRecode.LOGGER.error("[menus] the {} page's «{}»", this.kind, line, e);
        }
        this.send(true);
    }

    /** The game shows this screen again (it went back to it): its page opens anew, with the page's own state. */
    void reshow() {
        if (this.opened) {
            this.opened = false;
            this.sent = null;
            this.id = MenuHost.nextId();
        }
    }

    void gone() {
        this.typing(false);
        this.page.gone();
    }

    /** Its page's extras while it sinks away after its screen went. */
    void extrasLeaving(GuiGraphicsExtractor graphics, float[] said) {
        try {
            this.page.extras(graphics, said);
        } catch (RuntimeException e) {
            CursedRecode.LOGGER.warn("[menus] the {} page's extras as it goes", this.kind, e);
        }
    }
}
