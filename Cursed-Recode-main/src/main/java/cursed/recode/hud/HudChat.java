package cursed.recode.hud;

import cursed.recode.mixin.client.ChatComponentAccess;
import cursed.recode.render.Native;
import java.time.LocalTime;
import java.util.ArrayList;
import java.util.IdentityHashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.function.Consumer;
import net.minecraft.ChatFormatting;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.components.ChatComponent;
import net.minecraft.client.gui.components.PlayerFaceExtractor;
import net.minecraft.client.multiplayer.PlayerInfo;
import net.minecraft.client.multiplayer.chat.GuiMessage;
import net.minecraft.client.multiplayer.chat.GuiMessageSource;
import net.minecraft.client.multiplayer.chat.GuiMessageTag;
import net.minecraft.client.resources.sounds.SimpleSoundInstance;
import net.minecraft.network.chat.Component;
import net.minecraft.network.chat.MessageSignature;
import net.minecraft.sounds.SoundEvents;
import net.minecraft.util.ARGB;
import net.minecraft.util.FormattedCharSequence;
import net.minecraft.util.Mth;
import org.joml.Matrix3x2f;
import org.jspecify.annotations.Nullable;

/**
 * The Custom Chat (Interface; native gui/hudchat.rs). The game writes the messages as ever (a server's glyphs, colours,
 * clicks and hovers stay), the client decides the rest: where the chat is (an element of the HUD, dragged in the
 * editor), how wide, how many lines, how big; new lines rise into place and the old ones fade when their time is up,
 * the wheel scrolls smoothly; before a message its time and the face of the player it is from; the same message again
 * counts up (×2) instead of another line; a message naming you is lit (and rings); a right click copies a message. The
 * glass behind, the times, the highlights and the scroll bar are the HUD's (it is drawn under the chat): each frame it
 * gets the lines' places from here (append), and says back where the chat goes (state).
 */
public final class HudChat {
    private HudChat() {
    }

    /** One message as the Custom Chat keeps it: when it came, the time it shows, whose face, whether it names you. */
    static final class Meta {
        long at;
        int clock;
        @Nullable PlayerInfo from;
        boolean mention;
        int count = 1;
        long bumpAt;
    }

    /** A line shown this frame: moved by, how seen, the first of its message, its message's. */
    private record Shown(float dx, float alpha, boolean first, @Nullable Meta meta) {
    }

    private static boolean active;
    /** Where the chat goes (window px): its words' left edge, its last line's bottom; the room before the words. */
    private static float originX, originY, gutter, headX;
    private static boolean heads, smooth = true, stack = true, mention = true, mentionSound = true, copy = true, fade = true, input = true, tags;
    private static int appear = 1, history = 100, lines = 10, linesOpen = 20;
    private static float speed = 1.0F, fadeAfter = 10.0F, widthPx, scaleK = 1.0F;

    private static final Map<GuiMessage, Meta> META = new IdentityHashMap<>();
    private static final Map<FormattedCharSequence, Shown> SHOWN = new IdentityHashMap<>();
    /** The lines shown (window px) and their messages, for a right click: top, bottom, left, right. */
    private static final List<float[]> RECTS = new ArrayList<>();
    private static final List<GuiMessage> RECT_OF = new ArrayList<>();
    /** How far the lines are from their places (the chat's own units, + lower): it eases to 0. */
    private static float slide;
    private static long last;
    public static boolean adding;
    private static boolean queueing, again;
    private static int before, scrollBefore;
    private static @Nullable Meta bumped;
    /** The same message again: the last one's text and message. */
    private static String lastText = "";
    private static @Nullable GuiMessage lastMessage;
    private static float lastWidth = -1.0F, lastScale = -1.0F;
    /** When the chat was opened (the lines it had hidden come in), and whether it is now. */
    private static long openAt;
    private static boolean wasOpen;

    public static boolean active() {
        return active;
    }

    /** From the HUD's state (Native.hudState: from 3 on). */
    static void state(float[] s) {
        active = s[3] > 0.5F;
        originX = s[4];
        originY = s[5];
        gutter = s[6];
        headX = s[7];
        heads = s[8] > 0.5F;
        appear = (int) s[9];
        speed = Math.max(0.25F, s[10]);
        smooth = s[11] > 0.5F;
        stack = s[12] > 0.5F;
        fade = s[13] > 0.5F;
        fadeAfter = s[14];
        history = Math.max(100, (int) s[15]);
        mention = s[16] > 0.5F;
        mentionSound = s[17] > 0.5F;
        copy = s[18] > 0.5F;
        widthPx = s[19];
        lines = Math.max(1, (int) s[20]);
        linesOpen = Math.max(1, (int) s[21]);
        scaleK = s[22] > 0.0F ? s[22] : 1.0F;
        input = s[23] > 0.5F;
        tags = s[24] > 0.5F;
        // a new width or size: the messages are broken into lines again
        float w = active ? widthPx : -2.0F, k = active ? scaleK : -2.0F;
        if (w != lastWidth || k != lastScale) {
            boolean was = lastWidth != -1.0F;
            lastWidth = w;
            lastScale = k;
            Minecraft mc = Minecraft.getInstance();
            if (was && mc.gui != null && mc.gui.hud != null) {
                mc.gui.hud.getChat().rescaleChat();
            }
        }
    }

    /** The chat's input field is drawn by the HUD (its plate). */
    public static boolean ownInput() {
        return active && input;
    }

    // ------------------------------------------------------------------------------------------ the game's chat asks

    public static int width() {
        if (!active || widthPx <= 0.0F) {
            return 0;
        }
        return Math.max(40, Math.round(widthPx / (float) Minecraft.getInstance().getWindow().getGuiScale()));
    }

    public static int height(ChatComponent chat) {
        if (!active) {
            return 0;
        }
        Minecraft mc = Minecraft.getInstance();
        int lh = (int) (9.0 * (mc.options.chatLineSpacing().get() + 1.0));
        return (chat.isChatFocused() ? linesOpen : lines) * lh;
    }

    public static double scale() {
        return active ? Minecraft.getInstance().options.chatScale().get() * scaleK : 0.0;
    }

    public static int history(int n) {
        return active ? Math.max(n, history) : n;
    }

    public static int extraLines() {
        if (!active || slide == 0.0F) {
            return 0;
        }
        int lh = lineHeight(Minecraft.getInstance());
        return (int) Math.ceil(Math.abs(slide) / lh) + 1;
    }

    private static int lineHeight(Minecraft mc) {
        return (int) (9.0 * (mc.options.chatLineSpacing().get() + 1.0));
    }

    /** The chat's scale and width (GUI px) as the game's chat has them now (its own are private). */
    private static float chatScale(Minecraft mc) {
        double s = scale();
        return (float) (s > 0.0 ? s : mc.options.chatScale().get());
    }

    private static int chatWidth(Minecraft mc) {
        int w = width();
        return w > 0 ? w : ChatComponent.getWidth(mc.options.chatWidth().get());
    }

    /** A message the same as the last (its words): the last goes, this one comes counted. True: done here. */
    public static boolean repeat(ChatComponent chat, Component contents, @Nullable MessageSignature signature, GuiMessageSource source, @Nullable GuiMessageTag tag) {
        if (again) {
            return false;
        }
        String text = contents.getString();
        if (!active || !stack) {
            lastText = text;
            lastMessage = null;
            return false;
        }
        var acc = (ChatComponentAccess) chat;
        List<GuiMessage> all = acc.cursed$messages();
        GuiMessage prev = lastMessage;
        if (prev == null || all.isEmpty() || all.getFirst() != prev || !text.equals(lastText) || text.isBlank()) {
            lastText = text;
            lastMessage = null;
            return false;
        }
        Meta old = META.get(prev);
        int count = (old == null ? 1 : old.count) + 1;
        // the last one out (its message, its lines), this one in with its count after it
        all.removeFirst();
        List<GuiMessage.Line> ls = acc.cursed$lines();
        while (!ls.isEmpty() && ls.getFirst().parent() == prev) {
            ls.removeFirst();
        }
        Meta m = new Meta();
        if (old != null) {
            m.at = old.at;
            m.clock = old.clock;
            m.from = old.from;
            m.mention = old.mention;
        }
        m.count = count;
        m.bumpAt = System.nanoTime();
        bumped = m;
        again = true;
        try {
            acc.cursed$add(contents.copy().append(Component.literal("  ×" + count).withStyle(ChatFormatting.GRAY)), signature, source, tag);
        } finally {
            again = false;
            bumped = null;
        }
        lastText = text;
        return true;
    }

    public static void queueing(ChatComponent chat, GuiMessage message) {
        queueing = true;
        before = ((ChatComponentAccess) chat).cursed$lines().size();
        if (META.containsKey(message)) {
            return;
        }
        Meta m = bumped != null ? bumped : new Meta();
        if (bumped == null) {
            m.at = adding ? System.nanoTime() : 0L;
            m.clock = LocalTime.now().toSecondOfDay();
            if (adding) {
                look(message, m);
            }
        }
        META.put(message, m);
        if (adding) {
            lastMessage = message;
        }
        // (the kept ones: as many as the chat keeps, and a margin)
        if (META.size() > history + 64) {
            var keep = new IdentityHashMap<GuiMessage, Boolean>();
            for (GuiMessage g : ((ChatComponentAccess) chat).cursed$messages()) {
                keep.put(g, true);
            }
            keep.put(message, true);
            META.keySet().removeIf(g -> !keep.containsKey(g));
        }
    }

    public static void queued(ChatComponent chat, GuiMessage message) {
        queueing = false;
        var acc = (ChatComponentAccess) chat;
        // new lines at the bottom: everything shown was a line lower a moment ago and rises into place
        if (active && adding && bumped == null && acc.cursed$scroll() == 0 && appear == 1) {
            int n = 0;
            for (GuiMessage.Line l : acc.cursed$lines()) {
                if (l.parent() != message) {
                    break;
                }
                n++;
            }
            int lh = lineHeight(Minecraft.getInstance());
            slide = Math.min(slide + n * lh, (lines + 4) * lh);
        }
    }

    public static void scrolling(ChatComponent chat) {
        scrollBefore = ((ChatComponentAccess) chat).cursed$scroll();
    }

    public static void scrolled(ChatComponent chat) {
        if (!active || !smooth || queueing) {
            return;
        }
        int d = ((ChatComponentAccess) chat).cursed$scroll() - scrollBefore;
        if (d != 0) {
            int lh = lineHeight(Minecraft.getInstance());
            slide = Mth.clamp(slide - d * lh, -(linesOpen + 2) * lh, (linesOpen + 2) * lh);
        }
    }

    public static void cleared() {
        META.clear();
        SHOWN.clear();
        slide = 0.0F;
        lastMessage = null;
        lastText = "";
    }

    /** Whose message it is (an online player's name in its first words, as a sender's is written: «<name>», «name:»,
     *  «name »», «name ⇨»…: a face before it), whether it names you (anywhere, not your own message). */
    private static void look(GuiMessage message, Meta m) {
        Minecraft mc = Minecraft.getInstance();
        String plain = ChatFormatting.stripFormatting(message.content().getString());
        if (plain == null || plain.isEmpty()) {
            return;
        }
        String head = plain.length() > 64 ? plain.substring(0, 64) : plain;
        var conn = mc.getConnection();
        int best = Integer.MAX_VALUE;
        if (conn != null) {
            for (PlayerInfo info : conn.getOnlinePlayers()) {
                String name = info.getProfile().name();
                if (name == null || name.length() < 3) {
                    continue;
                }
                int i = word(head, name, false);
                if (i >= 0 && !sender(head, i, name.length())) {
                    i = -1;
                }
                if (i >= 0 && (i < best || i == best && m.from != null && name.length() > m.from.getProfile().name().length())) {
                    best = i;
                    m.from = info;
                }
            }
        }
        String me = mc.getUser().getName();
        boolean fromMe = m.from != null && m.from.getProfile().name().equalsIgnoreCase(me);
        if (mention && !fromMe && message.source() != GuiMessageSource.SYSTEM_CLIENT && word(plain, me, true) >= 0) {
            m.mention = true;
            if (mentionSound && active) {
                mc.getSoundManager().play(SimpleSoundInstance.forUI(SoundEvents.EXPERIENCE_ORB_PICKUP, 1.7F, 0.35F));
            }
        }
    }

    /** Where `name` is in `s` as a word of its own (-1 if not). */
    static int word(String s, String name, boolean anyCase) {
        String a = anyCase ? s.toLowerCase(Locale.ROOT) : s;
        String b = anyCase ? name.toLowerCase(Locale.ROOT) : name;
        int from = 0;
        while (true) {
            int i = a.indexOf(b, from);
            if (i < 0) {
                return -1;
            }
            int j = i + b.length();
            boolean left = i == 0 || !part(a.charAt(i - 1));
            boolean right = j >= a.length() || !part(a.charAt(j));
            if (left && right) {
                return i;
            }
            from = i + 1;
        }
    }

    /** The name at `i` (`len` long) written as a sender's: after «<», or before «:», «>», «»», «→», «⇨», «|», «-». */
    static boolean sender(String s, int i, int len) {
        if (i > 0 && s.charAt(i - 1) == '<') {
            return true;
        }
        for (int j = i + len; j < Math.min(s.length(), i + len + 3); j++) {
            char c = s.charAt(j);
            if (c == ' ') {
                continue;
            }
            return ":>»→⇨|-›➥➜➤".indexOf(c) >= 0;
        }
        return false;
    }

    private static boolean part(char c) {
        return Character.isLetterOrDigit(c) || c == '_';
    }

    // ---------------------------------------------------------------------------------------------- each frame

    /** The lines' places for the HUD (gui/hudchat.rs Chat::parse): a head of 10, then 9 a line. */
    static void append(Minecraft mc, List<Double> n) {
        long now = System.nanoTime();
        float dt = last == 0L ? 0.0F : Math.min((now - last) / 1e9F, 0.1F);
        last = now;
        slide *= (float) Math.exp(-dt * 12.0 * speed);
        if (Math.abs(slide) < 0.02F) {
            slide = 0.0F;
        }
        SHOWN.clear();
        RECTS.clear();
        RECT_OF.clear();
        ChatComponent chat = mc.gui.hud.getChat();
        var acc = (ChatComponentAccess) chat;
        List<GuiMessage.Line> ls = acc.cursed$lines();
        boolean focused = chat.isChatFocused();
        if (focused && !wasOpen) {
            openAt = now;
        }
        wasOpen = focused;
        float opened = ease((now - openAt) / 1e9F / 0.22F);
        int pos = acc.cursed$scroll();
        int per = chat.getLinesPerPage();
        int lh = lineHeight(mc);
        float f = chatScale(mc);
        float gs = (float) mc.getWindow().getGuiScale();
        float unit = f * gs;
        float widthPxNow = Mth.ceil(chatWidth(mc) / f) * unit;
        int guiH = mc.getWindow().getGuiScaledHeight();
        int k = Mth.floor((guiH - 40) / f);
        int extra = extraLines();
        int count = Math.max(0, Math.min(ls.size() - pos, per + extra));
        int head = n.size();
        int total = ls.size();
        n.add(0.0);
        n.add(focused ? 1.0 : 0.0);
        n.add((double) widthPxNow);
        n.add((double) lh * unit);
        n.add((double) per * lh * unit);
        n.add(total > per ? (double) pos / Math.max(1, total - per) : 0.0);
        n.add(total > per ? (double) per / total : 1.0);
        n.add(acc.cursed$newSince() ? 1.0 : 0.0);
        n.add((double) gs);
        n.add((double) unit);
        float top0 = k - per * lh;
        int shown = 0;
        for (int idx = 0; idx < count; idx++) {
            int l = idx + pos;
            GuiMessage.Line line = ls.get(l);
            Meta m = META.get(line.parent());
            float bottom = k - idx * lh + slide;
            float top = bottom - lh;
            float a = 1.0F;
            float age = m == null || m.at == 0L ? 99.0F : (now - m.at) / 1e9F;
            float life = fade ? 1.0F - Mth.clamp((age - fadeAfter) / 0.8F, 0.0F, 1.0F) : 1.0F;
            if (!focused) {
                a = life;
            } else if (idx >= lines || life < 1.0F) {
                // the chat just opened: what it had hidden comes in
                a = Math.max(idx >= lines ? 0.0F : life, opened);
            }
            // the edges: a line leaving at the top fades out, one rising from below fades in
            a *= Mth.clamp((bottom - top0) / lh, 0.0F, 1.0F);
            a *= Mth.clamp((k + 0.001F - top) / lh, 0.0F, 1.0F);
            float dx = 0.0F;
            if (appear == 2) {
                float e = ease(age * speed / 0.4F);
                dx = -(1.0F - e) * 26.0F;
                a *= e;
            } else if (appear == 3) {
                a *= ease(age * speed / 0.35F);
            }
            boolean first = l + 1 >= ls.size() || ls.get(l + 1).parent() != line.parent();
            if (a <= 0.003F) {
                continue;
            }
            SHOWN.put(line.content(), new Shown(dx, a, first, m));
            shown++;
            int flags = (first ? 1 : 0) | (m != null && m.mention ? 2 : 0) | (first && m != null && m.from != null && heads ? 4 : 0);
            n.add((double) ((top - k) * unit));
            n.add((double) ((bottom - k) * unit));
            n.add((double) a);
            n.add((double) flags);
            n.add(m == null ? -1.0 : (double) m.clock);
            n.add((double) (dx * unit));
            n.add((double) (System.identityHashCode(line.parent()) & 0xffffff));
            n.add(m == null || m.bumpAt == 0L ? 99.0 : (now - m.bumpAt) / 1e9);
            n.add((double) (mc.font.width(line.content()) * unit));
            RECTS.add(new float[] {originY + (top - k) * unit, originY + (bottom - k) * unit, originX - gutter, originX + widthPxNow});
            RECT_OF.add(line.parent());
        }
        n.set(head, (double) shown);
    }

    private static float ease(float x) {
        float t = Mth.clamp(x, 0.0F, 1.0F);
        return 1.0F - (1.0F - t) * (1.0F - t) * (1.0F - t);
    }

    /** A right click in the open chat (window px): the message under it copied. True: taken. */
    public static boolean copyAt(float x, float y) {
        if (!active || !copy) {
            return false;
        }
        for (int i = 0; i < RECTS.size(); i++) {
            float[] r = RECTS.get(i);
            if (y >= r[0] && y < r[1] && x >= r[2] - 4.0F && x < r[3] + 8.0F) {
                Minecraft mc = Minecraft.getInstance();
                String s = ChatFormatting.stripFormatting(RECT_OF.get(i).content().getString());
                mc.keyboardHandler.setClipboard(s == null ? "" : s);
                Native.hudEvent(3, "");
                mc.getSoundManager().play(SimpleSoundInstance.forUI(SoundEvents.UI_BUTTON_CLICK.value(), 1.6F, 0.25F));
                return true;
            }
        }
        return false;
    }

    // ---------------------------------------------------------------------------------------------- the drawing

    public static ChatComponent.ChatGraphicsAccess wrap(ChatComponent.ChatGraphicsAccess inner, @Nullable GuiGraphicsExtractor graphics) {
        return active ? new Access(inner, graphics) : inner;
    }

    /** The game's chat drawn through the Custom Chat: moved where it goes, lines faded and slid, faces, no plates. */
    private static final class Access implements ChatComponent.ChatGraphicsAccess {
        private final ChatComponent.ChatGraphicsAccess inner;
        private final @Nullable GuiGraphicsExtractor graphics;
        private boolean posed;
        private final float unit;
        /** The line last drawn: its tag goes with it (as seen, as moved). */
        private float lineAlpha = 1.0F, lineDx;

        Access(ChatComponent.ChatGraphicsAccess inner, @Nullable GuiGraphicsExtractor graphics) {
            this.inner = inner;
            this.graphics = graphics;
            Minecraft mc = Minecraft.getInstance();
            this.unit = (float) (chatScale(mc) * mc.getWindow().getGuiScale());
        }

        @Override
        public void updatePose(Consumer<Matrix3x2f> updater) {
            if (this.posed) {
                this.inner.updatePose(updater);
                return;
            }
            // the first: the chat's scale and margin, after it is put where the HUD says; then how far it slides
            this.posed = true;
            Minecraft mc = Minecraft.getInstance();
            float gs = (float) mc.getWindow().getGuiScale();
            float f = chatScale(mc);
            int k = Mth.floor((mc.getWindow().getGuiScaledHeight() - 40) / f);
            float tx = originX / gs - 4.0F * f, ty = originY / gs - k * f;
            float s = slide;
            this.inner.updatePose(p -> {
                p.translate(tx, ty);
                updater.accept(p);
                p.translate(0.0F, s);
            });
        }

        @Override
        public void fill(int x0, int y0, int x1, int y1, int color) {
            // the game's plates under the lines and its scroll bar: the HUD draws its own
        }

        @Override
        public boolean handleMessage(int textTop, float opacity, FormattedCharSequence message) {
            Shown s = SHOWN.get(message);
            if (s == null) {
                // not among the lines laid out this frame (gone, or the chat not laid out yet): as the game would
                this.lineAlpha = SHOWN.isEmpty() ? 1.0F : 0.0F;
                this.lineDx = 0.0F;
                return SHOWN.isEmpty() && this.inner.handleMessage(textTop, opacity, message);
            }
            float dx = s.dx();
            this.lineAlpha = s.alpha();
            this.lineDx = dx;
            if (dx != 0.0F) {
                this.inner.updatePose(p -> p.translate(dx, 0.0F));
            }
            Meta m = s.meta();
            if (this.graphics != null && heads && s.first() && m != null && m.from != null) {
                int x = Math.round((headX - gutter) / this.unit);
                PlayerFaceExtractor.extractRenderState(this.graphics, m.from.getSkin(), x, textTop, 8, ARGB.white(opacity * s.alpha()));
            }
            boolean r = this.inner.handleMessage(textTop, opacity * s.alpha(), message);
            if (dx != 0.0F) {
                this.inner.updatePose(p -> p.translate(-dx, 0.0F));
            }
            return r;
        }

        @Override
        public void handleTag(int x0, int y0, int x1, int y1, float opacity, GuiMessageTag tag) {
            if (this.lineAlpha <= 0.003F || !tags) {
                return;
            }
            float dx = this.lineDx;
            if (dx != 0.0F) {
                this.inner.updatePose(p -> p.translate(dx, 0.0F));
            }
            this.inner.handleTag(x0, y0, x1, y1, opacity * this.lineAlpha, tag);
            if (dx != 0.0F) {
                this.inner.updatePose(p -> p.translate(-dx, 0.0F));
            }
        }

        @Override
        public void handleTagIcon(int left, int bottom, boolean forceVisible, GuiMessageTag tag, GuiMessageTag.Icon icon) {
            if (this.lineAlpha > 0.5F) {
                this.inner.handleTagIcon(left, bottom, forceVisible, tag, icon);
            }
        }
    }
}
