package cursed.recode.hud;

import cursed.recode.mixin.client.ChatScreenAccess;
import cursed.recode.mixin.client.CommandSuggestionsAccess;
import cursed.recode.mixin.client.EditBoxAccess;
import cursed.recode.mixin.client.SuggestionsListAccess;
import cursed.recode.render.Native;
import java.time.LocalTime;
import java.util.ArrayList;
import java.util.List;
import java.util.Optional;
import java.util.regex.Matcher;
import java.util.regex.Pattern;
import net.minecraft.ChatFormatting;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.components.EditBox;
import net.minecraft.client.gui.components.PlayerFaceExtractor;
import net.minecraft.client.gui.screens.ChatScreen;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.multiplayer.PlayerInfo;
import net.minecraft.client.multiplayer.chat.GuiMessageSource;
import net.minecraft.network.chat.Component;
import net.minecraft.network.chat.FontDescription;
import net.minecraft.network.chat.FormattedText;
import net.minecraft.network.chat.HoverEvent;
import net.minecraft.network.chat.Style;
import net.minecraft.network.chat.TextColor;
import net.minecraft.network.chat.contents.TranslatableContents;
import net.minecraft.util.ARGB;
import net.minecraft.util.FormattedCharSequence;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import org.jspecify.annotations.Nullable;

/**
 * The client's own chat (native gui/hud/hudnchat.rs; Custom Chat's "Client text"). Each message the game's chat gets is
 * sent to the native as its runs — its words, colours, bold and the like, what can be clicked, what a hover says — and
 * the native draws it in the client's type; what the client cannot draw (a server's pack glyphs, faces, items) is drawn
 * here where it says. The game's chat screen stays the input (its field, its history, the server's command hints)
 * but draws nothing: the native draws the field and the hints from what is sent here each frame, and what is clicked
 * comes back here to be done as the game does it.
 */
public final class NChat {
    private NChat() {
    }

    private static boolean on;
    private static int nextId = 1;
    /** The server and you as last told to the native (whose formats it reads, your name lit). */
    private static String lastServer = "";
    /** The players in the game as last told to the native (only they have a face), counted frames. */
    private static String lastOnline = "";
    private static int onlineTick;
    /** The chat's search (Ctrl+F): the field is its words while it is on; what was typed is kept aside. */
    private static boolean finding;
    private static String stash = "";
    /** The faces the native's chat asks for this frame (icon 23 by their place), fetched once a frame. */
    private static String[] faces = new String[0];
    private static boolean facesStale = true;
    /** The items a card shows (icon 24). */
    private static final ItemStack[] CARD_ITEMS = {
        new ItemStack(Items.BEACON), new ItemStack(Items.CHEST), new ItemStack(Items.IRON_PICKAXE), new ItemStack(Items.TOTEM_OF_UNDYING),
        new ItemStack(Items.EMERALD), new ItemStack(Items.IRON_SWORD), new ItemStack(Items.COMPASS), new ItemStack(Items.ENDER_PEARL), new ItemStack(Items.CLOCK),
    };
    /** What a message's runs were (their styles and words: clicks, glyphs, items), by its id's key. */
    private static final Entry[] ENTRIES = new Entry[32768];
    private static String lastIn = "";

    private record Entry(int id, List<Style> styles, List<String> texts, @Nullable PlayerInfo from) {
    }

    private static final Pattern IN = Pattern.compile("^\\[?\\s*([A-Za-z0-9_]{2,16})\\s*(?:->|→|➡|⇨)\\s*(?:я|Я|me|Me|ME|мне|вам)\\s*\\]?");
    private static final Pattern OUT = Pattern.compile("^\\[?\\s*(?:я|Я|me|Me|ME|вы|Вы)\\s*(?:->|→|➡|⇨)\\s*([A-Za-z0-9_]{2,16})\\s*\\]?");

    public static boolean on() {
        return on;
    }

    /** From the HUD's state (Native.hudState: 80). */
    static void state(float[] s) {
        boolean was = on;
        on = s[80] > 0.5F;
        if (on != was) {
            org.slf4j.LoggerFactory.getLogger("NChat").info("[chat] the client's own chat {}", on ? "on" : "off");
        }
    }

    // ------------------------------------------------------------------------------------------ messages

    /** A message the game's chat gets (ChatComponentMixin): its runs to the native. */
    public static void add(Component contents, @Nullable GuiMessageSource source) {
        try {
            send(contents, source);
        } catch (Throwable t) {
            org.slf4j.LoggerFactory.getLogger("NChat").warn("a chat message not sent to the client's chat", t);
        }
    }

    private static void send(Component contents, @Nullable GuiMessageSource source) {
        Minecraft mc = Minecraft.getInstance();
        int id = nextId++;
        List<Style> styles = new ArrayList<>();
        List<String> texts = new ArrayList<>();
        StringBuilder runs = new StringBuilder();
        // (a server's colours written into the text itself read as the game reads them: HudText.pieces)
        HudText.pieces(contents, (style, text) -> {
            boolean ownFont = !style.getFont().equals(FontDescription.DEFAULT);
            // the words cut where a pack's glyphs begin and end (those Java draws)
            StringBuilder cur = new StringBuilder();
            boolean curGlyph = ownFont;
            for (int i = 0; i < text.length(); ) {
                int cp = text.codePointAt(i);
                boolean g = ownFont || glyph(cp);
                if (g != curGlyph && !cur.isEmpty()) {
                    run(runs, styles, texts, cur.toString(), style, curGlyph, mc);
                    cur.setLength(0);
                }
                curGlyph = g;
                cur.appendCodePoint(cp);
                i += Character.charCount(cp);
            }
            if (!cur.isEmpty()) {
                run(runs, styles, texts, cur.toString(), style, curGlyph, mc);
            }
        });
        // whose it is, private or not, naming you
        String plain = ChatFormatting.stripFormatting(contents.getString());
        if (plain == null) {
            plain = "";
        }
        String me = mc.getUser().getName();
        int kind = 0;
        String sender = "";
        PlayerInfo from = null;
        if (contents.getContents() instanceof TranslatableContents tc && tc.getKey().startsWith("commands.message.display.")) {
            kind = tc.getKey().endsWith("incoming") ? 2 : 3;
            Object[] args = tc.getArgs();
            if (args.length > 0) {
                sender = args[0] instanceof Component c ? c.getString() : String.valueOf(args[0]);
            }
        } else {
            Matcher mi = IN.matcher(plain);
            Matcher mo = OUT.matcher(plain);
            if (mi.find()) {
                kind = 2;
                sender = mi.group(1);
            } else if (mo.find()) {
                kind = 3;
                sender = mo.group(1);
            }
        }
        var conn = mc.getConnection();
        if (conn != null) {
            String head = plain.length() > 64 ? plain.substring(0, 64) : plain;
            int best = Integer.MAX_VALUE;
            for (PlayerInfo info : conn.getOnlinePlayers()) {
                String name = info.getProfile().name();
                if (name == null || name.length() < 3) {
                    continue;
                }
                if (!sender.isEmpty()) {
                    if (name.equalsIgnoreCase(sender)) {
                        from = info;
                    }
                    continue;
                }
                int i = HudChat.word(head, name, false);
                if (i >= 0 && !HudChat.sender(head, i, name.length())) {
                    i = -1;
                }
                if (i >= 0 && i < best) {
                    best = i;
                    from = info;
                }
            }
        }
        if (sender.isEmpty() && from != null) {
            sender = from.getProfile().name();
            kind = 1;
        }
        boolean fromMe = sender.equalsIgnoreCase(me) && kind != 2;
        boolean mention = !fromMe && source != GuiMessageSource.SYSTEM_CLIENT && HudChat.word(plain, me, true) >= 0;
        ENTRIES[id & 32767] = new Entry(id, styles, texts, from);
        String payload = id + "\u0001" + LocalTime.now().toSecondOfDay() + "\u0001" + kind + "\u0001" + (mention ? 1 : 0) + "\u0001" + clean(sender) + "\u0001" + runs;
        Native.guiCall("chatadd\t" + payload);
    }

    private static void run(StringBuilder runs, List<Style> styles, List<String> texts, String text, Style style, boolean glyph, Minecraft mc) {
        if (styles.size() >= 255) {
            return;
        }
        int fl = 0;
        if (style.isBold()) {
            fl |= 1;
        }
        if (style.isItalic()) {
            fl |= 2;
        }
        if (style.isUnderlined()) {
            fl |= 4;
        }
        if (style.isStrikethrough()) {
            fl |= 8;
        }
        if (style.isObfuscated()) {
            fl |= 16;
        }
        float gw = 0.0F;
        if (glyph) {
            fl |= 32;
            gw = mc.font.width(FormattedText.of(text, style));
        }
        if (style.getClickEvent() != null) {
            fl |= 64;
        }
        String hover = "";
        HoverEvent he = style.getHoverEvent();
        if (he instanceof HoverEvent.ShowText st) {
            hover = st.value().getString();
            if (hover.length() > 200) {
                hover = hover.substring(0, 200);
            }
        } else if (he instanceof HoverEvent.ShowItem) {
            fl |= 128;
        }
        TextColor col = style.getColor();
        if (col == null) {
            fl |= 256;
        }
        if (!runs.isEmpty()) {
            runs.append('\u0002');
        }
        runs.append(clean(text)).append('\u0003').append(String.format("%06X", col != null ? col.getValue() & 0xFFFFFF : 0xFFFFFF))
                .append('\u0003').append(fl).append('\u0003').append(gw).append('\u0003').append(clean(hover));
        styles.add(style);
        texts.add(text);
    }

    /** Letters a server's pack draws (its private ones, and those packs commonly take over: Yi, Vai). */
    private static boolean glyph(int cp) {
        return cp >= 0xE000 && cp <= 0xF8FF || cp >= 0xF0000 || cp >= 0xA000 && cp <= 0xA63F;
    }

    private static String clean(String s) {
        StringBuilder b = new StringBuilder(s.length());
        for (int i = 0; i < s.length(); i++) {
            char c = s.charAt(i);
            b.append(c == '\t' || c == '\u0001' || c == '\u0002' || c == '\u0003' || c == '\r' ? ' ' : c);
        }
        return b.toString();
    }

    public static void cleared() {
        Native.guiCall("chatclear");
    }

    // ------------------------------------------------------------------------------------------ each frame

    /** The field, the hints and the pointer to the native (before the HUD's frame); what it asked done. */
    public static void frame(Minecraft mc) {
        if (!on) {
            return;
        }
        Screen s = mc.gui.screen();
        var sd = mc.getCurrentServer();
        // CURSED_DEV_CHATSERVER: the server the chat reads the formats of (the stand's world is no server)
        String ip = cursed.recode.dev.Dev.on("CURSED_DEV_CHATSERVER") ? System.getenv("CURSED_DEV_CHATSERVER") : sd != null ? sd.ip : "";
        String srv = clean(ip) + "\t" + clean(mc.getUser().getName());
        if (!srv.equals(lastServer)) {
            lastServer = srv;
            Native.guiCall("chatserver\t" + srv);
        }
        facesStale = true;
        // who is in the game (only they have a face), said again when it changes, looked at twice a second
        if (++onlineTick % 30 == 0) {
            var conn = mc.getConnection();
            StringBuilder on = new StringBuilder();
            if (conn != null) {
                for (PlayerInfo info : conn.getOnlinePlayers()) {
                    String name = info.getProfile().name();
                    if (name != null && !name.isEmpty()) {
                        on.append(name).append('\t');
                    }
                }
            }
            String now = on.toString();
            if (!now.equals(lastOnline)) {
                lastOnline = now;
                Native.guiCall("chatonline\t" + now);
            }
        }
        if (finding && !(s instanceof ChatScreen)) {
            finding = false;
            stash = "";
            Native.guiCall("chatfind\t0");
        }
        StringBuilder b = new StringBuilder(256);
        if (s instanceof ChatScreen cs) {
            var acc = (ChatScreenAccess) cs;
            EditBox in = acc.cursed$input();
            String ghost = ((EditBoxAccess) in).cursed$suggestion();
            b.append('1').append('\u0001').append(clean(in.getValue())).append('\u0001').append(in.getCursorPosition()).append('\u0001')
                    .append(((EditBoxAccess) in).cursed$highlight()).append('\u0001').append(ghost == null ? "" : clean(ghost)).append('\u0001')
                    .append(clean(mc.getUser().getName())).append('\u0001');
            var sug = acc.cursed$suggestions();
            var list = sug == null ? null : ((CommandSuggestionsAccess) sug).cursed$list();
            int sel = -1, off = 0;
            if (list != null) {
                var la = (SuggestionsListAccess) (Object) list;
                var all = la.cursed$all();
                for (int i = 0; i < all.size() && i < 64; i++) {
                    if (i > 0) {
                        b.append('\u0002');
                    }
                    b.append(clean(all.get(i).getText()));
                }
                sel = la.cursed$current();
                off = la.cursed$offset();
            }
            b.append('\u0001').append(sel).append('\u0001').append(off).append('\u0001');
            if (sug != null && list == null) {
                List<FormattedCharSequence> usage = ((CommandSuggestionsAccess) sug).cursed$usage();
                for (int i = 0; i < usage.size() && i < 8; i++) {
                    if (i > 0) {
                        b.append('\u0002');
                    }
                    StringBuilder u = new StringBuilder();
                    usage.get(i).accept((pos, style, cp) -> {
                        u.appendCodePoint(cp);
                        return true;
                    });
                    b.append(clean(u.toString()));
                }
            }
            b.append('\u0001').append((float) mc.mouseHandler.xpos()).append('\u0001').append((float) mc.mouseHandler.ypos());
        } else {
            b.append('0');
        }
        String now = b.toString();
        if (!now.equals(lastIn)) {
            lastIn = now;
            Native.guiCall("chatin\t" + now);
        }
        if (s instanceof ChatScreen) {
            cursed.recode.gui.GuiAsks.poll(mc);
        }
    }

    /** What the native asked (GuiAsks "chat"): a run clicked, the field set or added to, a hint used. */
    public static void act(String arg) {
        Minecraft mc = Minecraft.getInstance();
        if (!(mc.gui.screen() instanceof ChatScreen cs)) {
            return;
        }
        var acc = (ChatScreenAccess) cs;
        String[] f = arg.split("\t");
        try {
            switch (f[0]) {
                case "click" -> {
                    int id = Integer.parseInt(f[1]), run = Integer.parseInt(f[2]);
                    Entry e = ENTRIES[id & 32767];
                    if (e != null && e.id == id && run >= 0 && run < e.styles.size()) {
                        acc.cursed$clicked(e.styles.get(run), mc.hasShiftDown());
                    }
                }
                case "set" -> {
                    EditBox in = acc.cursed$input();
                    in.setValue(f.length > 1 ? f[1] : "");
                    in.moveCursorToEnd(false);
                }
                case "insert" -> acc.cursed$input().insertText(f.length > 1 ? f[1] : "");
                case "cursor" -> {
                    // the field pressed: the caret where it was pressed, nothing selected
                    EditBox in = acc.cursed$input();
                    in.moveCursorTo(Math.max(0, Math.min(in.getValue().length(), Integer.parseInt(f[1]))), false);
                }
                case "send" -> {
                    // a card's answer or the player's card's teleport: the command as the player would type it
                    String cmd = f.length > 1 ? f[1] : "";
                    var conn = mc.getConnection();
                    if (conn != null && !cmd.isBlank()) {
                        if (cmd.startsWith("/")) {
                            conn.sendCommand(cmd.substring(1));
                        } else {
                            conn.sendChat(cmd);
                        }
                    }
                }
                case "find" -> find(cs, !finding);
                case "sugg" -> {
                    var sug = acc.cursed$suggestions();
                    var list = sug == null ? null : ((CommandSuggestionsAccess) sug).cursed$list();
                    if (list != null) {
                        list.select(Integer.parseInt(f[1]));
                        list.useSuggestion();
                    }
                }
                default -> {
                }
            }
        } catch (RuntimeException ignored) {
            // a stale line: nothing done
        }
    }

    /**
     * Enter in a conversation (the native's Private tab, one open): the field holds only what is said, it goes to them
     * as a private message (the history keeps it), the chat stays open on them. A command typed is the game's.
     */
    public static boolean say(ChatScreen cs) {
        EditBox in = ((ChatScreenAccess) cs).cursed$input();
        String words = in.getValue().trim();
        if (!on() || finding || words.isEmpty() || words.startsWith("/")) {
            return false;
        }
        String to = Native.guiCall("chatto");
        if (to == null || to.isEmpty()) {
            return false;
        }
        // the server's line is 256 at most, the command's words included
        String head = "/msg " + to + " ";
        int room = 256 - head.length();
        if (words.length() > room) {
            words = words.substring(0, room);
        }
        cs.handleChatInput(head + words, true);
        in.setValue("");
        Native.guiCall("chatsent\t" + to);
        return true;
    }

    /** The search on or off: the words typed kept aside while the field is the search's, back after. */
    public static void find(ChatScreen cs, boolean want) {
        if (want == finding) {
            return;
        }
        EditBox in = ((ChatScreenAccess) cs).cursed$input();
        if (want) {
            stash = in.getValue();
            in.setValue("");
        } else {
            in.setValue(stash);
            in.moveCursorToEnd(false);
            stash = "";
        }
        finding = want;
        Native.guiCall("chatfind\t" + (want ? "1" : "0"));
    }

    public static boolean finding() {
        return finding;
    }

    /** The HUD's icons of the chat (HudLayer): 20 a message's sender's face, 21 a pack's glyphs, 22 an item a message
     *  shows, 23 a face by a name, 24 a card's item (GUI units). */
    /**
     * A player's face (its hat over it) at x, y, `size` GUI px, its corners rounded by `round` of its side (0 square,
     * 0.5 a circle). The GUI's scissor is whole GUI px, too coarse for a face's corners, so the face is drawn in the
     * window's own px as bands: the rows by the corners one px each, cut to the round, the middle at once; each band
     * reads its own part of the skin.
     */
    static void face(GuiGraphicsExtractor g, net.minecraft.resources.Identifier tex, float x, float y, float size, float round, float alpha) {
        float gs = (float) Minecraft.getInstance().getWindow().getGuiScale();
        int s = Math.max(1, Math.round(size * gs));
        int col = ARGB.white(alpha);
        var pose = g.pose();
        pose.pushMatrix();
        pose.translate(x, y);
        pose.scale(1.0F / gs, 1.0F / gs);
        float r = Math.min(round, 0.5F) * s;
        int edge = (int) Math.ceil(r);
        // the skin read in whole units, four a screen px (its head's 8 px are the face's s): every band reads the same
        // place for the same px, an eighth of a px off a skin pixel's edge (no px's middle falls right on one)
        int q = 4;
        int t = 8 * s * q;
        float bias = 0.5F;
        int y0 = 0;
        while (y0 < s) {
            // a corner's rows: as wide as the round lets each (those as wide as the one before go with it); the
            // middle: all of it, down to the lower corners
            int y1;
            int in;
            if (y0 < edge || y0 >= s - edge) {
                in = faceInset(y0, s, r, edge);
                y1 = y0 + 1;
                while (y1 < s && (y1 < edge || y1 >= s - edge) && faceInset(y1, s, r, edge) == in) {
                    y1++;
                }
            } else {
                y1 = s - edge;
                in = 0;
            }
            int bw = s - 2 * in;
            if (bw > 0) {
                float v = s * q + y0 * q + bias;
                g.blit(net.minecraft.client.renderer.RenderPipelines.GUI_TEXTURED, tex, in, y0, s * q + in * q + bias, v, bw, y1 - y0, bw * q, (y1 - y0) * q, t, t, col);
                g.blit(net.minecraft.client.renderer.RenderPipelines.GUI_TEXTURED, tex, in, y0, 5 * s * q + in * q + bias, v, bw, y1 - y0, bw * q, (y1 - y0) * q, t, t, col);
            }
            y0 = y1;
        }
        pose.popMatrix();
    }

    /** How far in from the side row `y` of a face `s` px is cut by its corners' round `r` (px). */
    private static int faceInset(int y, int s, float r, int edge) {
        float dy = Math.max(0.0F, r - (y < edge ? y + 0.5F : s - y - 0.5F));
        return Math.round(r - (float) Math.sqrt(Math.max(0.0F, r * r - dy * dy)));
    }

    static void icon(GuiGraphicsExtractor graphics, int kind, int index, float x, float y, float w, float h, float alpha) {
        Minecraft mc = Minecraft.getInstance();
        if (kind == 23 || kind == 26 || kind == 27) {
            if (facesStale) {
                String all = Native.guiCall("chatfaces");
                faces = all == null || all.isEmpty() ? new String[0] : all.split("\n");
                facesStale = false;
            }
            var conn = mc.getConnection();
            if (index >= 0 && index < faces.length && conn != null) {
                PlayerInfo info = conn.getPlayerInfoIgnoreCase(faces[index]);
                if (info != null) {
                    face(graphics, info.getSkin().body().texturePath(), x, y, h, kind == 26 ? 0.24F : kind == 27 ? 0.5F : 0.0F, alpha);
                }
            }
            return;
        }
        if (kind == 24) {
            if (index >= 0 && index < CARD_ITEMS.length) {
                HudInv.item(graphics, CARD_ITEMS[index], x, y, w, 1.0F, index);
            }
            return;
        }
        Entry e = ENTRIES[(index >> 8) & 32767];
        if (e == null) {
            return;
        }
        int run = index & 255;
        switch (kind) {
            case 20 -> {
                if (e.from != null) {
                    int s = Math.max(1, Math.round(h));
                    PlayerFaceExtractor.extractRenderState(graphics, e.from.getSkin(), Math.round(x), Math.round(y), s);
                }
            }
            case 21 -> {
                if (run < e.styles.size()) {
                    var pose = graphics.pose();
                    pose.pushMatrix();
                    pose.translate(x, y);
                    float k = h / 9.0F;
                    pose.scale(k, k);
                    graphics.text(mc.font, FormattedCharSequence.forward(e.texts.get(run), e.styles.get(run)), 0, 0, ARGB.color(Math.round(alpha * 255.0F), 0xFFFFFF), false);
                    pose.popMatrix();
                }
            }
            case 22 -> {
                if (run < e.styles.size() && e.styles.get(run).getHoverEvent() instanceof HoverEvent.ShowItem si) {
                    HudInv.item(graphics, si.item().create(), x, y, w, 1.0F, index);
                }
            }
            default -> {
            }
        }
    }

}
