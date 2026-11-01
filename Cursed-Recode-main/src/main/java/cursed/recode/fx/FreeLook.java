package cursed.recode.fx;

import cursed.recode.render.Native;
import net.minecraft.client.CameraType;
import net.minecraft.client.Minecraft;
import net.minecraft.client.player.LocalPlayer;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.level.ClipContext;
import net.minecraft.world.level.Level;
import net.minecraft.world.phys.HitResult;
import net.minecraft.world.phys.Vec3;

/**
 * Free Look (native gui/freelook.rs decides): each frame, before the camera is placed, it is asked where the camera
 * looks and how far behind it is; the game's view is set for it while it looks (put back as it was after). The
 * mouse's turns and the wheel go to it instead of the player while it looks (FreeLookCameraMixin, FreeLookTurnMixin,
 * FreeLookScrollMixin).
 */
public final class FreeLook {
    private static boolean on;
    private static float yaw;
    private static float pitch;
    private static float dist;
    private static float height;
    /** The game's view before it looked (put back after); null while it does not. */
    private static CameraType kept;
    private static long last;

    private FreeLook() {
    }

    /** Each frame, before the camera is placed. */
    public static void frame(Minecraft mc) {
        long now = System.nanoTime();
        float dt = last == 0L ? 0.016F : Math.min((now - last) / 1.0E9F, 0.1F);
        last = now;
        LocalPlayer p = mc.player;
        if (p == null || mc.level == null) {
            if (on) {
                Native.guiCall("fl\treset");
            }
            end(mc);
            return;
        }
        boolean first = (kept != null ? kept : mc.options.getCameraType()).isFirstPerson();
        String said = Native.guiCall("fl\tframe\t" + dt + "\t" + p.getYRot() + "\t" + p.getXRot() + "\t" + (mc.gui.screen() != null ? "1" : "0") + "\t" + (first ? "1" : "0"));
        int view = -1;
        if (said != null && !said.isEmpty()) {
            for (String l : said.split("\n")) {
                String[] f = l.split(" ");
                try {
                    if (f[0].equals("cam") && f.length >= 5) {
                        yaw = Float.parseFloat(f[1]);
                        pitch = Float.parseFloat(f[2]);
                        dist = Float.parseFloat(f[3]);
                        view = Integer.parseInt(f[4]);
                        height = f.length >= 6 ? Float.parseFloat(f[5]) : 0.0F;
                        if (!Float.isFinite(yaw) || !Float.isFinite(pitch) || !Float.isFinite(dist) || !Float.isFinite(height)) {
                            view = -1;
                        }
                    } else if (f[0].equals("turn") && f.length >= 3) {
                        // "Look there": the player turned to where it looked
                        float y = Float.parseFloat(f[1]);
                        float x = Float.parseFloat(f[2]);
                        p.setYRot(y);
                        p.setXRot(x);
                        p.yRotO = y;
                        p.xRotO = x;
                    }
                } catch (NumberFormatException e) {
                    view = -1;
                }
            }
        }
        if (view < 0) {
            end(mc);
            return;
        }
        if (kept == null) {
            kept = mc.options.getCameraType();
        }
        CameraType want = switch (view) {
            case 1 -> CameraType.THIRD_PERSON_BACK;
            case 2 -> CameraType.THIRD_PERSON_FRONT;
            case 3 -> CameraType.FIRST_PERSON;
            default -> kept;
        };
        if (mc.options.getCameraType() != want) {
            mc.options.setCameraType(want);
        }
        on = true;
    }

    private static void end(Minecraft mc) {
        on = false;
        height = 0.0F;
        if (kept != null) {
            mc.options.setCameraType(kept);
            kept = null;
        }
    }

    public static boolean on() {
        return on;
    }

    /** The camera's turn, where it looks (the player's own when it does not). */
    public static float yaw(float own) {
        return on ? yaw : own;
    }

    public static float pitch(float own) {
        return on ? pitch : own;
    }

    /** How far behind the camera goes (the game's own when it does not; its own scaled as the game's is). */
    public static float distance(float own) {
        return on ? own * dist / 4.0F : own;
    }

    /** Read the same eight visual-block rays as the game's camera, then apply native's clearance. */
    public static Vec3 raised(Level level, Entity entity, Vec3 eye, Vec3 position) {
        if (!on || level == null || entity == null || eye == null || Math.abs(height) < 0.0001F) {
            return position;
        }
        Vec3 wanted = position.add(0.0, height, 0.0);
        Vec3 travel = wanted.subtract(eye);
        double length = travel.length();
        if (!Double.isFinite(length) || length < 0.0001) {
            return position;
        }
        StringBuilder facts = new StringBuilder("fl\tclip\t").append(length);
        for (int i = 0; i < 8; i++) {
            Vec3 start = eye.add(((i & 1) * 2 - 1) * 0.1, (((i >> 1) & 1) * 2 - 1) * 0.1, (((i >> 2) & 1) * 2 - 1) * 0.1);
            HitResult hit = level.clip(new ClipContext(start, start.add(travel), ClipContext.Block.VISUAL, ClipContext.Fluid.NONE, entity));
            facts.append('\t').append(hit.getType() == HitResult.Type.MISS ? -1.0 : hit.getLocation().distanceTo(start));
        }
        try {
            float fraction = Float.parseFloat(Native.guiCall(facts.toString()));
            if (Float.isFinite(fraction) && fraction >= 0.0F && fraction <= 1.0F) {
                return eye.add(travel.scale(fraction));
            }
        } catch (NumberFormatException | NullPointerException ignored) {
            // Keep the game's already clipped position if the native response is unavailable.
        }
        return position;
    }

    /** The mouse turned: the camera's while it looks (true), else the player's. */
    public static boolean turn(double dx, double dy) {
        return on && "1".equals(Native.guiCall("fl\tturn\t" + dx + "\t" + dy));
    }

    /** The wheel turned: nearer or farther while it looks (true), else the game's. */
    public static boolean scroll(double dy) {
        return on && dy != 0.0 && "1".equals(Native.guiCall("fl\tscroll\t" + dy));
    }
}
