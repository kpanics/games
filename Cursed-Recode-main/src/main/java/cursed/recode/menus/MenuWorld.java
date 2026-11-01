package cursed.recode.menus;

import com.mojang.blaze3d.systems.RenderSystem;
import com.mojang.renderpearl.api.GpuFormat;
import com.mojang.renderpearl.api.textures.FilterMode;
import com.mojang.renderpearl.api.textures.GpuTexture;
import com.mojang.renderpearl.api.textures.GpuTextureView;
import cursed.recode.launch.LaunchScreen;
import cursed.recode.render.LaunchFrame;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import org.jspecify.annotations.Nullable;

/**
 * The main menu's valley under the menus' screens out of a world: the launch's picture (launch/mod.rs) on the main
 * menu's clocks, the camera at the screen's place and the world as the screen asks (the page's knobs: a new world's
 * terrain, the settings' render distance, clouds, brightness…).
 */
public final class MenuWorld {
    private static @Nullable GpuTexture texture;
    private static @Nullable GpuTextureView view;
    private static float curX = 0.62F, curY = 0.55F, curSX = 0.62F, curSY = 0.55F, curOn, lookX, lookY;
    private static float last = -1;
    private static double flashAt = -99, nextFlash;
    /** The page's world: 24 knobs and the camera's place (menus/mod.rs out POSE, KNOBS). */
    private static final float[] KNOBS = new float[29];
    /** When a world was last drawn (ns): the valley comes up out of the dark after it, not all at once. */
    private static volatile long worldSeenAt;

    /** Each frame of the game (GuiRendererMixin): whether a world is drawn. */
    public static void frame() {
        if (Minecraft.getInstance().level != null) {
            worldSeenAt = System.nanoTime();
        }
    }

    /** How much of the valley is seen (0..1): from the dark once a world was just left (Save and Quit). */
    public static float shown() {
        if (worldSeenAt == 0 || cursed.recode.launch.Transition.enterBackground()) {
            return 1.0F;
        }
        float k = Math.max(0.0F, Math.min(1.0F, ((System.nanoTime() - worldSeenAt) / 1e9F - 0.05F) / 0.9F));
        return k * k * (3.0F - 2.0F * k);
    }

    private MenuWorld() {
    }

    private static float toward(float v, float target, float rate, float dt) {
        return v + (target - v) * Math.min(1.0F, dt * rate);
    }

    static void draw(GuiGraphicsExtractor graphics, int w, int h, float[] said) {
        if (!LaunchFrame.active()) {
            return;
        }
        if (texture == null || texture.getWidth(0) != w || texture.getHeight(0) != h) {
            if (view != null) {
                view.close();
            }
            if (texture != null) {
                texture.close();
            }
            texture = RenderSystem.getDevice().createTexture("Cursed menus world", GpuTexture.USAGE_COPY_DST | GpuTexture.USAGE_TEXTURE_BINDING, GpuFormat.RGBA8_UNORM, w, h, 1, 1);
            view = RenderSystem.getDevice().createTextureView(texture);
        }
        Minecraft mc = Minecraft.getInstance();
        float now = LaunchScreen.menusNow();
        float dt = last < 0 ? 0.016F : Math.max(0.0F, Math.min(0.1F, now - last));
        last = now;
        // the cursor: a small fire in the world, the camera leans toward it
        curX = (float) mc.mouseHandler.xpos() / w;
        curY = (float) mc.mouseHandler.ypos() / h;
        lookX = toward(lookX, (Math.max(0, Math.min(1, curX)) - 0.5F) * 0.05F, 1.8F, dt);
        lookY = toward(lookY, -(Math.max(0, Math.min(1, curY)) - 0.5F) * 0.028F, 1.8F, dt);
        curSX = toward(curSX, curX, 5.0F, dt);
        curSY = toward(curSY, curY, 5.0F, dt);
        curOn = toward(curOn, 1.0F, 2.0F, dt);
        float t = LaunchScreen.cardNow();
        float[] hover = new float[10];
        System.arraycopy(said, 1, hover, 0, 10);
        System.arraycopy(said, 20, KNOBS, 0, 29);
        float ui = Math.max(1.0F, h / 1300.0F);
        // going into a world: the valley unloads into the line, the loading runs along it (Transition)
        cursed.recode.launch.Transition.enterScene();
        LaunchFrame.send(texture, t, LaunchScreen.fxNow(), LaunchScreen.lifeNow(), dt, LaunchFrame.LAUNCHED | LaunchFrame.MENUS_FIRE, 1.0F, new float[] {curSX, curSY, curOn},
                new float[] {lookX, lookY}, said[11], flash(now), shown(), ui, said[13], said[14], hover, 1.0F, new float[0], 0, NO_ICONS, new float[0], 0, KNOBS);
        int gw = mc.getWindow().getGuiScaledWidth(), gh = mc.getWindow().getGuiScaledHeight();
        graphics.blit(view, RenderSystem.getSamplerCache().getClampToEdge(FilterMode.NEAREST), 0, 0, gw, gh, 0.0F, 1.0F, 0.0F, 1.0F);
    }

    private static final float[] NO_ICONS = {0, 0, 0, -1, 0, 0, 0, -1, 0, 0, 0, -1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0};

    /** Sheet lightning now and then, as over the main menu (Hide Lightning Flashes turns it off). */
    private static float flash(double now) {
        Minecraft mc = Minecraft.getInstance();
        if (mc.options.hideLightningFlash().get()) {
            return 0.0F;
        }
        if (nextFlash == 0 || now < flashAt - 1) {
            nextFlash = now + 4 + Math.random() * 5;
        }
        if (now >= nextFlash) {
            flashAt = now;
            nextFlash = now + 7 + Math.random() * 9;
        }
        double x = now - flashAt;
        return (float) ((pulse(x) + 0.55 * pulse(x - 0.2) + 0.3 * pulse(x - 0.45)) * 0.9);
    }

    private static double pulse(double y) {
        return y < 0 ? 0 : Math.exp(-y * 6.5) * (1 - Math.exp(-y * 45));
    }
}
