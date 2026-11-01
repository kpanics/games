package cursed.recode.mixin.client;

import java.util.List;
import net.minecraft.client.gui.components.ChatComponent;
import net.minecraft.client.multiplayer.chat.GuiMessage;
import net.minecraft.client.multiplayer.chat.GuiMessageSource;
import net.minecraft.client.multiplayer.chat.GuiMessageTag;
import net.minecraft.network.chat.Component;
import net.minecraft.network.chat.MessageSignature;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Accessor;
import org.spongepowered.asm.mixin.gen.Invoker;

/** The chat's lines and messages, where it is scrolled: the Custom Chat lays them out (cursed.recode.hud.HudChat). */
@Mixin(ChatComponent.class)
public interface ChatComponentAccess {
    @Accessor("trimmedMessages")
    List<GuiMessage.Line> cursed$lines();

    @Accessor("allMessages")
    List<GuiMessage> cursed$messages();

    @Accessor("chatScrollbarPos")
    int cursed$scroll();

    @Accessor("newMessageSinceScroll")
    boolean cursed$newSince();

    @Invoker("addMessage")
    void cursed$add(Component contents, MessageSignature signature, GuiMessageSource source, GuiMessageTag tag);
}
