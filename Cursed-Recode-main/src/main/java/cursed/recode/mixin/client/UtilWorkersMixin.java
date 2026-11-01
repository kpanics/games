package cursed.recode.mixin.client;

import cursed.recode.dev.Dev;
import java.util.concurrent.ForkJoinPool;
import java.util.concurrent.ForkJoinWorkerThread;
import net.minecraft.util.Util;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Unique;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.ModifyArg;

/**
 * The game's background workers (Worker-Main: the sections' meshes, the world's generation and lighting) run below the
 * threads that put frames on the screen. There is one per core but one, so when new chunks come in fast they take every
 * core, and at the same priority the render thread, the native frame thread and the driver's own wait their turn for
 * tens of milliseconds: the game froze while its frame rate stayed high. Below them the workers still get every core
 * the frame does not need. CURSED_DEV_WORKERS_NORMAL keeps the game's priority, for comparing.
 */
@Mixin(Util.class)
public abstract class UtilWorkersMixin {
    @Unique
    private static final boolean NORMAL = Dev.on("CURSED_DEV_WORKERS_NORMAL");

    @ModifyArg(method = "makeExecutor", at = @At(value = "INVOKE",
            target = "Ljava/util/concurrent/ForkJoinPool;<init>(ILjava/util/concurrent/ForkJoinPool$ForkJoinWorkerThreadFactory;Ljava/lang/Thread$UncaughtExceptionHandler;Z)V"),
            index = 1)
    private static ForkJoinPool.ForkJoinWorkerThreadFactory cursed$lowerWorkers(ForkJoinPool.ForkJoinWorkerThreadFactory factory) {
        if (NORMAL) {
            return factory;
        }
        return pool -> {
            ForkJoinWorkerThread t = factory.newThread(pool);
            t.setPriority(Thread.NORM_PRIORITY - 2);
            return t;
        };
    }
}
