package cursed.recode.mixin.client;

import net.minecraft.client.gui.Hud;
import net.minecraft.network.chat.Component;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Accessor;

/** The title and the action bar as the game holds them: Other Custom Things draws them (cursed.recode.hud.HudOther). */
@Mixin(Hud.class)
public interface HudAccess {
    @Accessor("title")
    Component cursed$title();

    @Accessor("subtitle")
    Component cursed$subtitle();

    @Accessor("titleTime")
    int cursed$titleTime();

    @Accessor("titleFadeInTime")
    int cursed$fadeIn();

    @Accessor("titleStayTime")
    int cursed$stay();

    @Accessor("titleFadeOutTime")
    int cursed$fadeOut();

    @Accessor("overlayMessageString")
    Component cursed$action();

    @Accessor("overlayMessageTime")
    int cursed$actionTime();

    @Accessor("animateOverlayMessageColor")
    boolean cursed$actionRainbow();

    /** A texture over the whole view (No Render lets through those it does not leave out: HudMixin). */
    @org.spongepowered.asm.mixin.gen.Invoker("extractTextureOverlay")
    void cursed$textureOverlay(net.minecraft.client.gui.GuiGraphicsExtractor graphics, net.minecraft.resources.Identifier texture, float alpha);
}
