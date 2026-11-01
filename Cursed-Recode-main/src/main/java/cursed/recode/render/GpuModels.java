package cursed.recode.render;

import com.mojang.logging.LogUtils;
import com.mojang.renderpearl.api.buffers.GpuBuffer;
import cursed.recode.dev.Dev;
import cursed.recode.scene.ModelJobs;
import org.slf4j.Logger;

/**
 * Entity models' vertices made by the GPU (native/src/vk/models.rs; its shader made by GpuShaders). Until it is made —
 * or when it cannot be — models keep the CPU path (NativeModels).
 * CURSED_DEV_NOGPUMODELS keeps the CPU path; CURSED_DEV_GPUMODELCHECK compares every part the GPU makes with it.
 */
public final class GpuModels {
    private static final Logger LOGGER = LogUtils.getLogger();
    private static final boolean OFF = Dev.on("CURSED_DEV_NOGPUMODELS");
    static final boolean CHECK = Dev.on("CURSED_DEV_GPUMODELCHECK");

    private GpuModels() {
    }

    /** Whether the GPU makes models' vertices (render thread). */
    public static boolean on() {
        return !OFF && GpuShaders.ready();
    }

    /** After the game's copy into `target`: the jobs fill their vertices in it. */
    public static void dispatch(ModelJobs jobs, GpuBuffer target) {
        if (CHECK) {
            int unplaced = jobs.unplaced();
            if (unplaced > 0) {
                LOGGER.warn("[models gpu] {} of {} jobs got no place in the vertex buffer", unplaced, jobs.count);
            }
        }
        CursedDevice.current.encoder.models(jobs.address(), jobs.count, CursedBuffer.handle(target));
    }
}
