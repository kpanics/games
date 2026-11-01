package cursed.recode.launch;

import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import java.io.InputStream;
import java.io.InputStreamReader;
import java.nio.charset.StandardCharsets;
import java.util.EnumMap;
import java.util.HashMap;
import java.util.Map;

/**
 * The menu's text laid out for the DLL, which draws it from distance fields (shaders/launch/text.comp): the faces'
 * metrics from tools/text_atlas.py (assets/cursed-recode/launch/text.json), sizes in picture px (the em), letter
 * spacing in em as in the preview's CSS. A glyph is 16 numbers: its box, its field in the atlas, brightness,
 * strength, glow, and the clip it is seen through (the masks the menu's words rise out of).
 */
public final class LaunchText {
    /** The preview's faces: Sofia Sans Extra Condensed at 250, 300, 350, 450, and Martian Mono. */
    public enum Face {
        ITEM("sofia_250"), MAIN("sofia_300"), NICK("sofia_350"), BOLD("sofia_450"), MONO("martian_400");

        final String file;

        Face(String file) {
            this.file = file;
        }
    }

    private record Glyph(float adv, float left, float top, float ax, float ay, float aw, float ah) {
    }

    private record Metrics(float ascent, float descent, Map<Integer, Glyph> glyphs) {
    }

    private static final float EM;
    private static final Map<Face, Metrics> FACES = new EnumMap<>(Face.class);

    static {
        float em = 48.0F;
        try (InputStream in = LaunchText.class.getResourceAsStream("/assets/cursed-recode/launch/text.json")) {
            JsonObject root = JsonParser.parseReader(new InputStreamReader(in, StandardCharsets.UTF_8)).getAsJsonObject();
            em = root.get("em").getAsFloat();
            JsonObject faces = root.getAsJsonObject("faces");
            for (Face f : Face.values()) {
                JsonObject o = faces.getAsJsonObject(f.file);
                Map<Integer, Glyph> glyphs = new HashMap<>();
                for (Map.Entry<String, JsonElement> e : o.getAsJsonObject("glyphs").entrySet()) {
                    JsonObject g = e.getValue().getAsJsonObject();
                    var at = g.getAsJsonArray("at");
                    glyphs.put(e.getKey().codePointAt(0), new Glyph(g.get("adv").getAsFloat(), g.get("left").getAsFloat(), g.get("top").getAsFloat(),
                            at.get(0).getAsFloat(), at.get(1).getAsFloat(), at.get(2).getAsFloat(), at.get(3).getAsFloat()));
                }
                FACES.put(f, new Metrics(o.get("ascent").getAsFloat(), o.get("descent").getAsFloat(), glyphs));
            }
        } catch (Exception e) {
            throw new IllegalStateException("The launch menu's text metrics are missing", e);
        }
        EM = em;
    }

    private LaunchText() {
    }

    /** Glyphs for the frame, 16 numbers each. */
    public static final class Sink {
        float[] data = new float[16 * 256];
        int count;

        void add(float... v) {
            if ((this.count + 1) * 16 > this.data.length) {
                this.data = java.util.Arrays.copyOf(this.data, this.data.length * 2);
            }
            System.arraycopy(v, 0, this.data, this.count * 16, 16);
            this.count++;
        }

        void clear() {
            this.count = 0;
        }
    }

    private static Glyph glyph(Metrics m, int cp) {
        Glyph g = m.glyphs.get(cp);
        return g != null ? g : m.glyphs.get((int) '?');
    }

    /** The width the text takes (CSS: every letter followed by its spacing). */
    public static float width(Face face, String text, float size, float spacing) {
        Metrics m = FACES.get(face);
        float k = size / EM, w = 0.0F;
        for (int i = 0; i < text.length(); ) {
            int cp = text.codePointAt(i);
            i += Character.charCount(cp);
            w += glyph(m, cp).adv * k + spacing * size;
        }
        return w;
    }

    public static float ascent(Face face, float size) {
        return FACES.get(face).ascent * size / EM;
    }

    public static float descent(Face face, float size) {
        return FACES.get(face).descent * size / EM;
    }

    /** The baseline of text set in a line box `lineHeight` em tall starting at `top` (the CSS way: half-leading). */
    public static float baseline(Face face, float top, float size, float lineHeight) {
        Metrics m = FACES.get(face);
        float k = size / EM;
        return top + (lineHeight * size - (m.ascent + m.descent) * k) / 2.0F + m.ascent * k;
    }

    /**
     * The text at its pen `x` and `baseline` (picture px, y down); seen only inside `clip` (x0 y0 x1 y1). Returns the
     * pen's end.
     */
    public static float emit(Sink out, Face face, String text, float x, float baseline, float size, float spacing, float bright, float alpha, float glow,
                             float cx0, float cy0, float cx1, float cy1) {
        Metrics m = FACES.get(face);
        float k = size / EM;
        for (int i = 0; i < text.length(); ) {
            int cp = text.codePointAt(i);
            i += Character.charCount(cp);
            Glyph g = glyph(m, cp);
            if (cp != ' ' && alpha > 0.0F) {
                float x0 = x + g.left * k, y0 = baseline + g.top * k;
                out.add(x0, y0, x0 + g.aw * k, y0 + g.ah * k, g.ax, g.ay, g.ax + g.aw, g.ay + g.ah, bright, alpha, glow, 0.0F, cx0, cy0, cx1, cy1);
            }
            x += g.adv * k + spacing * size;
        }
        return x;
    }
}
