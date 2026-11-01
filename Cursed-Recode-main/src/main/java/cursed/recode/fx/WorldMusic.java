package cursed.recode.fx;

import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import java.net.URI;
import java.net.URLEncoder;
import java.net.http.HttpClient;
import java.net.http.HttpRequest;
import java.net.http.HttpResponse;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.time.Duration;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.Random;
import java.util.concurrent.ConcurrentHashMap;
import java.util.regex.Matcher;
import java.util.regex.Pattern;
import net.minecraft.client.Minecraft;
import net.minecraft.client.multiplayer.ClientLevel;
import net.minecraft.core.BlockPos;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.level.ClipContext;
import net.minecraft.world.phys.HitResult;
import net.minecraft.world.phys.Vec3;

/**
 * World Music (the menu's module "world-music"): the words of the song playing (the HUD's music: the client's own, or
 * whatever Windows' media controls say plays), found with their times on LRCLIB, written into the world round you as
 * they are sung. Each line breaks into pieces of a word or a few; each piece stands in the world on its own — in sight
 * when it is read, clear of the crosshair, at its own distance and place on the screen, out of the blocks with nothing
 * between — a plaque facing you; moving, it is put ahead where you will be, comes toward you as it is written and you
 * go past it (the blocks in front hide it). It is written in light letter by letter as
 * the voice goes, a spark at the pen, each letter white-hot and cooling to the sung colour; held a while, then it burns
 * away from its first letter to its last, embers and ash drifting off. The menu's own letters (their distance fields:
 * sharp at any distance), a glow round them.
 */
final class WorldMusic {
    private static final Pattern STAMP = Pattern.compile("\\[(\\d+):(\\d+(?:\\.\\d+)?)]");
    private static final Pattern NOISE = Pattern.compile("\\s*[(\\[][^)\\]]*(official|video|lyric|audio|clip|mv|visualizer|remaster)[^)\\]]*[)\\]]",
            Pattern.CASE_INSENSITIVE);
    private static final String SUNG = "f3e2c0";
    private static final float BURN = 0.9F;
    private static final HttpClient HTTP = HttpClient.newBuilder().connectTimeout(Duration.ofSeconds(6)).build();
    /** CURSED_DEV_LYRICS="title|artist|seconds": that song played from there (no player needed); CURSED_DEV_LRC=a file:
     *  its words instead of LRCLIB's. */
    private static final String[] DEV = devSong();
    private static final String DEV_LRC = cursed.recode.dev.Dev.on("CURSED_DEV_LRC") ? System.getenv("CURSED_DEV_LRC") : null;
    private static Map<String, String> set = Map.of();
    private static long polled;
    private static long musicAt;
    /** What plays: its key (title and artist), where in it (s), its length, whether it plays; when that was asked. */
    private static String song = "";
    private static String title = "";
    private static String artist = "";
    private static double pos;
    private static double len;
    private static boolean playing;
    private static double pausedFor;
    private static double time;
    private static double devStart = -1.0;
    /** The words of each song asked for: its lines, or none (null in the map: being asked). */
    private static final Map<String, Words> WORDS = new ConcurrentHashMap<>();
    private static final List<Piece> PIECES = new ArrayList<>();
    private static final Map<String, Layout> LAYOUTS = new HashMap<>();
    private static final Bits BITS = new Bits(400);
    private static final Random RANDOM = new Random();
    /** The pieces of the lines (the next not yet shown). */
    private static List<Piece> queue = new ArrayList<>();
    private static int next;
    /** Pieces whose time came with no room for them: tried again each frame while they are sung. */
    private static final List<Piece> WAITING = new ArrayList<>();
    /** How you move (blocks a second): across smoothed over a quarter second, up over more (a jump's up and down
     *  evened out); from where the camera was last frame. */
    private static double vx;
    private static double vy;
    private static double vz;
    private static Vec3 was;
    /** The HUD's plates on the frame, when last asked. */
    private static final List<float[]> PLATES = new ArrayList<>();
    private static long platesAt;

    private record Line(double start, double end, String text) {
    }

    /** A song's words: its lines (empty: none found), when last asked (a failure is asked again later). */
    private record Words(List<Line> lines, boolean failed, double at) {
    }

    /** A piece's letters as the menu lays them out: ascent, descent, width (em); each letter's box (em) and atlas rect;
     *  whether a letter of it is still being drawn from the system's fonts. */
    private record Layout(float ascent, float descent, float width, float[] g, int[] page, boolean waiting) {
        int letters() {
            return this.page.length;
        }
    }

    /** A piece of a line: its words, when they are sung (song s); where it stands once shown (its top left corner, the
     *  ways across and down, a block to the em), when it was shown and when it began to burn (world s). */
    private static final class Piece {
        final String text;
        final double start;
        final double end;
        final double lineEnd;
        final boolean accent;
        double[] at;
        double[] right;
        double[] down;
        float em;
        int face;
        Layout layout;
        double shown = -1.0;
        double burning = -1.0;
        float emitted;

        Piece(String text, double start, double end, double lineEnd, boolean accent) {
            this.text = text;
            this.start = start;
            this.end = end;
            this.lineEnd = lineEnd;
            this.accent = accent;
        }
    }

    private WorldMusic() {
    }

    private static String[] devSong() {
        if (!cursed.recode.dev.Dev.on("CURSED_DEV_LYRICS")) {
            return null;
        }
        String[] p = System.getenv("CURSED_DEV_LYRICS").split("\\|");
        return p.length >= 3 ? p : null;
    }

    private static Map<String, String> settings() {
        long now = System.nanoTime();
        if (now - polled > 250_000_000L) {
            polled = now;
            set = Fx.settings("world-music");
        }
        return set;
    }

    static void gone() {
        PIECES.clear();
        WAITING.clear();
        BITS.clear();
    }

    /** What plays, asked of the menu five times a second (moved on by the time between while it plays). */
    private static void listen(float dt) {
        if (playing) {
            pos += dt;
        }
        long now = System.nanoTime();
        if (now - musicAt < 200_000_000L) {
            return;
        }
        musicAt = now;
        String t;
        String a;
        double p;
        double l;
        boolean on;
        if (DEV != null) {
            if (devStart < 0.0) {
                devStart = time - Double.parseDouble(DEV[2]);
            }
            t = DEV[0];
            a = DEV[1];
            p = time - devStart;
            // (its length not known: none asked for)
            l = 0.0;
            on = true;
        } else {
            String m = cursed.recode.render.Native.guiCall("music");
            String[] f = m == null ? new String[0] : m.split("\t");
            if (f.length < 5) {
                playing = false;
                song = "";
                return;
            }
            t = f[0];
            a = f[1];
            try {
                p = Double.parseDouble(f[2]);
                l = Double.parseDouble(f[3]);
            } catch (NumberFormatException e) {
                return;
            }
            on = f[4].equals("1");
        }
        String key = t + "\u0000" + a;
        if (!key.equals(song)) {
            song = key;
            title = t;
            artist = a;
            pos = p;
            next = 0;
            queue = new ArrayList<>();
            WAITING.clear();
            // the last song's words burn away
            for (Piece q : PIECES) {
                if (q.burning < 0.0) {
                    q.burning = time;
                }
            }
        } else if (Math.abs(p - pos) > 0.35) {
            // moved in the song: its pieces from there on
            boolean jumped = Math.abs(p - pos) > 2.0;
            pos = p;
            if (jumped) {
                // (made anew: the pieces shown before keep their own state, and burn away as they were)
                queue = new ArrayList<>();
                next = 0;
                WAITING.clear();
            }
        } else {
            pos += (p - pos) * 0.2;
        }
        len = l;
        playing = on;
    }

    /** The song's words: asked of LRCLIB once (again after a failure, a while later). */
    private static List<Line> words() {
        if (song.isEmpty()) {
            return List.of();
        }
        Words w = WORDS.get(song);
        if (w == null || w.failed && time - w.at > 20.0) {
            if (w == null || w.failed) {
                WORDS.put(song, new Words(List.of(), false, time));
                ask(song, title, artist, len);
            }
            return List.of();
        }
        return w.lines;
    }

    private static void ask(String key, String t, String a, double length) {
        if (DEV_LRC != null) {
            try {
                WORDS.put(key, new Words(parse(Files.readString(Path.of(DEV_LRC))), false, time));
            } catch (Exception e) {
                WORDS.put(key, new Words(List.of(), true, time));
            }
            return;
        }
        String[] ta = clean(t, a);
        String q = "track_name=" + URLEncoder.encode(ta[0], StandardCharsets.UTF_8) + "&artist_name=" + URLEncoder.encode(ta[1], StandardCharsets.UTF_8);
        String exact = "https://lrclib.net/api/get?" + q + (length > 1.0 ? "&duration=" + Math.round(length) : "");
        get(exact).thenCompose(r -> {
            if (r != null && r.statusCode() == 200) {
                List<Line> lines = synced(JsonParser.parseString(r.body()));
                if (!lines.isEmpty()) {
                    return java.util.concurrent.CompletableFuture.completedFuture(lines);
                }
            }
            if (r == null) {
                return java.util.concurrent.CompletableFuture.completedFuture((List<Line>) null);
            }
            // not that one: the closest found with times
            return get("https://lrclib.net/api/search?" + q).thenApply(s -> {
                if (s == null || s.statusCode() != 200) {
                    return s == null ? null : List.<Line>of();
                }
                JsonElement all = JsonParser.parseString(s.body());
                if (!all.isJsonArray()) {
                    return List.<Line>of();
                }
                for (JsonElement e : (JsonArray) all) {
                    JsonObject o = e.getAsJsonObject();
                    double d = o.has("duration") && !o.get("duration").isJsonNull() ? o.get("duration").getAsDouble() : 0.0;
                    if (length > 1.0 && d > 0.0 && Math.abs(d - length) > 8.0) {
                        continue;
                    }
                    List<Line> lines = synced(o);
                    if (!lines.isEmpty()) {
                        return lines;
                    }
                }
                return List.<Line>of();
            });
        }).whenComplete((lines, err) -> {
            // a failure (no answer) is asked again later; no words is kept
            boolean failed = err != null || lines == null;
            WORDS.put(key, new Words(failed ? List.of() : lines, failed, time));
            if (!failed) {
                org.slf4j.LoggerFactory.getLogger("WorldMusic").info("[world music] {} lines for \"{}\" by {}", lines.size(), ta[0], ta[1]);
            }
        });
    }

    private static java.util.concurrent.CompletableFuture<HttpResponse<String>> get(String url) {
        HttpRequest r = HttpRequest.newBuilder(URI.create(url)).timeout(Duration.ofSeconds(10)).header("User-Agent", "Cursed Recode (https://cursedclient.xyz)").GET().build();
        return HTTP.sendAsync(r, HttpResponse.BodyHandlers.ofString(StandardCharsets.UTF_8)).exceptionally(e -> null);
    }

    /** A title and an artist as a player gives them, cleaned: "Artist - Title" taken apart, "(Official Video)" off. */
    private static String[] clean(String t, String a) {
        String tt = NOISE.matcher(t).replaceAll("").trim();
        String aa = a.trim();
        int dash = tt.indexOf(" - ");
        if ((aa.isEmpty() || aa.toLowerCase().contains("topic") || tt.toLowerCase().startsWith(aa.toLowerCase() + " - ")) && dash > 0) {
            aa = tt.substring(0, dash).trim();
            tt = tt.substring(dash + 3).trim();
        }
        return new String[] {tt, aa};
    }

    private static List<Line> synced(JsonElement e) {
        if (!e.isJsonObject()) {
            return List.of();
        }
        JsonObject o = e.getAsJsonObject();
        if (!o.has("syncedLyrics") || o.get("syncedLyrics").isJsonNull()) {
            return List.of();
        }
        return parse(o.get("syncedLyrics").getAsString());
    }

    /** "[mm:ss.xx]words" lines (a line may have several times) into lines, each to the next's start. */
    private static List<Line> parse(String lrc) {
        List<double[]> times = new ArrayList<>();
        List<String> texts = new ArrayList<>();
        for (String raw : lrc.split("\r?\n")) {
            Matcher m = STAMP.matcher(raw);
            int end = 0;
            List<Double> at = new ArrayList<>();
            while (m.find() && m.start() == end) {
                at.add(Integer.parseInt(m.group(1)) * 60.0 + Double.parseDouble(m.group(2)));
                end = m.end();
            }
            String text = raw.substring(end).trim();
            for (double t : at) {
                times.add(new double[] {t, texts.size()});
                texts.add(text);
            }
        }
        times.sort((x, y) -> Double.compare(x[0], y[0]));
        List<Line> lines = new ArrayList<>();
        for (int i = 0; i < times.size(); i++) {
            String text = texts.get((int) times.get(i)[1]);
            double start = times.get(i)[0];
            double end = i + 1 < times.size() ? times.get(i + 1)[0] : start + 5.0;
            if (!text.isEmpty()) {
                lines.add(new Line(start, end, text));
            }
        }
        return lines;
    }

    /** A line's pieces: a word or a few (as many as fit sixteen letters), each its share of the line's time by its
     *  letters; the last one stands out. */
    private static List<Piece> pieces(Line l) {
        String[] words = l.text.split("\\s+");
        List<String> parts = new ArrayList<>();
        StringBuilder b = new StringBuilder();
        int count = 0;
        for (String w : words) {
            if (w.isEmpty()) {
                continue;
            }
            if (b.length() > 0 && (b.length() + 1 + w.length() > 16 || count >= 3)) {
                parts.add(b.toString());
                b.setLength(0);
                count = 0;
            }
            if (b.length() > 0) {
                b.append(' ');
            }
            b.append(w);
            count++;
        }
        if (b.length() > 0) {
            parts.add(b.toString());
        }
        // sung over the line's time, no longer than its letters take to sing
        int letters = parts.stream().mapToInt(String::length).sum();
        double span = Math.min(l.end - l.start, 0.75 + letters * 0.085);
        List<Piece> out = new ArrayList<>();
        double t = l.start;
        for (int i = 0; i < parts.size(); i++) {
            String p = parts.get(i);
            double d = span * p.length() / Math.max(1, letters);
            out.add(new Piece(p, t, t + d, l.end, i == parts.size() - 1 && parts.size() > 1));
            t += d;
        }
        return out;
    }

    private static Layout layout(int face, String text) {
        String key = face + "\u0000" + text;
        Layout had = LAYOUTS.get(key);
        if (had != null) {
            return had;
        }
        if (LAYOUTS.size() > 512) {
            LAYOUTS.clear();
        }
        Layout made = lay(face, text);
        // (one with a letter still being drawn from the system's fonts is asked again, not kept)
        if (made != null && !made.waiting()) {
            LAYOUTS.put(key, made);
        }
        return made;
    }

    private static Layout lay(int face, String text) {
        String s = cursed.recode.render.Native.guiCall("glyphs\t" + face + "\t" + text);
        if (s == null || s.isEmpty()) {
            return null;
        }
        String[] parts = s.split(";");
        String[] head = parts[0].trim().split(" ");
        int n = 0;
        for (int i = 1; i < parts.length; i++) {
            if (!parts[i].isBlank()) {
                n++;
            }
        }
        float[] g = new float[n * 8];
        int[] page = new int[n];
        int j = 0;
        for (int i = 1; i < parts.length; i++) {
            if (parts[i].isBlank()) {
                continue;
            }
            String[] f = parts[i].trim().split(" ");
            for (int c = 0; c < 8; c++) {
                g[j * 8 + c] = Float.parseFloat(f[c]);
            }
            page[j] = Integer.parseInt(f[8]);
            j++;
        }
        return new Layout(Float.parseFloat(head[0]), Float.parseFloat(head[1]), Float.parseFloat(head[2]), g, page, head.length > 3 && head[3].equals("1"));
    }

    static void draw(Minecraft mc, float partial, float dt, Vec3 cam) {
        Map<String, String> v = settings();
        ClientLevel level = mc.level;
        time += dt;
        // how you move: a piece is put where you will read it
        if (was != null && dt > 0.0F) {
            double mx = cam.x - was.x;
            double my = cam.y - was.y;
            double mz = cam.z - was.z;
            if (mx * mx + my * my + mz * mz > 64.0) {
                // gone somewhere else at once (a teleport): not moving
                vx = 0.0;
                vy = 0.0;
                vz = 0.0;
            } else {
                double across = 1.0 - Math.exp(-dt / 0.25);
                double upward = 1.0 - Math.exp(-dt / 0.6);
                vx += (mx / dt - vx) * across;
                vz += (mz / dt - vz) * across;
                vy += (my / dt - vy) * upward;
            }
        }
        was = cam;
        boolean on = Fx.on(v, "on", false);
        if (on) {
            listen(dt);
        } else {
            playing = false;
        }
        pausedFor = playing ? 0.0 : pausedFor + dt;
        double now = pos + Fx.number(v, "Offset", 0.0F);
        if (on && !song.isEmpty()) {
            List<Line> lines = words();
            if (queue.isEmpty() && !lines.isEmpty()) {
                for (Line l : lines) {
                    queue.addAll(pieces(l));
                }
                next = 0;
                while (next < queue.size() && queue.get(next).end < now) {
                    next++;
                }
            }
            // the pieces whose time has come, shown where there is room for them
            while (playing && next < queue.size() && queue.get(next).start - 0.1 <= now) {
                WAITING.add(queue.get(next++));
            }
            for (int i = 0; i < WAITING.size(); i++) {
                Piece q = WAITING.get(i);
                if (q.end < now - 0.3 || !playing) {
                    WAITING.remove(i--);
                } else if (place(mc, level, q, v, cam, partial)) {
                    PIECES.add(q);
                    WAITING.remove(i--);
                }
            }
        }
        int most = Math.round(Fx.number(v, "Pieces at once", 4.0F));
        int live = 0;
        for (int i = PIECES.size() - 1; i >= 0; i--) {
            Piece q = PIECES.get(i);
            if (q.burning < 0.0) {
                live++;
                // held while it is sung and a while after; too many, or nothing playing a while, and it goes
                boolean done = now > Math.max(q.lineEnd, q.end) + 1.8 && time - q.shown > 2.6;
                if (done || live > most || pausedFor > 6.0 || !on) {
                    q.burning = time;
                }
            }
        }
        int rgb = Fx.colour(v, "Colour", 0xffffff);
        int sung = Fx.colour(v, "Sung colour", Integer.parseInt(SUNG, 16));
        float glow = Fx.number(v, "Glow", 1.0F);
        int flags = Fx.on(v, "Through blocks", false) ? Fx.XRAY : 0;
        boolean ash = Fx.on(v, "Ash when they go", true);
        for (int i = PIECES.size() - 1; i >= 0; i--) {
            Piece q = PIECES.get(i);
            if (q.burning >= 0.0 && time - q.burning > BURN + 0.1) {
                PIECES.remove(i);
                continue;
            }
            write(q, now, rgb, sung, glow, flags, ash, dt);
        }
        if (level != null) {
            BITS.step(level, dt, 1.0F);
        }
    }

    /** Where a piece stands: in sight when it is read — as far off as asked, at its own place on the screen (across it
     *  and up and down it, clear of the crosshair, clear of the others, not far overhead nor at your feet), whole in the
     *  frame and off the HUD's plates, out of the blocks with nothing between, facing where it is read from. Moving, it is put where you will be by then: ahead, coming toward you as it is written,
     *  and you go past it; in sight from where you are now too, and never in your face. */
    private static boolean place(Minecraft mc, ClientLevel level, Piece q, Map<String, String> v, Vec3 eye, float partial) {
        if (level == null) {
            return false;
        }
        Entity cam = mc.getCameraEntity() != null ? mc.getCameraEntity() : mc.player;
        if (cam == null) {
            return false;
        }
        String weight = Fx.choice(v, "Font weight", "Semi");
        q.face = switch (weight) {
            case "Regular" -> 0;
            case "Medium" -> 1;
            case "Bold" -> 3;
            default -> 2;
        };
        if (q.accent && q.face < 3) {
            q.face++;
        }
        q.layout = layout(q.face, q.text);
        if (q.layout == null || q.layout.letters() == 0 || q.layout.waiting()) {
            return false;
        }
        float spread = Fx.number(v, "Spread", 8.0F);
        float size = Fx.number(v, "Size", 1.0F) * (q.accent ? 1.18F : 1.0F);
        double yaw = Math.toRadians(cam.getViewYRot(partial));
        double pitch = Math.toRadians(cam.getViewXRot(partial));
        // the way you look: ahead, to your right, up
        Vec3 fwd = new Vec3(-Math.sin(yaw) * Math.cos(pitch), -Math.sin(pitch), Math.cos(yaw) * Math.cos(pitch));
        Vec3 side = new Vec3(-Math.cos(yaw), 0.0, -Math.sin(yaw));
        Vec3 up = side.cross(fwd);
        // how far the frame reaches either way (the tangents of half the view across and up, the level's projection's)
        org.joml.Matrix4f pm = cursed.recode.render.NativeTerrain.PROJECTION;
        double tanX = Math.abs(pm.m00()) > 1e-4 ? 1.0 / Math.abs(pm.m00()) : 1.2;
        double tanY = Math.abs(pm.m11()) > 1e-4 ? 1.0 / Math.abs(pm.m11()) : 0.7;
        // when it is read: as it is half written (a moment at least, a while at most; sooner when fast, so not too far)
        double speed = Math.sqrt(vx * vx + vy * vy + vz * vz);
        double read = Math.clamp((q.end - q.start) * 0.6 + 0.5, 0.8, 2.2);
        if (speed > 1e-3) {
            read = Math.min(read, 20.0 / speed);
        }
        for (int attempt = 0; attempt < 32; attempt++) {
            // moving: where you will be then (the last tries where you are: ahead may have no room)
            double ahead = speed > 0.8 && attempt < 24 ? read : 0.0;
            Vec3 then = eye.add(vx * ahead, vy * ahead, vz * ahead);
            double d = 3.5 + RANDOM.nextDouble() * Math.max(0.5, spread - 3.5);
            double em = size * 0.062 * d;
            double w = q.layout.width() * em;
            double h = (q.layout.ascent() + Math.abs(q.layout.descent())) * em;
            // its place on the frame then (-1..1 across, up): anywhere in it but the crosshair, its edges in
            double hw = w * 0.5 / (d * tanX);
            double hh = h * 0.5 / (d * tanY);
            double roomX = 0.86 - hw;
            if (roomX < 0.1 || hh > 0.6) {
                continue;
            }
            double sx = (RANDOM.nextDouble() * 2.0 - 1.0) * roomX;
            double sy = Math.clamp(-0.5 + RANDOM.nextDouble() * 1.15, -0.82 + hh, 0.82 - hh);
            if (Math.abs(sx) < hw + 0.12 && Math.abs(sy) < hh + 0.1) {
                continue;
            }
            Vec3 dir = fwd.add(side.scale(sx * tanX)).add(up.scale(sy * tanY)).normalize();
            // not far over your head nor at your feet
            if (dir.y > 0.62 || dir.y < -0.55) {
                continue;
            }
            Vec3 p = then.add(dir.scale(d));
            // facing where it is read from, both ways (seen from below, an upright one leant away and ran off the
            // frame): across to the reader's right (level), down along its face
            double fl = Math.max(Math.sqrt(dir.x * dir.x + dir.z * dir.z), 1e-6);
            Vec3 right = new Vec3(-dir.z / fl, 0.0, dir.x / fl);
            Vec3 down = dir.cross(right).normalize();
            Vec3 corner = p.subtract(right.scale(w * 0.5)).subtract(down.scale(h * 0.5));
            // all of it in sight now and then (moving on, it is further along your way now), off the HUD's plates, not
            // in your face
            if (p.distanceToSqr(eye) < 2.5 * 2.5 || !inSight(corner, right.scale(w), down.scale(h), eye, fwd, side, up, tanX, tanY)
                    || ahead > 0.0 && !inSight(corner, right.scale(w), down.scale(h), then, fwd, side, up, tanX, tanY)) {
                continue;
            }
            // clear of the others shown, as they lie from here and from where it is read
            if (crowded(eye, p, w, h) || ahead > 0.0 && crowded(then, p, w, h)) {
                continue;
            }
            // in the air, nothing between
            if (!level.getBlockState(BlockPos.containing(p)).getCollisionShape(level, BlockPos.containing(p)).isEmpty()) {
                continue;
            }
            HitResult hit = level.clip(new ClipContext(eye, p, ClipContext.Block.VISUAL, ClipContext.Fluid.NONE, cam));
            if (hit.getType() != HitResult.Type.MISS && hit.getLocation().distanceToSqr(eye) < p.distanceToSqr(eye) - 0.3) {
                continue;
            }
            q.right = new double[] {right.x, right.y, right.z};
            q.down = new double[] {down.x, down.y, down.z};
            q.em = (float) em;
            q.at = new double[] {corner.x, corner.y, corner.z};
            q.shown = time;
            return true;
        }
        return false;
    }

    /** Whether a piece from `corner` (its top left) along `across` and `downward` is whole in the frame seen from
     *  `from`, each corner a little in from the frame's edges, and clear of the HUD's plates. */
    private static boolean inSight(Vec3 corner, Vec3 across, Vec3 downward, Vec3 from, Vec3 fwd, Vec3 side, Vec3 up, double tanX, double tanY) {
        // where it lies on the frame (-1..1 across, up)
        double x0 = 1.0;
        double y0 = 1.0;
        double x1 = -1.0;
        double y1 = -1.0;
        for (int k = 0; k < 4; k++) {
            Vec3 rel = corner.add((k & 1) != 0 ? across : Vec3.ZERO).add((k & 2) != 0 ? downward : Vec3.ZERO).subtract(from);
            double z = rel.dot(fwd);
            if (z < 1.0) {
                return false;
            }
            double x = rel.dot(side) / (z * tanX);
            double y = rel.dot(up) / (z * tanY);
            if (Math.abs(x) > 0.94 || Math.abs(y) > 0.92) {
                return false;
            }
            x0 = Math.min(x0, x);
            y0 = Math.min(y0, y);
            x1 = Math.max(x1, x);
            y1 = Math.max(y1, y);
        }
        for (float[] r : plates()) {
            if (x0 < r[2] && x1 > r[0] && y0 < r[3] && y1 > r[1]) {
                return false;
            }
        }
        return true;
    }

    /** The HUD's plates (the music, the modules' list, the chat…) on the frame (-1..1 across, up: lowest x, y, highest),
     *  asked of the menu four times a second. */
    private static List<float[]> plates() {
        long now = System.nanoTime();
        if (now - platesAt < 250_000_000L) {
            return PLATES;
        }
        platesAt = now;
        PLATES.clear();
        String s = cursed.recode.render.Native.guiCall("plates");
        if (s == null || s.isEmpty()) {
            return PLATES;
        }
        String[] parts = s.split(";");
        String[] size = parts[0].trim().split(" ");
        try {
            float w = Float.parseFloat(size[0]);
            float h = Float.parseFloat(size[1]);
            if (w <= 0.0F || h <= 0.0F) {
                return PLATES;
            }
            for (int i = 1; i < parts.length; i++) {
                String[] r = parts[i].trim().split(" ");
                // (their shadows' margin, less a little)
                float m = 12.0F;
                float ax = Float.parseFloat(r[0]) + m;
                float ay = Float.parseFloat(r[1]) + m;
                float bx = Float.parseFloat(r[2]) - m;
                float by = Float.parseFloat(r[3]) - m;
                if (bx > ax && by > ay) {
                    PLATES.add(new float[] {ax / w * 2.0F - 1.0F, 1.0F - by / h * 2.0F, bx / w * 2.0F - 1.0F, 1.0F - ay / h * 2.0F});
                }
            }
        } catch (RuntimeException e) {
            PLATES.clear();
        }
        return PLATES;
    }

    /** Whether a piece `w` by `h` blocks round `p` would lie over another shown, as they lie from `from` (a piece
     *  burning stays in the way until it is mostly gone). */
    private static boolean crowded(Vec3 from, Vec3 p, double w, double h) {
        Vec3 dir = p.subtract(from);
        double dist = dir.length();
        if (dist < 1e-3) {
            return true;
        }
        dir = dir.scale(1.0 / dist);
        double half = Math.atan2(w * 0.5, dist);
        double halfUp = Math.atan2(h * 0.5, dist);
        for (Piece o : PIECES) {
            if (o.at == null || o.burning >= 0.0 && time - o.burning > BURN * 0.6) {
                continue;
            }
            double oh = (o.layout.ascent() + Math.abs(o.layout.descent())) * o.em;
            double ow = o.layout.width() * o.em;
            Vec3 oc = new Vec3(o.at[0] + o.right[0] * ow * 0.5 + o.down[0] * oh * 0.5, o.at[1] + o.right[1] * ow * 0.5 + o.down[1] * oh * 0.5,
                    o.at[2] + o.right[2] * ow * 0.5 + o.down[2] * oh * 0.5);
            Vec3 od = oc.subtract(from);
            double odist = od.length();
            if (odist < 1e-3) {
                continue;
            }
            od = od.scale(1.0 / odist);
            // how far across and up apart, from there, against how far each reaches
            double across = Math.atan2(-od.x, od.z) - Math.atan2(-dir.x, dir.z);
            across = Math.abs(Math.atan2(Math.sin(across), Math.cos(across)));
            double upApart = Math.abs(Math.asin(Math.clamp(od.y, -1.0, 1.0)) - Math.asin(Math.clamp(dir.y, -1.0, 1.0)));
            double theirs = Math.atan2(o.layout.width() * o.em * 0.5, odist);
            double theirsUp = Math.atan2(oh * 0.5, odist);
            if (across < half + theirs + Math.toRadians(2.5) && upApart < halfUp + theirsUp + Math.toRadians(2.0)) {
                return true;
            }
        }
        return false;
    }

    /** A piece this frame: its letters as far as sung (the pen's letter coming in, white-hot, cooling to the sung
     *  colour), the spark at the pen; burning away from its first letter, embers and ash off the burning edge. */
    private static void write(Piece q, double now, int rgb, int sung, float glow, int flags, boolean ash, float dt) {
        Layout l = q.layout;
        int n = l.letters();
        double sungTo = Math.clamp((now - q.start) / Math.max(q.end - q.start, 0.05), 0.0, 1.0);
        double pen = sungTo * n;
        double burn = q.burning >= 0.0 ? Math.clamp((time - q.burning) / BURN, 0.0, 1.0) : -1.0;
        float em = q.em;
        for (int i = 0; i < n; i++) {
            float in = (float) Math.clamp(pen - i, 0.0, 1.0);
            if (in <= 0.0F) {
                break;
            }
            float k = in;
            // white-hot as it is written, cooled a little after
            double writtenAt = q.start + (i + 1.0) / n * Math.max(q.end - q.start, 0.05);
            float heat = (float) Math.clamp(1.0 - (now - writtenAt) / 0.7, 0.0, 1.0);
            if (burn >= 0.0) {
                // the burning edge goes from the first letter to the last
                double edge = burn * (n + 2.0) - 1.0;
                double over = edge - i;
                if (over > 1.0) {
                    continue;
                }
                if (over > 0.0) {
                    k *= (float) (1.0 - over);
                    heat = Math.max(heat, (float) (1.0 - over));
                }
            }
            int c = Bits.mix(sung, 0xffffff, heat * 0.85);
            if (burn < 0.0 && rgb != 0xffffff) {
                c = Bits.mix(rgb, c, 0.6);
            }
            float[] g = l.g();
            double x0 = g[i * 8] * em;
            double y0 = (l.ascent() + g[i * 8 + 1]) * em;
            double ox = q.at[0] + q.right[0] * x0 + q.down[0] * y0;
            double oy = q.at[1] + q.right[1] * x0 + q.down[1] * y0;
            double oz = q.at[2] + q.right[2] * x0 + q.down[2] * y0;
            double w = g[i * 8 + 2] * em;
            double h = g[i * 8 + 3] * em;
            Fx.letter(ox, oy, oz, q.right[0] * w, q.right[1] * w, q.right[2] * w, q.down[0] * h, q.down[1] * h, q.down[2] * h, g[i * 8 + 4], g[i * 8 + 5], g[i * 8 + 6], g[i * 8 + 7], l.page()[i],
                    glow * (1.0F + 0.5F * heat), c, k * (1.0F + 0.25F * heat), flags);
        }
        double mid = (l.ascent() * 0.55) * em;
        // the spark at the pen while it writes
        if (burn < 0.0 && sungTo > 0.0 && sungTo < 1.0) {
            int i = Math.min(n - 1, (int) pen);
            float[] g = l.g();
            double x = (g[i * 8] + g[i * 8 + 2] * (pen - i)) * em;
            double sx = q.at[0] + q.right[0] * x + q.down[0] * mid;
            double sy = q.at[1] + q.right[1] * x + q.down[1] * mid;
            double sz = q.at[2] + q.right[2] * x + q.down[2] * mid;
            Fx.dot(sx, sy, sz, 0.012F * em / 0.3F, 2.6F * glow, 0xffffff, 1.4F, flags);
            q.emitted += dt * 18.0F;
            while (q.emitted >= 1.0F) {
                q.emitted -= 1.0F;
                BITS.add(sx, sy, sz, RANDOM.nextGaussian() * 0.5, 0.3 + RANDOM.nextDouble() * 0.8, RANDOM.nextGaussian() * 0.5, 0.25F + RANDOM.nextFloat() * 0.35F,
                        0.012F + RANDOM.nextFloat() * 0.012F, 0xffe8c8, Bits.EMBER, 3.0F, 2.0F);
            }
        }
        // embers and ash off the burning edge
        if (burn >= 0.0 && burn < 1.0 && ash) {
            double edge = Math.clamp(burn * (n + 2.0) - 1.0, 0.0, n);
            float[] g = l.g();
            int i = Math.min(n - 1, (int) edge);
            double x = (g[i * 8] + g[i * 8 + 2] * (edge - i)) * em;
            q.emitted += dt * 60.0F;
            while (q.emitted >= 1.0F) {
                q.emitted -= 1.0F;
                double y = mid * 2.0 * RANDOM.nextDouble();
                double px = q.at[0] + q.right[0] * x + q.down[0] * y;
                double py = q.at[1] + q.right[1] * x + q.down[1] * y;
                double pz = q.at[2] + q.right[2] * x + q.down[2] * y;
                if (RANDOM.nextFloat() < 0.55F) {
                    BITS.add(px, py, pz, RANDOM.nextGaussian() * 0.3 + 0.4, 0.4 + RANDOM.nextDouble() * 0.7, RANDOM.nextGaussian() * 0.3 + 0.2, 0.4F + RANDOM.nextFloat() * 0.5F,
                            0.012F + RANDOM.nextFloat() * 0.014F, 0xffd9a8, Bits.EMBER, 1.5F, 1.4F);
                } else {
                    BITS.add(px, py, pz, RANDOM.nextGaussian() * 0.25 + 0.5, 0.15 + RANDOM.nextDouble() * 0.4, RANDOM.nextGaussian() * 0.25 + 0.25,
                            0.8F + RANDOM.nextFloat() * 0.8F, 0.014F + RANDOM.nextFloat() * 0.016F, 0x6a6a6a, Bits.GRAIN, 0.6F, 1.2F);
                }
            }
        }
    }
}
