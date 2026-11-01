package cursed.recode.mixin.client;

import cursed.recode.hud.HudLayer;
import net.minecraft.client.Minecraft;
import net.minecraft.client.multiplayer.MultiPlayerGameMode;
import net.minecraft.client.player.LocalPlayer;
import net.minecraft.core.BlockPos;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.InteractionResult;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.phys.BlockHitResult;
import org.jspecify.annotations.Nullable;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Unique;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** The player hits something: the HUD's Target element shows it (a friend is not hit at all: No Friend Damage). A
 *  block broken or placed: Block Outline's flash. */
@Mixin(MultiPlayerGameMode.class)
public abstract class MultiPlayerGameModeMixin {
    /** What the block being broken was, until it is gone. */
    @Unique
    private @Nullable BlockState cursed$was;

    @Inject(method = "attack", at = @At("HEAD"), cancellable = true)
    private void cursed$target(Player player, Entity entity, CallbackInfo ci) {
        // a double of Fake Player's: the blow is the client's own (never sent)
        if (cursed.recode.fx.FakePlayers.number(entity) >= 0) {
            cursed.recode.fx.FakePlayers.hit(entity);
            ci.cancel();
            return;
        }
        if (cursed.recode.play.Play.spared(player, entity)) {
            ci.cancel();
            return;
        }
        HudLayer.attacked(entity);
    }

    @Inject(method = "destroyBlock", at = @At("HEAD"))
    private void cursed$breaking(BlockPos pos, CallbackInfoReturnable<Boolean> cir) {
        var level = Minecraft.getInstance().level;
        this.cursed$was = level != null ? level.getBlockState(pos) : null;
    }

    @Inject(method = "destroyBlock", at = @At("RETURN"))
    private void cursed$broken(BlockPos pos, CallbackInfoReturnable<Boolean> cir) {
        BlockState was = this.cursed$was;
        this.cursed$was = null;
        if (was != null && cir.getReturnValueZ()) {
            cursed.recode.fx.BlockOutline.broken(pos, was);
        }
    }

    @Inject(method = "useItemOn", at = @At("HEAD"))
    private void cursed$placing(LocalPlayer player, InteractionHand hand, BlockHitResult hit, CallbackInfoReturnable<InteractionResult> cir) {
        cursed.recode.fx.BlockOutline.using(hit, player.getItemInHand(hand));
        // (Shulker Preview: the box opened by this click is remembered as it is seen open)
        cursed.recode.fx.ShulkerPreview.used(hit.getBlockPos());
    }
}
