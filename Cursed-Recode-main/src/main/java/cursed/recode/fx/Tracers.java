package cursed.recode.fx;

import cursed.recode.render.Native;

/** A CS rifle's tracers (native/src/cosmetics/tracer.rs): this frame's streaks, drawn as light. */
final class Tracers {
    private Tracers() {
    }

    static void draw() {
        String s = Native.guiCall("cosm\ttracers");
        if (s == null || s.isEmpty()) {
            return;
        }
        for (String one : s.split(";")) {
            String[] f = one.split(" ");
            if (f.length < 7) {
                continue;
            }
            try {
                float k = Float.parseFloat(f[6]);
                Fx.seg(Double.parseDouble(f[0]), Double.parseDouble(f[1]), Double.parseDouble(f[2]), Double.parseDouble(f[3]), Double.parseDouble(f[4]), Double.parseDouble(f[5]),
                        0.018F, 1.0F, 1.6F, 0xFFE3A6, k, Fx.THIN);
            } catch (NumberFormatException ignored) {
                // a streak cut short: the next frame's
            }
        }
    }
}
