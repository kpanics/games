package cursed.recode.chams;

import com.mojang.blaze3d.vertex.PoseStack;
import net.minecraft.client.renderer.SubmitNodeCollector;
import net.minecraft.client.renderer.entity.EntityRenderDispatcher;
import net.minecraft.client.renderer.entity.state.AvatarRenderState;
import net.minecraft.client.renderer.entity.state.EntityRenderState;
import net.minecraft.client.renderer.entity.state.LivingEntityRenderState;
import net.minecraft.client.renderer.state.level.CameraRenderState;

/**
 * Where the game's frame meets Chams (the mixins call these): a frame of bodies begins, a body is submitted (a player,
 * one's own seen from outside, a mob), the first person's hands are.
 */
public final class ChamsHooks {
    /** The body being submitted is in the look (Chams.wants). */
    private static boolean bodyIn;
    /**
     * A creature drawn into the screen (the HUD's target, the inventory's player, the menus' cells: GuiEntityRenderer,
     * ItemStage): shown as it is, never in a look — its boxes are not the world's, the engine would draw its smoke and
     * its liquid in the sky.
     */
    private static int gui;

    private ChamsHooks() {
    }

    public static void frameStart() {
        Chams.frame();
        gui = 0;
    }

    public static void guiStart() {
        gui++;
    }

    public static void guiDone() {
        gui = Math.max(0, gui - 1);
    }

    /** Whether what is submitted now is drawn into the screen, not the world. */
    public static boolean inGui() {
        return gui > 0;
    }

    public static void bodyStart(EntityRenderState body) {
        bodyIn = Chams.wants(body);
        Chams.Settings o = Chams.settings;
        int kind = bodyIn ? o.bodyKind() : 0;
        Chams.begin(kind);
        if (kind == Chams.LIQUID || kind == Chams.CURSED) {
            net.minecraft.client.Minecraft mc = net.minecraft.client.Minecraft.getInstance();
            ChamsLiquid.bodyStart(mc.player != null && body instanceof AvatarRenderState a && a.id == mc.player.getId());
        }
        if (kind != 0 && o.parts[Chams.BODY] && !o.outerLayer && body instanceof AvatarRenderState a) {
            // the look is one matter: the skin's outer layer (hat, jacket, sleeves, trousers) would stand out of it in
            // squares, its pixels each another piece of it
            a.showHat = false;
            a.showJacket = false;
            a.showLeftSleeve = false;
            a.showRightSleeve = false;
            a.showLeftPants = false;
            a.showRightPants = false;
        }
    }

    /** Whether the first person's arm shows its sleeve (the skin's outer layer): not in the look without it. */
    public static boolean sleeve(boolean visible) {
        Chams.Settings o = Chams.settings;
        return visible && !(Chams.kind() != 0 && o.parts[Chams.BODY] && !o.outerLayer);
    }

    public static void bodyDone(EntityRenderDispatcher dispatcher, EntityRenderState body, CameraRenderState camera, PoseStack poseStack, SubmitNodeCollector out) {
        boolean in = bodyIn;
        bodyIn = false;
        Chams.end();
        Chams.Settings o = Chams.settings;
        if (in && o.engineBodies() && body instanceof LivingEntityRenderState living) {
            ChamsInk.body(living, camera);
        }
    }

    public static void handsStart() {
        Chams.Settings o = Chams.settings();
        if (o.on && o.hands && o.bodyKind() != 0) {
            Chams.begin(o.bodyKind());
            Chams.inHands = true;
        }
    }

    public static void handsDone() {
        Chams.inHands = false;
        Chams.end();
    }
}
