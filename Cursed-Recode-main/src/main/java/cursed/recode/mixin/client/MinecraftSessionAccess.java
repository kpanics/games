package cursed.recode.mixin.client;

import com.mojang.authlib.minecraft.UserApiService;
import com.mojang.authlib.services.ProfileResult;
import java.util.concurrent.CompletableFuture;
import net.minecraft.client.Minecraft;
import net.minecraft.client.User;
import net.minecraft.client.multiplayer.ProfileKeyPairManager;
import net.minecraft.client.multiplayer.chat.report.ReportingContext;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Mutable;
import org.spongepowered.asm.mixin.gen.Accessor;

/**
 * The game's session, changed by Alt Manager (cursed.recode.alts.Alts): who plays, their profile asked anew, and what
 * the game keeps by the account — its services (the flags chat and servers go by, reports) and the key chat is signed
 * with.
 */
@Mixin(Minecraft.class)
public interface MinecraftSessionAccess {
    @Mutable
    @Accessor("user")
    void cursed$setUser(User user);

    @Mutable
    @Accessor("profileFuture")
    void cursed$setProfileFuture(CompletableFuture<ProfileResult> future);

    @Mutable
    @Accessor("userApiService")
    void cursed$setUserApiService(UserApiService service);

    @Mutable
    @Accessor("userPropertiesFuture")
    void cursed$setUserPropertiesFuture(CompletableFuture<UserApiService.UserProperties> future);

    @Mutable
    @Accessor("profileKeyPairManager")
    void cursed$setProfileKeyPairManager(ProfileKeyPairManager manager);

    @Accessor("reportingContext")
    void cursed$setReportingContext(ReportingContext context);
}
