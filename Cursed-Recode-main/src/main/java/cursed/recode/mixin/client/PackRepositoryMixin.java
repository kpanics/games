package cursed.recode.mixin.client;

import com.google.common.collect.ImmutableSet;
import cursed.recode.OwnPack;
import cursed.recode.UserPack;
import java.util.Arrays;
import java.util.Collection;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Set;
import net.minecraft.client.resources.ClientPackSource;
import net.minecraft.server.packs.repository.Pack;
import net.minecraft.server.packs.repository.PackRepository;
import net.minecraft.server.packs.repository.RepositorySource;
import org.spongepowered.asm.mixin.Final;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Mutable;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** The client's resources as a pack of the game's (cursed.recode.OwnPack): only in the client's own resources (the
 *  repository made with the game's ClientPackSource; not a world's data packs), just over the game's own. */
@Mixin(PackRepository.class)
public abstract class PackRepositoryMixin {
    @Shadow
    @Final
    @Mutable
    private Set<RepositorySource> sources;

    @Inject(method = "<init>", at = @At("RETURN"))
    private void cursed$own(RepositorySource[] given, CallbackInfo ci) {
        if (OwnPack.wanted() && Arrays.stream(given).anyMatch(s -> s instanceof ClientPackSource)) {
            Set<RepositorySource> all = new LinkedHashSet<>(this.sources);
            all.add(OwnPack::load);
            // the player's own (the swords' models): always in the repository — the folder may appear at any time
            all.add(UserPack::load);
            this.sources = ImmutableSet.copyOf(all);
        }
    }

    @Inject(method = "rebuildSelected", at = @At("RETURN"), cancellable = true)
    private void cursed$placed(Collection<String> names, CallbackInfoReturnable<List<Pack>> cir) {
        List<Pack> placed = UserPack.placed(OwnPack.placed(cir.getReturnValue()));
        if (placed != cir.getReturnValue()) {
            cir.setReturnValue(placed);
        }
    }
}
