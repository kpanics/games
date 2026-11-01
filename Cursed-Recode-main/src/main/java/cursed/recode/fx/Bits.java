package cursed.recode.fx;

import java.util.Random;
import net.minecraft.client.multiplayer.ClientLevel;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.world.phys.shapes.VoxelShape;

/**
 * Small things flying in the world for the effects (Jump Circle's dust, Trails' sparks and smoke): each where it is,
 * how fast, how old, how long it lives, how big, its colour, its form (GRAIN of matter, EMBER of light, PUFF of dust
 * or smoke swelling and thinning), how hard it falls (below nothing: it rises) and how the air holds it back; bouncing
 * on the blocks it falls on.
 */
final class Bits {
    static final int GRAIN = 0;
    static final int EMBER = 3;
    static final int PUFF = 4;
    private static final Random RANDOM = new Random();
    private final int most;
    private final double[] p;
    private final double[] v;
    private final float[] age;
    private final float[] life;
    private final float[] size;
    private final float[] fall;
    private final float[] drag;
    private final int[] rgb;
    private final int[] form;
    private int n;

    Bits(int most) {
        this.most = most;
        this.p = new double[most * 3];
        this.v = new double[most * 3];
        this.age = new float[most];
        this.life = new float[most];
        this.size = new float[most];
        this.fall = new float[most];
        this.drag = new float[most];
        this.rgb = new int[most];
        this.form = new int[most];
    }

    int count() {
        return this.n;
    }

    void clear() {
        this.n = 0;
    }

    /** A new one (when full, one of the others gives way). */
    void add(double x, double y, double z, double vx, double vy, double vz, float life, float size, int rgb, int form, float fall, float drag) {
        int i = this.n < this.most ? this.n++ : RANDOM.nextInt(this.most);
        this.p[i * 3] = x;
        this.p[i * 3 + 1] = y;
        this.p[i * 3 + 2] = z;
        this.v[i * 3] = vx;
        this.v[i * 3 + 1] = vy;
        this.v[i * 3 + 2] = vz;
        this.age[i] = 0.0F;
        this.life[i] = life;
        this.size[i] = size;
        this.rgb[i] = rgb;
        this.form[i] = form;
        this.fall[i] = fall;
        this.drag[i] = drag;
    }

    /** One frame: they fly, fall, bounce, fade, and are drawn. */
    void step(ClientLevel level, float dt, float strength) {
        for (int i = 0; i < this.n; i++) {
            this.age[i] += dt;
            if (this.age[i] >= this.life[i]) {
                this.n--;
                System.arraycopy(this.p, this.n * 3, this.p, i * 3, 3);
                System.arraycopy(this.v, this.n * 3, this.v, i * 3, 3);
                this.age[i] = this.age[this.n];
                this.life[i] = this.life[this.n];
                this.size[i] = this.size[this.n];
                this.rgb[i] = this.rgb[this.n];
                this.form[i] = this.form[this.n];
                this.fall[i] = this.fall[this.n];
                this.drag[i] = this.drag[this.n];
                i--;
                continue;
            }
            double d = Math.exp(-dt * this.drag[i]);
            this.v[i * 3] *= d;
            this.v[i * 3 + 1] = this.v[i * 3 + 1] * d - this.fall[i] * dt;
            this.v[i * 3 + 2] *= d;
            double x = this.p[i * 3] + this.v[i * 3] * dt;
            double y = this.p[i * 3 + 1] + this.v[i * 3 + 1] * dt;
            double z = this.p[i * 3 + 2] + this.v[i * 3 + 2] * dt;
            double top = top(level, x, y, z);
            if (y < top && this.v[i * 3 + 1] < 0.0) {
                y = top;
                this.v[i * 3 + 1] = -this.v[i * 3 + 1] * 0.32;
                this.v[i * 3] *= 0.55;
                this.v[i * 3 + 2] *= 0.55;
            }
            this.p[i * 3] = x;
            this.p[i * 3 + 1] = y;
            this.p[i * 3 + 2] = z;
            float q = this.age[i] / this.life[i];
            int f = this.form[i];
            float k = f == EMBER ? (float) Math.pow(1.0F - q, 0.8) * 1.3F
                    : f == PUFF ? 0.8F * (float) Math.pow(1.0F - q, 1.6) * Math.min(1.0F, q * 8.0F) : (float) Math.pow(1.0F - q, 1.3) * 0.9F;
            float sz = this.size[i] * (f == EMBER ? 1.0F - 0.4F * q : f == PUFF ? 1.0F + 1.4F * q : 1.0F);
            Fx.sprite(x, y + sz * 0.5, z, sz, f, this.age[i] * 3.0F + i, this.rgb[i], k * strength, f == EMBER ? 0 : Fx.SOLID);
        }
    }

    /** The top of what is solid in the block at a point (nothing: far below). */
    private static double top(ClientLevel level, double x, double y, double z) {
        BlockPos b = BlockPos.containing(x, y, z);
        VoxelShape s = level.getBlockState(b).getCollisionShape(level, b);
        return s.isEmpty() ? Double.NEGATIVE_INFINITY : b.getY() + s.max(Direction.Axis.Y);
    }

    static int mix(int a, int b, double t) {
        t = Math.clamp(t, 0.0, 1.0);
        int r = (int) Math.round(((a >> 16) & 255) + (((b >> 16) & 255) - ((a >> 16) & 255)) * t);
        int g = (int) Math.round(((a >> 8) & 255) + (((b >> 8) & 255) - ((a >> 8) & 255)) * t);
        int bl = (int) Math.round((a & 255) + ((b & 255) - (a & 255)) * t);
        return r << 16 | g << 8 | bl;
    }
}
