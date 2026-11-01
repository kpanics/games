package cursed.recode.render;

import com.mojang.logging.LogUtils;
import com.mojang.renderpearl.api.pipeline.ShaderSource;
import com.mojang.renderpearl.api.pipeline.ShaderType;
import com.mojang.renderpearl.backend.api.SpvModule;
import com.mojang.renderpearl.frontend.shaders.SPIRVModule;
import cursed.recode.dev.Dev;
import java.nio.ByteBuffer;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;
import java.security.MessageDigest;
import java.util.HashSet;
import java.util.HexFormat;
import java.util.Map;
import java.util.Set;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.regex.Matcher;
import java.util.regex.Pattern;
import net.fabricmc.loader.api.FabricLoader;
import net.minecraft.SharedConstants;
import net.minecraft.client.renderer.ShaderDefines;
import net.minecraft.resources.Identifier;
import org.jspecify.annotations.Nullable;
import org.lwjgl.system.MemoryUtil;
import org.lwjgl.util.shaderc.ShadercIncludeResult;
import org.slf4j.Logger;

/**
 * The SPIR-V the game compiles from its GLSL (GlslCompiler.compileToSpv, shaderc), kept on disk: the same input gives
 * the same bytes, so a shader seen before is read instead of compiled — most of the "Shader Loader" part of every
 * resource reload. The key is a SHA-256 of everything the result depends on: the source and, recursively, every file
 * it includes (their text), the defines in order, the compiler's switches, the game and LWJGL versions.
 * CURSED_DEV_NOSHADERCACHE turns it off.
 */
public final class ShaderCache {
    private static final Logger LOGGER = LogUtils.getLogger();
    private static final boolean OFF = Dev.on("CURSED_DEV_NOSHADERCACHE");
    private static final String VERSION = "cursed shader cache 1";
    private static final Pattern INCLUDE = Pattern.compile("#\\s*include\\s*[<\"]([^>\"]+)[>\"]");
    private static final ThreadLocal<String> PENDING = new ThreadLocal<>();
    private static final AtomicInteger hits = new AtomicInteger();
    private static final AtomicInteger misses = new AtomicInteger();
    private static volatile @Nullable Path dir;

    private ShaderCache() {
    }

    private static Path dir() {
        Path d = dir;
        if (d == null) {
            d = FabricLoader.getInstance().getGameDir().resolve("cursed-recode").resolve("shaders");
            dir = d;
        }
        return d;
    }

    /** Before compiling: the cached module, or null (and the key is kept for `store`). */
    public static @Nullable SpvModule load(String name, String source, ShaderType type, ShaderDefines defines, ShaderSource shaderSource, String switches) {
        PENDING.remove();
        if (OFF) {
            return null;
        }
        String key;
        try {
            key = key(name, source, type, defines, shaderSource, switches);
        } catch (Exception e) {
            return null;
        }
        Path file = dir().resolve(key + ".spv");
        if (Files.isRegularFile(file)) {
            try {
                byte[] bytes = Files.readAllBytes(file);
                if (bytes.length > 0 && bytes.length % 4 == 0) {
                    ByteBuffer spv = MemoryUtil.memCalloc(bytes.length);
                    spv.put(bytes).flip();
                    hits.incrementAndGet();
                    return new SPIRVModule(spv, type);
                }
            } catch (Exception e) {
                LOGGER.warn("[shader cache] cannot read {}", file, e);
            }
        }
        misses.incrementAndGet();
        PENDING.set(key);
        return null;
    }

    /** After compiling: the module's bytes under the key `load` made. */
    public static void store(@Nullable SpvModule module) {
        String key = PENDING.get();
        PENDING.remove();
        if (key == null || module == null) {
            return;
        }
        try {
            ByteBuffer spv = module.spv();
            byte[] bytes = new byte[spv.remaining()];
            spv.duplicate().get(bytes);
            Path d = dir();
            Files.createDirectories(d);
            Path tmp = Files.createTempFile(d, key, ".tmp");
            Files.write(tmp, bytes);
            Path target = d.resolve(key + ".spv");
            try {
                Files.move(tmp, target, StandardCopyOption.REPLACE_EXISTING, StandardCopyOption.ATOMIC_MOVE);
            } catch (java.io.IOException raced) {
                // Two threads compiled the same shader at once: the other one's file is the same bytes.
                Files.deleteIfExists(tmp);
                if (!Files.isRegularFile(target)) {
                    throw raced;
                }
            }
        } catch (Exception e) {
            LOGGER.warn("[shader cache] cannot write {}", key, e);
        }
    }

    public static String stats() {
        return hits.get() + " from the cache, " + misses.get() + " compiled; " + CursedDevice.pipelineStats();
    }

    private static String key(String name, String source, ShaderType type, ShaderDefines defines, ShaderSource shaderSource, String switches) throws Exception {
        MessageDigest d = MessageDigest.getInstance("SHA-256");
        put(d, VERSION);
        put(d, SharedConstants.getCurrentVersion().name());
        put(d, org.lwjgl.Version.getVersion());
        put(d, switches);
        put(d, name);
        put(d, type.name());
        for (Map.Entry<String, String> e : defines.values().entrySet()) {
            put(d, "D " + e.getKey() + "=" + e.getValue());
        }
        for (String f : defines.flags()) {
            put(d, "F " + f);
        }
        put(d, source);
        includes(d, source, shaderSource, new HashSet<>());
        return HexFormat.of().formatHex(d.digest());
    }

    /** Every file the text includes, depth first, each once: its name and its text (or that it is missing). */
    private static void includes(MessageDigest d, String text, ShaderSource shaderSource, Set<String> seen) {
        Matcher m = INCLUDE.matcher(text);
        while (m.find()) {
            String id = m.group(1);
            if (!seen.add(id)) {
                continue;
            }
            put(d, "I " + id);
            Identifier identifier = Identifier.tryParse(id);
            ShaderSource.CachedIncludeSource include = identifier == null ? null : shaderSource.getInclude(identifier);
            if (include == null) {
                put(d, "missing");
                continue;
            }
            ShadercIncludeResult r = ShadercIncludeResult.create(include.includeResultPtr());
            // by its length: the game's text has no zero after it (content() would read on to one, past the buffer's
            // end when it lies at a page's — the game falls with an access violation reloading its resources)
            long at = MemoryUtil.memGetAddress(r.address() + ShadercIncludeResult.CONTENT);
            String content = at == 0L ? "" : MemoryUtil.memUTF8(at, (int) r.content_length());
            put(d, content);
            includes(d, content, shaderSource, seen);
        }
    }

    private static void put(MessageDigest d, String s) {
        byte[] b = s.getBytes(StandardCharsets.UTF_8);
        d.update((byte) (b.length >>> 24));
        d.update((byte) (b.length >>> 16));
        d.update((byte) (b.length >>> 8));
        d.update((byte) b.length);
        d.update(b);
    }
}
