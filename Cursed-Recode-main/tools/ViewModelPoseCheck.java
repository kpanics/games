import com.mojang.blaze3d.vertex.PoseStack;
import cursed.recode.fx.ViewModel;
import org.joml.Matrix4f;
import org.joml.Vector3f;
import java.lang.reflect.Method;

/** Pure pose/bridge check: no Minecraft instance, window, native library or network is started. */
public final class ViewModelPoseCheck {
    static void set(String name, Object value) throws Exception {
        var f = ViewModel.class.getDeclaredField(name);
        f.setAccessible(true);
        f.set(null, value);
    }
    static void equal(Matrix4f got, Matrix4f want) {
        float[] a = got.get(new float[16]), b = want.get(new float[16]);
        for (int i = 0; i < 16; i++) if (Math.abs(a[i] - b[i]) > 0.000002F)
            throw new AssertionError("Matrix element " + i + ": " + a[i] + " != " + b[i]);
    }
    public static void main(String[] args) throws Exception {
        Method parse = ViewModel.class.getDeclaredMethod("operations", String.class);
        parse.setAccessible(true);
        if (parse.invoke(null, "r 1 0 0 NaN") != null || parse.invoke(null, "t 1 2") != null)
            throw new AssertionError("Malformed transforms accepted");
        int checks = 2;
        for (int source = 1; source <= 3; source++) for (float direction : new float[]{-1, 1}) {
            set("classicSource", source); set("classicApplied", false); set("swing", null); set("edit", null);
            set("direction", direction); set("equip", 0.4F); set("equipFactor", source == 2 ? 0.25F : Float.NaN);
            set("pose", new float[]{0.13F, -0.2F, 0.05F, 0, 0, 0, 1});
            String text = "t 0.2 0.1 -0.3;r 0 1 0 45;r 1 0 -1 27;r 0 1 0 -45";
            set("operations", parse.invoke(null, text));
            PoseStack pose = new PoseStack();
            ViewModel.beforeBase(pose);
            pose.translate(direction * 0.56F, -0.52F - 0.4F * 0.6F, -0.72F);
            ViewModel.pose(pose);
            Matrix4f expected = new Matrix4f().translate(0.13F, -0.2F, 0.05F);
            if (source != 1) expected.translate(direction * 0.56F, -0.52F - 0.4F * (source == 2 ? 0.25F : 0.6F), -0.72F);
            Vector3f axis = new Vector3f(1, 0, -1).normalize();
            expected.translate(0.2F, 0.1F, -0.3F).rotateY((float)Math.toRadians(45))
                    .rotate((float)Math.toRadians(27), axis.x, axis.y, axis.z).rotateY((float)Math.toRadians(-45));
            equal(pose.last().pose(), expected);
            if (!ViewModel.swing(pose) || !ViewModel.classicApplied()) throw new AssertionError("Vanilla swing was not suppressed");
            equal(pose.last().pose(), expected);
            checks += 3;
        }
        set("classicSource", 2); set("classicApplied", false); set("pose", null); set("equipFactor", Float.NaN);
        set("operations", parse.invoke(null, ""));
        PoseStack idle = new PoseStack(); ViewModel.pose(idle);
        if (!ViewModel.swing(idle)) throw new AssertionError("Idle source pose fell back to vanilla");
        equal(idle.last().pose(), new Matrix4f()); checks += 2;
        set("classicSource", 0); set("classicApplied", false); set("operations", null);
        if (ViewModel.swing(new PoseStack())) throw new AssertionError("Vanilla was suppressed without a custom pose");
        System.out.println("Pose bridge: " + (checks + 1) + " checks passed");
    }
}
