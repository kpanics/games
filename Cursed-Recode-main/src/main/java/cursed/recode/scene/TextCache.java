package cursed.recode.scene;

import com.mojang.blaze3d.systems.RenderSystem;
import cursed.recode.dev.Dev;
import cursed.recode.mixin.client.StyleAccessor;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Objects;
import java.util.WeakHashMap;
import net.minecraft.client.gui.Font;
import net.minecraft.network.chat.Component;
import net.minecraft.network.chat.Style;
import net.minecraft.util.FormattedCharSequence;
import org.jspecify.annotations.Nullable;

/**
 * Text the game lays out again every frame, laid out once (render thread). Name tags: their width and their visual
 * order by the name's content (the names are built anew each frame and not changed after: SubmitNodeCollectionMixin);
 * prepared glyphs by the sequence and the place and colours (FontMixin) — so name tags and text displays, whose
 * sequences now stay the same objects, are prepared once. Text with obfuscated (§k) characters is never kept: its
 * glyphs change every time. Emptied when fonts or the language reload. CURSED_DEV_NOTEXTCACHE keeps the game's way.
 */
public final class TextCache {
    public static final boolean OFF = Dev.on("CURSED_DEV_NOTEXTCACHE");
    private static final int NAMES = 4096;

    /** A name tag's layout: its visual order and its width. */
    public record Name(FormattedCharSequence sequence, int width) {
    }

    private static final Map<NameKey, Name> names = new LinkedHashMap<>(256, 0.75F, true) {
        @Override
        protected boolean removeEldestEntry(Map.Entry<NameKey, Name> eldest) {
            return this.size() > NAMES;
        }
    };
    /** The last name asked for: the name tag asks for its width and then its order. */
    private static @Nullable Component lastName;
    private static @Nullable Name last;

    /**
     * A name tag's key: its contents and the style that shows — colour, shadow, bold, italic, underline, strikethrough,
     * obfuscation, font, each as set (unset = the parent's) — not the hover, click or insertion. An entity's display
     * name is built anew every frame with a hover of its own (the entity's type, UUID and name) and its UUID as the
     * insertion; hashing those each frame cost more than the rest of the name tag, and they draw nothing.
     */
    private static final class NameKey {
        private final Component name;
        private final int hash;

        NameKey(Component name) {
            this.name = name;
            this.hash = hash(name);
        }

        @Override
        public int hashCode() {
            return this.hash;
        }

        @Override
        public boolean equals(Object o) {
            return o instanceof NameKey k && k.hash == this.hash && same(this.name, k.name);
        }

        private static int hash(Component c) {
            int h = c.getContents().hashCode();
            h = 31 * h + styleHash(c.getStyle());
            List<Component> siblings = c.getSiblings();
            for (int i = 0, n = siblings.size(); i < n; i++) {
                h = 31 * h + hash(siblings.get(i));
            }
            return h;
        }

        private static int styleHash(Style style) {
            StyleAccessor s = (StyleAccessor) (Object) style;
            int h = Objects.hashCode(s.cursed$color());
            h = 31 * h + Objects.hashCode(s.cursed$shadowColor());
            h = 31 * h + Objects.hashCode(s.cursed$bold());
            h = 31 * h + Objects.hashCode(s.cursed$italic());
            h = 31 * h + Objects.hashCode(s.cursed$underlined());
            h = 31 * h + Objects.hashCode(s.cursed$strikethrough());
            h = 31 * h + Objects.hashCode(s.cursed$obfuscated());
            return 31 * h + Objects.hashCode(s.cursed$font());
        }

        private static boolean same(Component a, Component b) {
            if (a == b) {
                return true;
            }
            if (!a.getContents().equals(b.getContents()) || !sameStyle(a.getStyle(), b.getStyle())) {
                return false;
            }
            List<Component> x = a.getSiblings();
            List<Component> y = b.getSiblings();
            if (x.size() != y.size()) {
                return false;
            }
            for (int i = 0, n = x.size(); i < n; i++) {
                if (!same(x.get(i), y.get(i))) {
                    return false;
                }
            }
            return true;
        }

        private static boolean sameStyle(Style a, Style b) {
            if (a == b) {
                return true;
            }
            StyleAccessor x = (StyleAccessor) (Object) a;
            StyleAccessor y = (StyleAccessor) (Object) b;
            return Objects.equals(x.cursed$color(), y.cursed$color()) && Objects.equals(x.cursed$shadowColor(), y.cursed$shadowColor())
                    && Objects.equals(x.cursed$bold(), y.cursed$bold()) && Objects.equals(x.cursed$italic(), y.cursed$italic()) && Objects.equals(x.cursed$underlined(), y.cursed$underlined())
                    && Objects.equals(x.cursed$strikethrough(), y.cursed$strikethrough()) && Objects.equals(x.cursed$obfuscated(), y.cursed$obfuscated())
                    && Objects.equals(x.cursed$font(), y.cursed$font());
        }
    }

    /** One sequence's prepared text at a place, in colours. */
    private record Prepared(Font font, float x, float y, int color, boolean shadow, boolean includeEmpty, int background, Font.PreparedText text) {
        boolean is(Font font, float x, float y, int color, boolean shadow, boolean includeEmpty, int background) {
            return this.font == font && this.x == x && this.y == y && this.color == color && this.shadow == shadow && this.includeEmpty == includeEmpty
                    && this.background == background;
        }
    }

    private static final Map<FormattedCharSequence, Prepared[]> prepared = new WeakHashMap<>();
    /** Set by FontPreparedTextBuilderMixin while a text is prepared: it has obfuscated characters. */
    public static boolean sawObfuscated;

    private TextCache() {
    }

    public static boolean on() {
        return !OFF && RenderSystem.isOnRenderThread();
    }

    /** A name tag's layout, made by `font` once per content. */
    public static Name name(Component name, Font font) {
        if (!on()) {
            return new Name(name.getVisualOrderText(), font.width(name));
        }
        if (name == lastName && last != null) {
            return last;
        }
        NameKey key = new NameKey(name);
        Name n = names.get(key);
        if (n == null) {
            n = new Name(name.getVisualOrderText(), font.width(name));
            names.put(key, n);
        }
        lastName = name;
        last = n;
        return n;
    }

    public static Font.@Nullable PreparedText prepared(Font font, FormattedCharSequence text, float x, float y, int color, boolean shadow, boolean includeEmpty, int background) {
        Prepared[] variants = prepared.get(text);
        if (variants != null) {
            for (Prepared p : variants) {
                if (p.is(font, x, y, color, shadow, includeEmpty, background)) {
                    return p.text();
                }
            }
        }
        return null;
    }

    public static void remember(Font font, FormattedCharSequence text, float x, float y, int color, boolean shadow, boolean includeEmpty, int background, Font.PreparedText result) {
        Prepared p = new Prepared(font, x, y, color, shadow, includeEmpty, background, result);
        Prepared[] variants = prepared.get(text);
        if (variants == null) {
            prepared.put(text, new Prepared[]{p});
        } else if (variants.length < 8) {
            Prepared[] more = java.util.Arrays.copyOf(variants, variants.length + 1);
            more[variants.length] = p;
            prepared.put(text, more);
        }
    }

    /** Fonts or the language reloaded: every kept layout refers to old glyphs or another direction. */
    public static void clear() {
        names.clear();
        lastName = null;
        last = null;
        prepared.clear();
    }
}
