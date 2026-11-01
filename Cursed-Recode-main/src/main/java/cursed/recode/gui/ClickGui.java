package cursed.recode.gui;

import com.mojang.blaze3d.platform.InputConstants;
import com.mojang.blaze3d.systems.RenderSystem;
import com.mojang.renderpearl.api.GpuFormat;
import com.mojang.renderpearl.api.textures.FilterMode;
import com.mojang.renderpearl.api.textures.GpuTexture;
import com.mojang.renderpearl.api.textures.GpuTextureView;
import cursed.recode.render.GuiFrame;
import cursed.recode.render.Native;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.components.PlayerFaceExtractor;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.input.CharacterEvent;
import net.minecraft.client.input.KeyEvent;
import net.minecraft.client.input.MouseButtonEvent;
import net.minecraft.network.chat.Component;
import net.minecraft.util.ARGB;
import org.jspecify.annotations.Nullable;
import org.lwjgl.sdl.SDLKeyboard;

/**
 * The client's menu (Right Shift in a world). All of it is native (gui/mod.rs): this screen hands it the mouse and the
 * keys, puts its picture over the game (GuiFrame) and the player's head on its account row.
 */
public final class ClickGui extends Screen {
    private @Nullable GpuTexture texture;
    private @Nullable GpuTextureView view;
    private final float[] state = new float[6];
    /** The letter of the key that opened it comes to the screen as typed: not heard until the first frame. */
    private boolean opening = true;

    public ClickGui() {
        super(Component.literal("Cursed Recode"));
    }

    /** A key pressed in a world with no screen (KeyboardHandlerMixin): true if it was the menu's and opened it. */
    public static boolean opens(Minecraft minecraft, KeyEvent event) {
        if (minecraft.gui.screen() != null || minecraft.player == null || !GuiFrame.active()) {
            return false;
        }
        if (!keyName(event.key()).equals(Native.guiMenuKey())) {
            return false;
        }
        minecraft.gui.setScreen(new ClickGui());
        return true;
    }

    /** A key by SDL's name for it ("Right Shift", "R", "F5"): the same whatever the keyboard's layout. */
    public static String keyName(int scancode) {
        String n = SDLKeyboard.SDL_GetScancodeName(scancode);
        return n == null || n.isEmpty() ? "Key " + scancode : n;
    }

    private float scale() {
        return (float) this.minecraft.getWindow().getGuiScale();
    }

    @Override
    protected void init() {
        var w = this.minecraft.getWindow();
        // its sounds as loud as the game's (silent on the test stand)
        var o = this.minecraft.options;
        Native.launchVolume(cursed.recode.launch.Twin.mute() ? 0.0F : o.getSoundSourceVolume(net.minecraft.sounds.SoundSource.MASTER),
                o.getSoundSourceVolume(net.minecraft.sounds.SoundSource.MUSIC));
        Native.guiOpen(w.getWidth(), w.getHeight(), this.minecraft.getUser().getName(), this.minecraft.gameDirectory.getAbsolutePath());
        // letters come only while the game takes text (SDL_StartTextInput): the search, the fields, the codes
        this.minecraft.textInputManager().startTextInput(this);
    }

    @Override
    public boolean isPauseScreen() {
        return false;
    }

    @Override
    public boolean shouldCloseOnEsc() {
        // Esc goes to the menu: it closes a list or a module's settings first
        return false;
    }

    @Override
    public void tick() {
        cursed.recode.gui.GuiAsks.poll(this.minecraft);
        cursed.recode.swap.SwapPage.tick(this.minecraft);
        Native.guiState(this.state);
        if (this.state[0] == 0.0F && this.minecraft.gui.screen() == this) {
            // "Edit the layout" closed it: the HUD's editor comes instead
            this.minecraft.gui.setScreen(cursed.recode.hud.HudLayer.wantsEditor() ? new cursed.recode.hud.HudEditor() : null);
        }
    }

    @Override
    public void extractBackground(final GuiGraphicsExtractor graphics, final int mouseX, final int mouseY, final float a) {
    }

    @Override
    public void extractRenderState(final GuiGraphicsExtractor graphics, final int mouseX, final int mouseY, final float a) {
        var win = this.minecraft.getWindow();
        int w = win.getWidth(), h = win.getHeight();
        if (w <= 0 || h <= 0 || !GuiFrame.active()) {
            return;
        }
        if (this.texture == null || this.texture.getWidth(0) != w || this.texture.getHeight(0) != h) {
            this.release();
            this.texture = RenderSystem.getDevice().createTexture("Cursed menu", GpuTexture.USAGE_COPY_DST | GpuTexture.USAGE_TEXTURE_BINDING, GpuFormat.RGBA8_UNORM, w, h, 1, 1);
            this.view = RenderSystem.getDevice().createTextureView(this.texture);
        }
        // the pointer from the game's own place for it: an inactive window gets no moves (the stand), and they come
        // between frames anyway
        Native.guiMouse((float) this.minecraft.mouseHandler.xpos(), (float) this.minecraft.mouseHandler.ypos(), 0, 0);
        int id = Native.guiFrame(w, h);
        this.opening = false;
        if (id == 0) {
            return;
        }
        GuiFrame.send(this.texture, id);
        graphics.blit(this.view, RenderSystem.getSamplerCache().getClampToEdge(FilterMode.NEAREST), 0, 0, this.width, this.height, 0.0F, 1.0F, 0.0F, 1.0F);
        Native.guiState(this.state);
        if (this.state[4] > 0.01F && this.minecraft.player != null) {
            float s = this.scale();
            PlayerFaceExtractor.extractRenderState(graphics, this.minecraft.player.getSkin(), Math.round(this.state[1] / s), Math.round(this.state[2] / s),
                    Math.max(1, Math.round(this.state[3] / s)), ARGB.white(this.state[4]));
        }
        // Auto Swap's editor: its things
        cursed.recode.swap.SwapPage.icons(graphics, this.minecraft, this.scale());
        cursed.recode.fx.FakePlayers.preview(graphics, this.scale());
        // Alt Manager's card: the accounts' faces
        cursed.recode.alts.Alts.faces(graphics, this.minecraft, this.scale());
    }

    /** SDL's buttons (left 1, middle 2, right 3, then 4, 5) as the menu's (left 0, right 1, middle 2, then 3, 4). */
    private static int button(int sdl) {
        return switch (sdl) {
            case InputConstants.MOUSE_BUTTON_LEFT -> 0;
            case InputConstants.MOUSE_BUTTON_RIGHT -> 1;
            case InputConstants.MOUSE_BUTTON_MIDDLE -> 2;
            default -> sdl - 1;
        };
    }

    @Override
    public boolean mouseClicked(final MouseButtonEvent event, final boolean doubleClick) {
        float s = this.scale();
        Native.guiMouse((float) event.x() * s, (float) event.y() * s, button(event.button()), 1);
        return true;
    }

    @Override
    public boolean mouseReleased(final MouseButtonEvent event) {
        float s = this.scale();
        Native.guiMouse((float) event.x() * s, (float) event.y() * s, button(event.button()), 2);
        return true;
    }

    @Override
    public void mouseMoved(final double x, final double y) {
        float s = this.scale();
        Native.guiMouse((float) x * s, (float) y * s, 0, 0);
    }

    @Override
    public boolean mouseDragged(final MouseButtonEvent event, final double dx, final double dy) {
        float s = this.scale();
        Native.guiMouse((float) event.x() * s, (float) event.y() * s, 0, 0);
        return true;
    }

    @Override
    public boolean mouseScrolled(final double x, final double y, final double scrollX, final double scrollY) {
        Native.guiScroll((float) scrollY);
        return true;
    }

    @Override
    public boolean keyPressed(final KeyEvent event) {
        int k = event.key();
        String name = keyName(k);
        Native.guiKey(event.isEscape(), k == InputConstants.KEY_RETURN || k == InputConstants.KEY_NUMPADENTER, k == InputConstants.KEY_BACKSPACE, name,
                name.equals(Native.guiMenuKey()));
        return true;
    }

    @Override
    public boolean keyReleased(final KeyEvent event) {
        Native.guiCall("gkeyup\t" + keyName(event.key()));
        return true;
    }

    @Override
    public boolean charTyped(final CharacterEvent event) {
        if (!this.opening) {
            Native.guiChar(event.codepoint());
        }
        return true;
    }

    @Override
    public void removed() {
        cursed.recode.swap.SwapPage.closed();
        this.minecraft.textInputManager().stopTextInput(this);
        Native.guiClose(true);
        GuiFrame.closed();
        this.release();
    }

    private void release() {
        if (this.view != null) {
            this.view.close();
            this.view = null;
        }
        if (this.texture != null) {
            this.texture.close();
            this.texture = null;
        }
    }
}
