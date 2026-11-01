package cursed.recode.dev;

import com.mojang.blaze3d.platform.InputConstants;
import com.mojang.logging.LogUtils;
import java.io.RandomAccessFile;
import java.nio.charset.StandardCharsets;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.input.KeyEvent;
import net.minecraft.client.input.MouseButtonEvent;
import net.minecraft.client.input.MouseButtonInfo;
import org.slf4j.Logger;

/**
 * CURSED_DEV_INPUT=file: the test stand's hands. Its window is never the active one and gets nothing from the keyboard
 * or the mouse, so what a player does is written to the file instead, a line at a time ("id command…"), and done here
 * through the game's own handlers — the keyboard's, the mouse's — as the window's events would be; the log says
 * "[devin] done id" after each. The pointer's moves are handed to the open screen too (the game only does that for
 * the active window), the button held while it moves drags. Never grabs the real cursor: a click with no screen open
 * is not sent.
 *
 *   key NAME[+NAME…]     a key pressed and let go (NAME: t, e, enter, escape, backspace, up, f, 1… or key.keyboard.*;
 *                        with ctrl+ / shift+ / alt+ before it)
 *   down NAME / up NAME  a key held / let go
 *   camera first|back|front  the view as F5 would set it
 *   type TEXT            letters typed
 *   move X Y             the pointer to window px X, Y
 *   click X Y [B]        pressed and let go there (B: 1 left, 2 middle, 3 right; shift+click… with modifiers)
 *   press X Y [B] / release X Y [B]
 *   drag X0 Y0 X1 Y1 [B] pressed at the first, moved in steps, let go at the second
 *   scroll DY [X Y]      the wheel (DY lines; up is positive)
 *   open NAME [TITLE]    a screen (DevCommands.screen: c-chest, c-furnace…, pause, options…); close — none
 *   cmd /COMMAND         a command sent as the player would
 *   say TEXT             a chat message sent
 *   mine S               the block looked at broken as the held attack button would, a tick at a time, for S seconds
 *   use                  the held item used on the block looked at (the right button)
 *   mouse down|up use|attack  the use or the attack key held or let go in the world, as the
 *                        game's own (the use key…) — the cursor never grabbed
 *   swing                the main hand swung, as an attack does
 *   turn DX DY           the mouse turned in the world (the game's turn, as the mouse's; Free Look takes it)
 */
public final class DevInput {
    private static final Logger LOGGER = LogUtils.getLogger();
    private static final String FILE = Dev.on("CURSED_DEV_INPUT") ? System.getenv("CURSED_DEV_INPUT") : null;
    private static long read;
    private static int held = -1;
    /** mine: until when, the game's tick it last went on in. */
    private static long mineUntil;
    private static long mineTick = -1;

    private DevInput() {
    }

    /** Each frame (the render thread): the lines written since, done in order. */
    public static void frame(Minecraft mc) {
        if (FILE == null) {
            return;
        }
        mining(mc);
        String chunk;
        try (RandomAccessFile f = new RandomAccessFile(FILE, "r")) {
            long len = f.length();
            if (len < read) {
                read = 0;
            }
            if (len == read) {
                return;
            }
            byte[] b = new byte[(int) (len - read)];
            f.seek(read);
            f.readFully(b);
            int end = b.length;
            while (end > 0 && b[end - 1] != '\n') {
                end--;
            }
            if (end == 0) {
                return;
            }
            read += end;
            chunk = new String(b, 0, end, StandardCharsets.UTF_8);
        } catch (Exception e) {
            return;
        }
        for (String line : chunk.split("\n")) {
            line = line.strip();
            if (line.isEmpty()) {
                continue;
            }
            String[] p = line.split(" ", 3);
            String id = p[0];
            String cmd = p.length > 1 ? p[1] : "";
            String arg = p.length > 2 ? p[2] : "";
            try {
                run(mc, cmd, arg);
                LOGGER.info("[devin] done {}", id);
            } catch (Throwable t) {
                LOGGER.warn("[devin] failed {} ({} {})", id, cmd, arg, t);
                LOGGER.info("[devin] done {}", id);
            }
        }
    }

    /** The breaking going on, once a tick (as the held button's), while mine's time lasts. */
    private static void mining(Minecraft mc) {
        if (mineUntil == 0 || mc.level == null || mc.gameMode == null) {
            return;
        }
        if (System.nanoTime() > mineUntil) {
            mineUntil = 0;
            mineTick = -1;
            mc.gameMode.stopDestroyBlock();
            return;
        }
        long tick = mc.level.getGameTime();
        if (tick == mineTick || !(mc.hitResult instanceof net.minecraft.world.phys.BlockHitResult hit) || hit.getType() == net.minecraft.world.phys.HitResult.Type.MISS) {
            return;
        }
        if (mineTick == -1) {
            mc.gameMode.startDestroyBlock(hit.getBlockPos(), hit.getDirection());
        } else {
            mc.gameMode.continueDestroyBlock(hit.getBlockPos(), hit.getDirection());
        }
        mineTick = tick;
    }

    private static void run(Minecraft mc, String cmd, String arg) {
        long h = mc.getWindow().handle();
        String[] a = arg.isEmpty() ? new String[0] : arg.split(" ");
        switch (cmd) {
            case "key" -> {
                int[] k = key(arg);
                KeyEvent e = event(k);
                mc.keyboardHandler.keyPress(h, 1, e);
                mc.keyboardHandler.keyPress(h, 0, e);
            }
            case "down", "up" -> {
                int[] k = key(arg);
                mc.keyboardHandler.keyPress(h, cmd.equals("down") ? 1 : 0, event(k));
            }
            case "type" -> mc.keyboardHandler.textInput(h, arg);
            case "move" -> move(mc, Double.parseDouble(a[0]), Double.parseDouble(a[1]));
            case "click", "press", "release" -> {
                int[] b = button(a.length > 2 ? a[2] : "1");
                move(mc, Double.parseDouble(a[0]), Double.parseDouble(a[1]));
                if (mc.gui.screen() == null) {
                    LOGGER.info("[devin] no screen open: the click is not sent (it would grab the cursor)");
                    return;
                }
                if (!cmd.equals("release")) {
                    mc.mouseHandler.onButton(h, new MouseButtonInfo(b[0], b[1]), 1);
                    held = b[0];
                }
                if (!cmd.equals("press")) {
                    mc.mouseHandler.onButton(h, new MouseButtonInfo(b[0], b[1]), 0);
                    held = -1;
                }
            }
            case "drag" -> {
                double x0 = Double.parseDouble(a[0]), y0 = Double.parseDouble(a[1]), x1 = Double.parseDouble(a[2]), y1 = Double.parseDouble(a[3]);
                int[] b = button(a.length > 4 ? a[4] : "1");
                move(mc, x0, y0);
                if (mc.gui.screen() == null) {
                    return;
                }
                mc.mouseHandler.onButton(h, new MouseButtonInfo(b[0], b[1]), 1);
                held = b[0];
                for (int i = 1; i <= 12; i++) {
                    move(mc, x0 + (x1 - x0) * i / 12.0, y0 + (y1 - y0) * i / 12.0);
                }
                mc.mouseHandler.onButton(h, new MouseButtonInfo(b[0], b[1]), 0);
                held = -1;
            }
            case "scroll" -> {
                if (a.length > 2) {
                    move(mc, Double.parseDouble(a[1]), Double.parseDouble(a[2]));
                }
                mc.mouseHandler.onScroll(h, 0.0, Double.parseDouble(a[0]));
            }
            case "open" -> {
                String[] q = arg.split(" ", 2);
                var screen = DevCommands.screen(mc, q[0], q.length > 1 ? q[1] : null);
                // a command with no screen (the toasts, the music) leaves the main menu as it is: none there is a new
                // title screen Fabric's events were not made for (a crash)
                if (screen != null || mc.level != null) {
                    mc.gui.setScreen(screen);
                }
            }
            case "close" -> mc.gui.setScreen(null);
            case "mine" -> mineUntil = System.nanoTime() + (long) (Double.parseDouble(arg) * 1e9);
            case "use" -> {
                if (mc.hitResult instanceof net.minecraft.world.phys.BlockHitResult hit && hit.getType() != net.minecraft.world.phys.HitResult.Type.MISS) {
                    mc.gameMode.useItemOn(mc.player, net.minecraft.world.InteractionHand.MAIN_HAND, hit);
                }
            }
            case "mouse" -> {
                net.minecraft.client.KeyMapping m = a[1].equals("attack") ? mc.options.keyAttack : mc.options.keyUse;
                boolean down = a[0].equals("down");
                m.setDown(down);
                if (down) {
                    net.minecraft.client.KeyMapping.click(m.getDefaultKey());
                }
            }
            // the mouse turned in the world by so much (the game's turn, as the mouse's: "turn 300 0")
            case "turn" -> {
                if (mc.player != null && a.length >= 2) {
                    mc.player.turn(Double.parseDouble(a[0]), Double.parseDouble(a[1]));
                }
            }
            case "swing" -> {
                if (mc.player != null) {
                    mc.player.swing(net.minecraft.world.InteractionHand.MAIN_HAND, mc.player.getMainHandItem().getAttackAnimation(), false);
                }
            }
            // Auto Swap's swaps anew (the menu's JSON, gui/swap.rs)
            case "swaps" -> cursed.recode.render.Native.guiCall("swapput\t" + arg);
            // a module's setting by its label, as the menu sets it: "set chams|Smoke=0", "set hb-health|on=0" (gui/mod.rs "put")
            case "set" -> {
                String[] mv = arg.split("\\|", 2);
                if (mv.length == 2) {
                    cursed.recode.render.Native.guiCall("put\t" + mv[0] + "\t" + mv[1]);
                }
            }
            case "camera" -> mc.options.setCameraType(switch (arg) {
                case "back" -> net.minecraft.client.CameraType.THIRD_PERSON_BACK;
                case "front" -> net.minecraft.client.CameraType.THIRD_PERSON_FRONT;
                default -> net.minecraft.client.CameraType.FIRST_PERSON;
            });
            case "cmd" -> {
                if (mc.getConnection() != null) {
                    mc.getConnection().sendCommand(arg.startsWith("/") ? arg.substring(1) : arg);
                }
            }
            case "say" -> {
                if (mc.getConnection() != null) {
                    mc.getConnection().sendChat(arg);
                }
            }
            default -> LOGGER.warn("[devin] what is {}?", cmd);
        }
    }

    /** The pointer moved: the game's handler keeps where it is; the open screen hears the move (and the drag). */
    private static void move(Minecraft mc, double x, double y) {
        var w = mc.getWindow();
        double ox = mc.mouseHandler.xpos(), oy = mc.mouseHandler.ypos();
        mc.mouseHandler.onMove(w.handle(), x, y, x - ox, y - oy);
        Screen s = mc.gui.screen();
        if (s != null && mc.gui.overlay() == null) {
            double sx = x * w.getGuiScaledWidth() / w.getScreenWidth();
            double sy = y * w.getGuiScaledHeight() / w.getScreenHeight();
            s.mouseMoved(sx, sy);
            if (held >= 0) {
                double dx = (x - ox) * w.getGuiScaledWidth() / w.getScreenWidth();
                double dy = (y - oy) * w.getGuiScaledHeight() / w.getScreenHeight();
                s.mouseDragged(new MouseButtonEvent(sx, sy, new MouseButtonInfo(held, 0)), dx, dy);
            }
            s.afterMouseMove();
        }
    }

    /** A key as SDL gives it: the place on the keyboard, the key it is there (the shortcuts read it), the modifiers. */
    private static KeyEvent event(int[] k) {
        return new KeyEvent(k[0], org.lwjgl.sdl.SDLKeyboard.SDL_GetKeyFromScancode(k[0], (short) k[1], false), k[1]);
    }

    /** "ctrl+shift+f" → the key and the modifiers (SDL's: shift 1, ctrl 64, alt 256). */
    private static int[] key(String spec) {
        int mods = 0;
        String name = spec.toLowerCase();
        while (true) {
            if (name.startsWith("ctrl+")) {
                mods |= 64;
                name = name.substring(5);
            } else if (name.startsWith("shift+")) {
                mods |= 1;
                name = name.substring(6);
            } else if (name.startsWith("alt+")) {
                mods |= 256;
                name = name.substring(4);
            } else {
                break;
            }
        }
        String full = switch (name) {
            case "enter", "return" -> "key.keyboard.enter";
            case "esc", "escape" -> "key.keyboard.escape";
            case "bs", "backspace" -> "key.keyboard.backspace";
            case "tab" -> "key.keyboard.tab";
            case "space" -> "key.keyboard.space";
            case "lshift" -> "key.keyboard.left.shift";
            case "rshift" -> "key.keyboard.right.shift";
            case "lctrl" -> "key.keyboard.left.control";
            case "lalt" -> "key.keyboard.left.alt";
            case "up", "down", "left", "right", "home", "end", "delete", "insert" -> "key.keyboard." + name;
            case "pgup" -> "key.keyboard.page.up";
            case "pgdn" -> "key.keyboard.page.down";
            case "slash" -> "key.keyboard.slash";
            default -> name.startsWith("key.") ? name : "key.keyboard." + name;
        };
        return new int[] {InputConstants.getKey(full).getValue(), mods};
    }

    private static int[] button(String spec) {
        int mods = 0;
        String b = spec.toLowerCase();
        if (b.startsWith("shift+")) {
            mods |= 1;
            b = b.substring(6);
        }
        if (b.startsWith("ctrl+")) {
            mods |= 64;
            b = b.substring(5);
        }
        return new int[] {Integer.parseInt(b), mods};
    }
}
