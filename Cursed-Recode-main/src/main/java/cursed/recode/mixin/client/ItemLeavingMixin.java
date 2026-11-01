package cursed.recode.mixin.client;

import cursed.recode.fx.ItemPhysic;
import it.unimi.dsi.fastutil.ints.IntArrayList;
import it.unimi.dsi.fastutil.ints.IntList;
import net.minecraft.client.multiplayer.ClientLevel;
import net.minecraft.client.multiplayer.ClientPacketListener;
import net.minecraft.network.protocol.game.ClientboundRemoveEntitiesPacket;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Redirect;

/** Item Physic: a dropped item the server takes away (burnt up, despawned, merged into another, into a hopper) is
 *  kept a moment to be seen going (ItemPhysic.leaving; it goes on its own after). The packet itself is left as it is
 *  (on a world opened to LAN the same one goes to the others). */
@Mixin(ClientPacketListener.class)
public abstract class ItemLeavingMixin {
    @Shadow
    private ClientLevel level;

    @Redirect(method = "handleRemoveEntities",
            at = @At(value = "INVOKE", target = "Lnet/minecraft/network/protocol/game/ClientboundRemoveEntitiesPacket;entityIds()Lit/unimi/dsi/fastutil/ints/IntList;"))
    private IntList cursed$leaving(ClientboundRemoveEntitiesPacket packet) {
        IntList ids = packet.entityIds();
        if (this.level == null || !ItemPhysic.any(this.level, ids)) {
            return ids;
        }
        IntList rest = new IntArrayList(ids.size());
        for (int i = 0; i < ids.size(); i++) {
            int id = ids.getInt(i);
            if (!ItemPhysic.leaving(this.level, id)) {
                rest.add(id);
            }
        }
        return rest;
    }
}
