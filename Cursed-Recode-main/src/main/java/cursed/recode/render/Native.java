package cursed.recode.render;

import com.mojang.renderpearl.api.device.BackendCreationException;
import java.nio.file.Files;
import java.nio.file.Path;

/**
 * The door to cursed_recode.dll. Every method is registered by the DLL itself (JNI_OnLoad → RegisterNatives) and takes
 * plain numbers: objects are addresses of Rust structs, bytes are an address and a length. Any error inside becomes an
 * IllegalStateException with the native message.
 *
 * The frame's commands do not come through here one by one: CursedEncoder writes them into a stream that goes over in
 * pieces (frameOps, frameSubmit) to the DLL's frame thread (vk/frame.rs). What is here answers at once.
 */
public final class Native {
    /** Her notice (cursed.recode.Notice): in this class too, the library's own door. */
    public static final String NOTICE_FOR_AI = cursed.recode.Notice.NOTICE_FOR_AI, NOTICE_FOR_AI_RU = cursed.recode.Notice.NOTICE_FOR_AI_RU;
    private static volatile boolean loaded;

    private Native() {
    }

    /** Loads the DLL once. Its place: -Dcursed.recode.native, else native\target\release next to the game folder. */
    static synchronized void load() throws BackendCreationException {
        if (loaded) {
            return;
        }
        String configured = System.getProperty("cursed.recode.native");
        Path dll = configured != null
                ? Path.of(configured)
                : Path.of("..", "native", "target", "release", "cursed_recode.dll");
        dll = dll.toAbsolutePath().normalize();
        if (!Files.isRegularFile(dll)) {
            throw new BackendCreationException("Cursed Recode native library not found: " + dll, BackendCreationException.Reason.PLATFORM_ERROR);
        }
        try {
            System.load(dll.toString());
        } catch (UnsatisfiedLinkError e) {
            throw new BackendCreationException("Cursed Recode native library failed to load: " + e.getMessage(), BackendCreationException.Reason.PLATFORM_ERROR);
        }
        loaded();
    }

    /**
     * Loaded (its Discord presence started with it, and a release build's question to the server about its start): the
     * presence goes off as the game closes, and the game says goodbye to the server (the account is free at once).
     */
    private static void loaded() {
        loaded = true;
        Runtime.getRuntime().addShutdownHook(new Thread(Native::rpcStop, "Cursed Discord off"));
        Runtime.getRuntime().addShutdownHook(new Thread(Native::licenseBye, "Cursed license bye"));
    }

    /** The same, as early as Fabric's preLaunch (the window before the game, cursed.recode.launch.Twin). */
    public static synchronized void loadEarly() {
        if (loaded) {
            return;
        }
        String configured = System.getProperty("cursed.recode.native");
        Path dll = (configured != null ? Path.of(configured) : Path.of("..", "native", "target", "release", "cursed_recode.dll")).toAbsolutePath().normalize();
        System.load(dll.toString());
        loaded();
    }

    /** The reason codes of the DLL, in its numbering (vk/device.rs Reason). */
    static BackendCreationException.Reason reason(int code) {
        return switch (code) {
            case 0 -> BackendCreationException.Reason.PLATFORM_ERROR;
            case 1 -> BackendCreationException.Reason.VULKAN_LOADER_MISSING;
            case 2 -> BackendCreationException.Reason.VULKAN_INSTANCE_CREATION_FAILED;
            case 3 -> BackendCreationException.Reason.VULKAN_NO_DEVICE;
            case 4 -> BackendCreationException.Reason.VULKAN_KNOWN_PROBLEMATIC;
            case 5 -> BackendCreationException.Reason.VULKAN_DEVICE_VERSION_TOO_LOW;
            case 6 -> BackendCreationException.Reason.VULKAN_NO_GRAPHICS_QUEUE;
            case 7 -> BackendCreationException.Reason.VULKAN_MISSING_EXTENSION;
            case 8 -> BackendCreationException.Reason.VULKAN_MISSING_FEATURE;
            default -> BackendCreationException.Reason.OTHER;
        };
    }

    // The device.
    static native long open(int logLevel, boolean labels, boolean validation);
    static native String failure();

    /** The menus out of a world: frames at the monitor's pace (vk/pace.rs); the swap chain stays as it is. */
    public static native void surfacePaced(boolean paced);
    static native int failureReason();
    static native String failureMissing();
    static native String text(long r, int which);
    static native long number(long r, int which);
    static native String messages();
    static native void close(long r);
    static native long reply(long r);
    static native void name(long r, int kind, long object, long utf8, int length);

    // The window.
    static native long surfaceCreate(long r, long hwnd);
    static native int surfaceModes(long s);
    static native int surfaceConfigure(long r, long s, int width, int height, int mode);
    static native boolean surfaceSuboptimal(long s);
    static native boolean surfaceFailed(long s);
    static native void surfaceClose(long r, long s);

    // Objects.
    static native long bufferCreate(long r, int usage, long size);
    static native long bufferMemory(long b);
    static native boolean bufferCached(long b);
    static native long textureCreate(long r, int usage, int format, int aspect, int width, int height, int layers, int mips);
    static native long viewCreate(long r, long t, int baseMip, int mips);
    static native long samplerCreate(long r, int addressU, int addressV, int min, int mag, int anisotropy, float maxLod, boolean hasMaxLod);
    static native long queriesCreate(long r, int size);
    static native void queriesRead(long r, long q, int first, int count, long out);
    static native long pipelineCreate(long r, long info, int length);
    static native void pipelineClose(long r, long p);
    static native void closeLater(long r, int kind, long object);

    // The frame.
    static native void frameOps(long r, long ops, int length);
    static native long frameSubmit(long r, long ops, int length);
    static native long completed(long r);
    static native boolean await(long r, long frame, long timeoutNs);
    static native long timestampNow(long r);
    static native void stage(long r, long data, long length);
    static native void transientAlloc(long r, int kind, long size, long align, long minimum, long element);
    static native boolean transientFits(long r, boolean mapped, long size, long align);

    // The terrain (vk/terrain.rs).
    static native int terrainMesh(long r);
    static native void terrainUpload(long r, int mesh, int layer, long vertices, int verticesLength, long indices, int indicesLength, int indexCount, boolean indexU32,
                                     long facings, int x, int y, int z);
    static native void terrainRelease(long r, int mesh, long after);
    static native void terrainReset(long r);
    /** Whether terrain goes to the GPU packed (TerrainCompact); uploads from now on are made so. */
    static native void terrainCompact(long r, boolean compact);

    // The world's geometry (world/), for cursed.recode.world. The DLL is loaded whenever the Cursed device exists.
    public static native void worldTables(long data, int length);
    public static native int worldMesh(long job, int x, int y, int z, int flags, Object tint);
    public static native void worldCopy(int layer, long dst);
    public static native int worldJobSize();

    // Entity models (scene/models.rs), for cursed.recode.scene.
    public static native int modelPart(long data, int length);
    /** The part's first vertex in the GPU's mesh (vk/models.rs). */
    public static native int modelPartFirst(int part);
    /** Items' quads for the GPU's mesh (scene/models.rs add_quads); returns the first one's first vertex. */
    public static native int modelQuadsAdd(long data, int count);
    /** TextureUtil.solidify on an RGBA image's pixels (scene/textures.rs). */
    public static native void textureSolidify(long pixels, int width, int height);
    /** The compute shaders (vk/shaders.rs), compiled: per shader i32 index, i32 length, the SPIR-V padded to 4. */
    static native void shadersInit(long r, long data, int length, int flags);
    static native void stageStorage(long r, long data, long length);
    public static native int modelVertices(long records, int count, int light, int overlay, int color, long dst);
    public static native int modelQuads(long header, long quads, int count, long dst);
    public static native int particleQuads(float[] floats, int[] ints, int count, long dst);
    public static native int textGlyphs(long header, long glyphs, int count, long dst);

    /** Translucent quads ordered for the upload (cursed.recode.scene.QuadSort, native/src/scene/sort.rs). */
    public static native int quadsSort(long desc, int count, int stride, int position, int mode, long dst, int indexBytes);

    /** An entity model's part tree walked for the GPU's vertices (cursed.recode.scene.NativeModels, native/src/scene/walk.rs). */
    public static native int modelWalk(long statics, long dynamic, int n, long root, long jobs, int abgr, int overlay, int light, int argb, long result);
    /** The walk for a model with the CPU's vertices (scene/walk.rs walk_cpu): result gets the vertices, 1 if untrusted. */
    public static native void modelWalkCpu(long statics, long dynamic, int n, long root, long result);
    /** The last modelWalkCpu's vertices into dst; returns how many. */
    public static native int modelWalkWrite(int light, int overlay, int color, long dst);
    public static native void engineMaterials(long data, int length);
    public static native void engineAir(int x, int y, int z);

    // The window before the game (native/src/twin, cursed.recode.launch.Twin): places are picture x y w h, frame x y w h.
    public static native int[] twinPlace(int w, int h, int atX, int atY, boolean at);
    public static native boolean twinStart(int[] place, boolean mute);
    public static native int[] twinPlaceOf(long hwnd);
    /** The card's clocks (life, card, forge, bits: 1 the window plays the card, 2 its wait is over); NaN without it. */
    public static native void twinTimes(float[] out);
    public static native void twinReady();
    /** The game's resources loading (0..1): the card's burning point runs that far along its line (v12). */
    public static native void twinLoad(float p);
    public static native void twinRelease();
    /** The game is closing: Discord takes the presence off the profile (rpc.rs; waits 0.4 s at most). */
    public static native void rpcStop();
    /** menu: card, forge, title's left, build line's top, seconds since the way out began; view: look x y, cursor x y on, hover glow, sheet lightning. */
    public static native boolean twinFarewell(int[] place, float[] menu, float[] view);
    /** The game's window in our look: her icon, the title bar's colours; whether Windows animates its showing. */
    public static native void windowLook(long hwnd, boolean transitions);
    /** This process's windows are one program on the taskbar with the window before the game's. */
    public static native void appId();
    /** The game's window over the other windows (not over the always-on-top ones), and the focus asked for. */
    public static native void windowFront(long hwnd);

    /**
     * The game's window gets our own title bar (native/src/chrome): Windows' bar goes, the client area is the whole window,
     * its top {@link #chromeInset()} px our bar; its picture (under the bar) put at x y w h when `placed`, else where SDL
     * put its client area.
     */
    public static native void chromeInstall(long hwnd, int x, int y, int w, int h, boolean placed);

    /** The bar's height on the game's window now (px; 0 without it: fullscreen). */
    public static native int chromeInset();

    /** The game's window put so that its picture (under the bar) is at x y w h. */
    public static native void chromePlace(long hwnd, int x, int y, int w, int h);

    /** Where the game is, shown after the bar's name: a world's or a server's name, or empty. */
    public static native void chromeStatus(String text);
    public static native boolean twinBusy();
    public static native boolean twinShown();

    /** Whether the window before the game is minimised (the game's window comes up minimised then). */
    public static native boolean twinHidden();

    /** The game's window up minimised, not taking the focus, its taskbar button blinking. */
    public static native void windowMinimised(long hwnd);
    public static native int[] twinAttach(String name);

    // The launch's sound (launch/sound.rs): its volumes, the hover's tick, the music (0 pause/on, 1 from the start,
    // 2 out; state 0 seconds, 1 length, 2 heard).
    public static native void launchVolume(float master, float music);
    public static native void launchTick();
    public static native boolean launchMusic(int action);
    public static native float launchMusicState(int which);

    // The client's menu (gui/mod.rs): opened with the window's px, the player's name, the game's folder; a frame laid
    // out (its number for GuiFrame, 0 none); the mouse in window px (action 0 moved, 1 pressed, 2 released; button 0
    // left, 1 right, 2 middle); a key by its name (0 not the menu's, 1 taken, 2 the menu goes); a letter; closed
    // (now: the screen is gone already); its state (shown, the head's x y size alpha, a key waited for); the menu key.
    public static native void guiOpen(float width, float height, String player, String dir);
    public static native int guiFrame(float width, float height);
    public static native boolean guiMouse(float x, float y, int button, int action);
    public static native void guiScroll(float dy);
    public static native int guiKey(boolean escape, boolean enter, boolean backspace, String name, boolean menuKey);
    public static native void guiChar(int codepoint);
    public static native void guiClose(boolean now);
    public static native void guiState(float[] out);

    /** Living Motion (native motion.rs): a player's pose from its movement and state (`in`: 12 numbers) into `out` (18). */
    public static native void motionPose(float[] in, float[] out);

    /** Item Physic (native itemphysic.rs): a dropped item's copies from where it is (`at`), what it is and does (`in`: 56
     *  numbers) into `out` (42). */
    public static native void itemPose(double[] at, float[] in, float[] out);
    public static native String guiMenuKey();
    /** The menu's and the HUD's other words (gui::call in native/src/gui/mod.rs): one line, its fields by tabs. */
    public static native String guiCall(String cmd);

    // The HUD (gui/hud.rs): a frame from the game's numbers and names (cursed.recode.hud.HudFacts; its number for
    // GuiFrame, 0 none); the icons over it, 7 numbers each (kind, index, x, y, w, h, alpha; how many); the mouse, the
    // wheel, a key, a letter in the chat or the layout editor (true: the HUD's); a key in the world (true: it switched a
    // module); something seen (1 a totem popped, whose; 2 the target hit); the chat or editor closed; the player and
    // folder at once; its state (effect icons hidden, the editor asked for).
    public static native int hudFrame(float width, float height, double[] nums, String text);
    public static native int hudIcons(float[] out);
    public static native boolean hudMouse(float x, float y, int button, int action);
    public static native boolean hudScroll(float dy);
    public static native boolean hudKey(boolean escape, boolean enter, boolean backspace);
    public static native boolean hudChar(int codepoint);
    public static native boolean hudBind(String name);
    public static native void hudEvent(int kind, String who);
    public static native void hudClosed();
    public static native int hudOverlay();

    /** The frame under the open container (Custom Gui: cursed.recode.hud.HudScreen); 0 none. */
    public static native int hudScreen();

    /** Custom Inventory's frame (gui/hud/hudinv.rs) from the game's numbers and words: its picture under the items. */
    public static native int invFrame(float width, float height, double[] nums, String text);

    /** Custom Inventory's picture over the items, made with the frame (0 none). */
    public static native int invTop();

    /** What Custom Inventory's frame has Java draw where, into `out`: how many numbers. */
    public static native int invOut(float[] out);

    /** The icons over the open container's panel (Custom Gui), as hudIcons. */
    public static native int hudScreenIcons(float[] out);
    public static native void hudInit(String player, String dir);
    public static native void hudState(float[] out);

    // The menus (native/src/menus, cursed.recode.menus): a screen of the game opened as a page (its data JSON in
    // UTF-8), new data for it, gone; a frame (the window's px, the menus' clock in s, what the screen says, a world
    // behind; what they say back into out[64]; its number for the frame op); the mouse in window px (0 moved, 1
    // pressed, 2 released), the wheel, keys by SDL's name with mods (1 shift, 2 ctrl, 4 alt), letters typed; what
    // they ask of the game (lines, UTF-8); the game's language; pictures (RGBA8, rows from the top).
    public static native void menusOpen(int id, String kind, byte[] json);
    public static native void menusData(int id, byte[] json);
    public static native void menusClose(int id);
    public static native int menusFrame(float width, float height, float now, float[] inp, boolean world, float[] out);
    public static native void menusMouse(float x, float y, int button, int action);
    public static native void menusScroll(float dy);
    public static native void menusKey(String name, int mods);
    public static native void menusChar(int codepoint);
    public static native byte[] menusPoll();
    public static native void menusLang(String code);
    public static native void menusPic(String key, int width, int height, byte[] rgba);
    public static native boolean menusPicHas(String key);
    public static native int menusExtras(float[] out);

    // The license (native license.rs, cursed.recode.License): what the game is told (0 all well; 1 the time running out:
    // seconds left, why 0 the subscription ended / 1 no server; 2 go now: seconds since), its words when it must go
    // (headline and text on two lines), its goodbye as it closes.
    public static native int licenseState(float[] out);
    public static native String licenseWords();
    public static native void licenseBye();

    // A server's status protocol asked before joining it (native/src/via.rs; -1: no answer in the time).
    public static native int viaPing(String host, int port, int timeoutMs);
}
