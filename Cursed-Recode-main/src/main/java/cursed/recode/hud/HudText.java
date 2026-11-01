package cursed.recode.hud;

import java.util.ArrayList;
import java.util.List;
import java.util.Optional;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.network.chat.Component;
import net.minecraft.network.chat.FontDescription;
import net.minecraft.network.chat.FormattedText;
import net.minecraft.network.chat.Style;
import net.minecraft.network.chat.TextColor;
import net.minecraft.util.ARGB;
import net.minecraft.util.FormattedCharSequence;
import org.jspecify.annotations.Nullable;

/**
 * A server's words the HUD writes in its own letters (native gui/hudtext.rs): a component as its runs — the words, their
 * colour, bold, italic, underlined, struck, obfuscated, and where a pack's glyphs are (those Java draws in their place,
 * icon 25: a slot's run). The runs as the chat's (NChat): fields by U+0003 (words, RRGGBB, flags, the glyphs' width in
 * the game's units), runs by U+0002.
 *
 * Slots: 0 the sidebar's title, 1..32 its lines, 33..64 their scores, 70..77 the boss bars, 80 the title, 81 the
 * subtitle, 82 the action bar, 90 a container's title, 100..139 an item's tooltip (HudPack).
 */
public final class HudText {
    private HudText() {
    }

    public static final int BOARD_TITLE = 0, BOARD_LINE = 1, BOARD_SCORE = 33, BAR = 70, TITLE = 80, SUBTITLE = 81, ACTION = 82;

    private static final int SLOTS = 160;
    @SuppressWarnings("unchecked")
    private static final List<Style>[] STYLES = new List[SLOTS];
    @SuppressWarnings("unchecked")
    private static final List<String>[] TEXTS = new List[SLOTS];

    /**
     * A component's text as the game draws it: each piece with its style, a server's colours written into the text
     * itself (§ and a letter: the sidebar's lines, the tab, the servers' descriptions, old plugins' chat) read as the
     * game reads them — §r back to the piece's own style, the codes themselves not shown.
     */
    public static void pieces(@Nullable FormattedText c, java.util.function.BiConsumer<Style, String> out) {
        if (c == null) {
            return;
        }
        c.visit((style, text) -> {
            if (text.isEmpty()) {
                return Optional.empty();
            }
            if (text.indexOf('§') < 0) {
                out.accept(style, text);
                return Optional.empty();
            }
            StringBuilder cur = new StringBuilder();
            Style[] at = {null};
            net.minecraft.util.StringDecomposer.iterateFormatted(text, style, (pos, st, cp) -> {
                if (at[0] != null && !st.equals(at[0]) && !cur.isEmpty()) {
                    out.accept(at[0], cur.toString());
                    cur.setLength(0);
                }
                at[0] = st;
                cur.appendCodePoint(cp);
                return true;
            });
            if (!cur.isEmpty()) {
                out.accept(at[0], cur.toString());
            }
            return Optional.empty();
        }, Style.EMPTY);
    }

    /** The component's runs (empty: none), its glyphs kept in the slot for drawing. */
    static String encode(int slot, @Nullable Component c) {
        List<Style> styles = new ArrayList<>();
        List<String> texts = new ArrayList<>();
        StringBuilder runs = new StringBuilder();
        if (c != null) {
            Minecraft mc = Minecraft.getInstance();
            pieces(c, (style, text) -> {
                boolean ownFont = !style.getFont().equals(FontDescription.DEFAULT);
                // the words cut where a pack's glyphs begin and end
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
        }
        if (slot >= 0 && slot < SLOTS) {
            STYLES[slot] = styles;
            TEXTS[slot] = texts;
        }
        return runs.toString();
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
        TextColor col = style.getColor();
        if (col == null) {
            fl |= 256;
        }
        if (!runs.isEmpty()) {
            runs.append('\u0002');
        }
        runs.append(clean(text)).append('\u0003').append(String.format("%06X", col != null ? col.getValue() & 0xFFFFFF : 0xFFFFFF))
                .append('\u0003').append(fl).append('\u0003').append(gw);
        styles.add(style);
        texts.add(text);
    }

    /** Letters a server's pack draws (its private ones, and those packs commonly take over: Yi, Vai). */
    static boolean glyph(int cp) {
        return cp >= 0xE000 && cp <= 0xF8FF || cp >= 0xF0000 || cp >= 0xA000 && cp <= 0xA63F;
    }

    private static String clean(String s) {
        StringBuilder b = new StringBuilder(s.length());
        for (int i = 0; i < s.length(); i++) {
            char c = s.charAt(i);
            b.append(c == '\t' || c == '\n' || c == '\u0001' || c == '\u0002' || c == '\u0003' || c == '\r' ? ' ' : c);
        }
        return b.toString();
    }

    /** Icon 25: a slot's run of a pack's glyphs, in its place (the box's height is the game's line, 9 units). */
    static void icon(GuiGraphicsExtractor graphics, int index, float x, float y, float w, float h, float alpha) {
        int slot = index >> 8, run = index & 255;
        if (slot < 0 || slot >= SLOTS || STYLES[slot] == null || run >= STYLES[slot].size()) {
            return;
        }
        var pose = graphics.pose();
        pose.pushMatrix();
        pose.translate(x, y);
        float k = h / 9.0F;
        pose.scale(k, k);
        graphics.text(Minecraft.getInstance().font, FormattedCharSequence.forward(TEXTS[slot].get(run), STYLES[slot].get(run)), 0, 0,
                ARGB.color(Math.round(Math.min(1.0F, alpha) * 255.0F), 0xFFFFFF), false);
        pose.popMatrix();
    }
}
