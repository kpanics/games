package cursed.recode.alts;

import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import cursed.recode.CursedRecode;
import cursed.recode.render.Native;
import java.io.Closeable;
import java.io.InputStream;
import java.io.OutputStream;
import java.net.InetAddress;
import java.net.InetSocketAddress;
import java.net.ServerSocket;
import java.net.Socket;
import java.net.SocketTimeoutException;
import java.net.URI;
import java.net.URLDecoder;
import java.net.URLEncoder;
import java.net.http.HttpClient;
import java.net.http.HttpRequest;
import java.net.http.HttpResponse;
import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.security.SecureRandom;
import java.time.Duration;
import java.util.ArrayList;
import java.util.Base64;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.CopyOnWriteArrayList;
import net.minecraft.client.Minecraft;
import net.minecraft.client.User;
import org.jspecify.annotations.Nullable;

/**
 * Signing in with a Microsoft account (a licence of the game) as the free Cursed does: the browser opens Microsoft's own
 * page (the client never sees the password), its answer comes to this computer's port (the address the app is
 * registered with), and Microsoft → Xbox Live → XSTS → Minecraft gives the game's session and the profile. What stays is
 * Microsoft's refresh token, sealed by Windows for this user (native alts.rs); the game's token lives in memory only and
 * is asked again when it runs out. Nothing of it goes to Cursed's site. The app is Cursed's (a friend of hers made it).
 */
final class Msa {
    private Msa() {
    }

    private static final String CLIENT_ID = "1c49812b-4322-4e35-b968-304ae245e89d";
    private static final int PORT = 59125;
    private static final String REDIRECT = "http://localhost:" + PORT;
    private static final String AUTHORIZE = "https://login.microsoftonline.com/consumers/oauth2/v2.0/authorize";
    private static final String TOKEN = "https://login.microsoftonline.com/consumers/oauth2/v2.0/token";
    private static final String SCOPE = "XboxLive.signin offline_access";
    /** How long the browser's answer is waited for. */
    private static final long WAIT_MS = 300_000;
    private static final HttpClient HTTP = HttpClient.newBuilder().connectTimeout(Duration.ofSeconds(15)).build();
    private static final SecureRandom RANDOM = new SecureRandom();

    /** A sign-in under way (a second press starts anew: the first one gives up), and its sockets (closed by the next). */
    private static volatile int attempt;
    private static final List<Closeable> OPEN = new CopyOnWriteArrayList<>();
    /** The game's sessions got, by name (lower case), till when (System.nanoTime). */
    private record Kept(User user, long until) {
    }

    private static final Map<String, Kept> KEPT = new ConcurrentHashMap<>();
    private static final Map<String, Boolean> RENEWING = new ConcurrentHashMap<>();

    /** What goes wrong, in words the menus put in the game's language (`words`), with what goes after them. */
    private static final class Failed extends Exception {
        final String detail;

        Failed(String words, String detail) {
            super(words);
            this.detail = detail;
        }

        Failed(String words) {
            this(words, "");
        }
    }

    static void login(Minecraft mc) {
        int mine = ++attempt;
        // the sign-in before gives up at once: its port is this one's
        for (Closeable c : OPEN) {
            shut(c);
        }
        OPEN.clear();
        Thread t = new Thread(() -> {
            try {
                User user = signIn(mine);
                mc.execute(() -> Alts.set(mc, user, true));
                Alts.status(1, "Signed in as", user.getName());
            } catch (Failed f) {
                if (attempt == mine && !f.getMessage().isEmpty()) {
                    Alts.status(2, f.getMessage(), f.detail);
                }
            } catch (Exception e) {
                CursedRecode.LOGGER.warn("[alts] Microsoft's sign-in", e);
                Alts.status(2, "The sign-in failed", e.getClass().getSimpleName());
            }
        }, "recode-msa");
        t.setDaemon(true);
        t.start();
    }

    /** A Microsoft account put to use: its game session (kept, else renewed with its key in the background). */
    static void renew(Minecraft mc, String name) {
        String key = name.toLowerCase(java.util.Locale.ROOT);
        Kept k = KEPT.get(key);
        if (k != null && k.until() > System.nanoTime()) {
            Alts.set(mc, k.user(), true);
            return;
        }
        if (RENEWING.putIfAbsent(key, Boolean.TRUE) != null) {
            return;
        }
        Alts.status(0, "Renewing the licence of", name);
        Thread t = new Thread(() -> {
            try {
                String refresh = Native.guiCall("alts\tmskey\t" + name);
                if (refresh == null || refresh.isEmpty()) {
                    throw new Failed("The account's key cannot be opened on this Windows user: sign in again");
                }
                JsonObject v = post(TOKEN, "application/x-www-form-urlencoded",
                        form(Map.of("client_id", CLIENT_ID, "grant_type", "refresh_token", "refresh_token", refresh, "scope", SCOPE)));
                if (!v.has("access_token") || !v.has("refresh_token")) {
                    throw new Failed("Microsoft asks to sign in again", str(v, "error"));
                }
                User user = minecraft(str(v, "access_token"));
                Native.guiCall("alts\tmsrekey\t" + name + "\t" + str(v, "refresh_token"));
                // (the account in use may have changed meanwhile: only this one's session is put to use)
                mc.execute(() -> {
                    if (Alts.wanted(mc).equalsIgnoreCase(name)) {
                        Alts.set(mc, user, true);
                    }
                });
                Alts.status(1, "Licence ready", user.getName());
            } catch (Failed f) {
                Alts.status(2, f.getMessage(), f.detail);
            } catch (Exception e) {
                CursedRecode.LOGGER.warn("[alts] renewing {}", name, e);
                Alts.status(2, "The licence could not be renewed", e.getClass().getSimpleName());
            } finally {
                RENEWING.remove(key);
            }
        }, "recode-msa-renew");
        t.setDaemon(true);
        t.start();
    }

    // ------------------------------------------------------------------------------------------ signing in

    private static User signIn(int mine) throws Exception {
        List<ServerSocket> ports = listen();
        OPEN.addAll(ports);
        if (attempt != mine) {
            // a press after this one came while it bound: that one's closing missed these
            ports.forEach(Msa::shut);
            OPEN.removeAll(ports);
            throw new Failed("");
        }
        Socket got = null;
        Map<String, String> q = Map.of();
        try {
            String verifier = b64url(bytes(48));
            String challenge = b64url(MessageDigest.getInstance("SHA-256").digest(verifier.getBytes(StandardCharsets.US_ASCII)));
            String state = b64url(bytes(16));
            String url = AUTHORIZE + "?client_id=" + CLIENT_ID + "&response_type=code&redirect_uri=" + enc(REDIRECT) + "&scope=" + enc(SCOPE)
                    + "&code_challenge=" + challenge + "&code_challenge_method=S256&state=" + state + "&prompt=select_account";
            if (cursed.recode.dev.Dev.on("CURSED_DEV_MSA_NOBROWSER")) {
                // the test stand: no browser opened on the computer, the address only said (its answer is sent by hand)
                CursedRecode.LOGGER.info("[alts] the sign-in's page: {}", url);
            } else {
                com.mojang.blaze3d.Blaze3D.openUri(URI.create(url));
            }
            Alts.status(0, "Sign in in the browser that opened", "");
            // the browser's answer: the one with this sign-in's state (it asks for its icon too, and anything on this
            // computer may knock: those are answered and the real one is waited for)
            long deadline = System.currentTimeMillis() + WAIT_MS;
            while (got == null) {
                if (System.currentTimeMillis() > deadline) {
                    throw new Failed("No answer from the browser: try again");
                }
                if (attempt != mine) {
                    throw new Failed("");
                }
                for (ServerSocket p : ports) {
                    Socket s;
                    try {
                        s = p.accept();
                    } catch (SocketTimeoutException ignored) {
                        continue;
                    } catch (java.io.IOException e) {
                        // closed by the next sign-in
                        if (attempt != mine) {
                            throw new Failed("");
                        }
                        throw e;
                    }
                    OPEN.add(s);
                    try {
                        s.setSoTimeout(5000);
                        Map<String, String> query = query(readHead(s.getInputStream()));
                        if (!query.containsKey("code") && !query.containsKey("error")) {
                            answer(s, false, "");
                        } else if (!state.equals(query.get("state"))) {
                            answer(s, false, "Something else answered: try again from the game.");
                        } else {
                            got = s;
                            q = query;
                            break;
                        }
                    } catch (Exception e) {
                        // a request broken off or that makes no sense: the next is waited for
                    } finally {
                        if (got != s) {
                            shut(s);
                            OPEN.remove(s);
                        }
                    }
                }
            }
            if (q.containsKey("error")) {
                boolean cancelled = "access_denied".equals(q.get("error"));
                answer(got, false, cancelled ? "The sign-in was cancelled." : q.getOrDefault("error_description", q.get("error")));
                throw cancelled ? new Failed("The sign-in was cancelled") : new Failed("Microsoft did not let in", q.get("error"));
            }
            answer(got, true, "Go back to the game: the account is being added.");
            Alts.status(0, "Adding the account", "");
            JsonObject v = post(TOKEN, "application/x-www-form-urlencoded", form(Map.of("client_id", CLIENT_ID, "grant_type", "authorization_code",
                    "code", q.get("code"), "redirect_uri", REDIRECT, "code_verifier", verifier, "scope", SCOPE)));
            if (!v.has("access_token") || !v.has("refresh_token")) {
                throw new Failed("Microsoft did not give the tokens", str(v, "error"));
            }
            User user = minecraft(str(v, "access_token"));
            String wrong = Native.guiCall("alts\tmsadd\t" + user.getName() + "\t" + user.getProfileId().toString().replace("-", "") + "\t" + str(v, "refresh_token"));
            if (wrong != null && !wrong.isEmpty()) {
                throw new Failed(wrong);
            }
            return user;
        } finally {
            ports.forEach(Msa::shut);
            OPEN.removeAll(ports);
            if (got != null) {
                shut(got);
                OPEN.remove(got);
            }
        }
    }

    /**
     * Both of "localhost": the browser may take either. A port still held a moment (the sign-in before letting go) is
     * tried again for 2 s; a family this computer has not is passed.
     */
    private static List<ServerSocket> listen() throws Failed {
        List<ServerSocket> out = new ArrayList<>();
        long until = System.currentTimeMillis() + 2000;
        for (String at : new String[] {"127.0.0.1", "::1"}) {
            while (true) {
                ServerSocket s = null;
                try {
                    s = new ServerSocket();
                    s.bind(new InetSocketAddress(InetAddress.getByName(at), PORT));
                    s.setSoTimeout(60);
                    out.add(s);
                    break;
                } catch (java.net.BindException e) {
                    shut(s);
                    if (System.currentTimeMillis() > until) {
                        break;
                    }
                    try {
                        Thread.sleep(100);
                    } catch (InterruptedException ie) {
                        Thread.currentThread().interrupt();
                        break;
                    }
                } catch (Exception e) {
                    shut(s);
                    break;
                }
            }
        }
        if (out.isEmpty()) {
            throw new Failed("The port for Microsoft's answer is taken by another program", String.valueOf(PORT));
        }
        return out;
    }

    private static void shut(@Nullable Closeable c) {
        if (c == null) {
            return;
        }
        try {
            c.close();
        } catch (Exception ignored) {
            // closing
        }
    }

    private static String readHead(InputStream in) throws Exception {
        byte[] buf = new byte[8192];
        int n = 0;
        while (n < buf.length) {
            int r = in.read(buf, n, buf.length - n);
            if (r <= 0) {
                break;
            }
            n += r;
            if (new String(buf, 0, n, StandardCharsets.ISO_8859_1).contains("\r\n\r\n")) {
                break;
            }
        }
        return new String(buf, 0, n, StandardCharsets.ISO_8859_1);
    }

    /** The query of a request's first line ("GET /?code=...&state=... HTTP/1.1"). */
    static Map<String, String> query(String request) {
        String line = request.lines().findFirst().orElse("");
        String[] parts = line.split(" ");
        String path = parts.length > 1 ? parts[1] : "";
        Map<String, String> q = new HashMap<>();
        int at = path.indexOf('?');
        if (at >= 0) {
            for (String pair : path.substring(at + 1).split("&")) {
                int eq = pair.indexOf('=');
                if (eq > 0) {
                    q.put(pair.substring(0, eq), URLDecoder.decode(pair.substring(eq + 1), StandardCharsets.UTF_8));
                }
            }
        }
        return q;
    }

    /** The page the browser shows when the answer has come: the client's look (monochrome, quiet). */
    private static void answer(Socket s, boolean ok, String words) {
        String title = word(ok ? "Signed in" : "Not signed in");
        words = word(words);
        String body = "<!doctype html><html><head><meta charset=\"utf-8\"><title>Cursed Recode</title><style>"
                + "body{margin:0;height:100vh;display:flex;align-items:center;justify-content:center;background:#060607;color:#9a9aa2;font:16px system-ui,sans-serif}"
                + "div{text-align:center}b{display:block;font-size:22px;letter-spacing:.14em;color:#ececec;margin-bottom:10px;text-transform:uppercase}"
                + "</style></head><body><div><b>" + title + "</b>" + escape(words) + "</div></body></html>";
        byte[] b = body.getBytes(StandardCharsets.UTF_8);
        try (s; OutputStream out = s.getOutputStream()) {
            out.write(("HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: " + b.length + "\r\nConnection: close\r\n\r\n")
                    .getBytes(StandardCharsets.ISO_8859_1));
            out.write(b);
        } catch (Exception ignored) {
            // the browser went
        }
    }

    // ------------------------------------------------------------------------------------------ the chain

    /** Microsoft's token → the game's session (its name, id, token), kept till its token runs out. */
    private static User minecraft(String access) throws Exception {
        JsonObject xbl = post("https://user.auth.xboxlive.com/user/authenticate", "application/json",
                "{\"Properties\":{\"AuthMethod\":\"RPS\",\"SiteName\":\"user.auth.xboxlive.com\",\"RpsTicket\":\"d=" + access
                        + "\"},\"RelyingParty\":\"http://auth.xboxlive.com\",\"TokenType\":\"JWT\"}");
        String xblToken = str(xbl, "Token");
        String uhs = "";
        try {
            uhs = xbl.getAsJsonObject("DisplayClaims").getAsJsonArray("xui").get(0).getAsJsonObject().get("uhs").getAsString();
        } catch (RuntimeException ignored) {
            // said below
        }
        if (xblToken.isEmpty() || uhs.isEmpty()) {
            throw new Failed("Xbox Live did not let in", str(xbl, "status"));
        }
        JsonObject xsts = post("https://xsts.auth.xboxlive.com/xsts/authorize", "application/json",
                "{\"Properties\":{\"SandboxId\":\"RETAIL\",\"UserTokens\":[\"" + xblToken + "\"]},\"RelyingParty\":\"rp://api.minecraftservices.com/\",\"TokenType\":\"JWT\"}");
        String xstsToken = str(xsts, "Token");
        if (xstsToken.isEmpty()) {
            long err = xsts.has("XErr") ? xsts.get("XErr").getAsLong() : 0L;
            throw new Failed(err == 2148916233L ? "This Microsoft account has no Xbox profile: make one at xbox.com and try again"
                    : err == 2148916235L ? "Xbox Live is not available in this account's country"
                    : err == 2148916236L || err == 2148916237L ? "This account has to be verified as an adult's at xbox.com first"
                    : err == 2148916238L ? "A child's account: an adult has to add it to a family at xbox.com first"
                    : "Xbox Live did not let in", err == 0 ? str(xsts, "status") : String.valueOf(err));
        }
        JsonObject mcv = post("https://api.minecraftservices.com/authentication/login_with_xbox", "application/json",
                "{\"identityToken\":\"XBL3.0 x=" + uhs + ";" + xstsToken + "\"}");
        String token = str(mcv, "access_token");
        if (token.isEmpty()) {
            // a new app is let into Minecraft's services only once Mojang has allowed it
            throw "403".equals(str(mcv, "status")) ? new Failed("Minecraft's services do not let the client in yet: the app waits for Mojang's permission")
                    : new Failed("Minecraft's services did not let in", str(mcv, "status"));
        }
        long lasts = mcv.has("expires_in") ? mcv.get("expires_in").getAsLong() : 86_400L;
        HttpResponse<String> r = HTTP.send(HttpRequest.newBuilder(URI.create("https://api.minecraftservices.com/minecraft/profile"))
                .header("Authorization", "Bearer " + token).header("Accept", "application/json").timeout(Duration.ofSeconds(20)).GET().build(),
                HttpResponse.BodyHandlers.ofString());
        JsonObject profile = json(r.body());
        String id = str(profile, "id"), name = str(profile, "name");
        if (id.isEmpty() || name.isEmpty()) {
            throw r.statusCode() == 404 ? new Failed("This Microsoft account has no Minecraft: Java Edition") : new Failed("The profile could not be read", String.valueOf(r.statusCode()));
        }
        User user = new User(name, com.mojang.util.UndashedUuid.fromStringLenient(id), token, Optional.empty(), Optional.empty());
        KEPT.put(name.toLowerCase(java.util.Locale.ROOT), new Kept(user, System.nanoTime() + Math.max(60L, lasts - 120L) * 1_000_000_000L));
        return user;
    }

    /** A request's answer as JSON (its status kept under "status"). */
    private static JsonObject post(String url, String type, String body) throws Exception {
        HttpResponse<String> r = HTTP.send(HttpRequest.newBuilder(URI.create(url)).header("Content-Type", type).header("Accept", "application/json")
                .timeout(Duration.ofSeconds(20)).POST(HttpRequest.BodyPublishers.ofString(body)).build(), HttpResponse.BodyHandlers.ofString());
        JsonObject o = json(r.body());
        o.addProperty("status", String.valueOf(r.statusCode()));
        return o;
    }

    private static JsonObject json(String s) {
        try {
            return JsonParser.parseString(s).getAsJsonObject();
        } catch (RuntimeException e) {
            return new JsonObject();
        }
    }

    private static String str(JsonObject o, String k) {
        return o.has(k) && o.get(k).isJsonPrimitive() ? o.get(k).getAsString() : "";
    }

    private static String form(Map<String, String> pairs) {
        StringBuilder b = new StringBuilder();
        for (var e : pairs.entrySet()) {
            if (!b.isEmpty()) {
                b.append('&');
            }
            b.append(e.getKey()).append('=').append(enc(e.getValue()));
        }
        return b.toString();
    }

    private static String enc(String s) {
        return URLEncoder.encode(s, StandardCharsets.UTF_8).replace("+", "%20");
    }

    /** A word in the game's language (the menus' own), as it is when they have none. */
    private static String word(String key) {
        if (key == null || key.isEmpty()) {
            return "";
        }
        String w = Native.guiCall("alts\tword\t" + key);
        return w == null || w.isEmpty() ? key : w;
    }

    private static String escape(String s) {
        return s == null ? "" : s.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;");
    }

    private static byte[] bytes(int n) {
        byte[] b = new byte[n];
        RANDOM.nextBytes(b);
        return b;
    }

    static String b64url(byte[] b) {
        return Base64.getUrlEncoder().withoutPadding().encodeToString(b);
    }
}
