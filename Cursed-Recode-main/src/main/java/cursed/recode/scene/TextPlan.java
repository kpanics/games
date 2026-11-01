package cursed.recode.scene;

import com.mojang.blaze3d.vertex.VertexConsumer;
import cursed.recode.dev.Dev;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.util.ArrayList;
import java.util.List;
import net.minecraft.client.gui.Font;
import net.minecraft.client.gui.font.TextRenderable;
import net.minecraft.client.gui.font.glyphs.BakedSheetGlyph;
import net.minecraft.client.renderer.rendertype.RenderType;
import org.joml.Matrix4fc;
import org.jspecify.annotations.Nullable;
import org.lwjgl.system.MemoryUtil;

/**
 * A prepared text's renderables as TextFeatureRenderer draws them, resolved once per display mode: the runs of sheet
 * glyphs with one render type, their records for the DLL written (NativeText's layout), and the rest (the background,
 * underlines, strikethroughs, other glyphs) kept to go the game's way. Name tags and text displays are prepared once
 * (TextCache keeps their prepared text) and drawn every frame: a run is then one builder lookup and one native call,
 * not a visit, a render type choice and a builder lookup per glyph. A prepared text gets a plan on its second use (one
 * used once, like text with obfuscated characters, never does). CURSED_DEV_NOTEXTPLANS leaves the game's visit.
 */
public final class TextPlan {
    public static final boolean OFF = Dev.on("CURSED_DEV_NOTEXTPLANS") || TextCache.OFF;

    /** A run of sheet glyphs with one render type. */
    private record Run(RenderType type, ByteBuffer records, long address, int count, int vertices, BakedSheetGlyph.GlyphInstance[] glyphs) {
    }

    /** In order: a Run, or a TextRenderable drawn the game's way. */
    private final Object[] parts;

    private TextPlan(Object[] parts) {
        this.parts = parts;
    }

    /** What a plan is drawn with: TextFeatureRenderer's glyph renderer (TextGlyphRendererMixin). */
    public interface Target extends Font.GlyphVisitor {
        Matrix4fc cursed$pose();

        int cursed$light();

        Font.DisplayMode cursed$mode();

        VertexConsumer cursed$builder(RenderType type);
    }

    /** A prepared text that keeps its plans (FontPreparedTextBuilderMixin). */
    public interface Holder {
        @Nullable TextPlan cursed$plan(Font.DisplayMode mode);
    }

    /** TextFeatureRenderer's text.visit(renderer). */
    public static void visit(Font.PreparedText text, Font.GlyphVisitor visitor) {
        if (!OFF && text instanceof Holder holder && visitor instanceof Target target) {
            TextPlan plan = holder.cursed$plan(target.cursed$mode());
            if (plan != null) {
                plan.render(target);
                return;
            }
        }
        text.visit(visitor);
    }

    public static TextPlan of(Font.PreparedText text, Font.DisplayMode mode) {
        List<Object> parts = new ArrayList<>();
        List<BakedSheetGlyph.GlyphInstance> run = new ArrayList<>();
        RenderType[] runType = new RenderType[1];
        text.visit(new Font.GlyphVisitor() {
            @Override
            public void acceptGlyph(TextRenderable.Styled glyph) {
                if (glyph instanceof BakedSheetGlyph.GlyphInstance g) {
                    RenderType type = g.renderType(mode);
                    if (!run.isEmpty() && type != runType[0]) {
                        parts.add(run(runType[0], run));
                        run.clear();
                    }
                    runType[0] = type;
                    run.add(g);
                } else {
                    this.acceptRenderable(glyph);
                }
            }

            @Override
            public void acceptRenderable(TextRenderable renderable) {
                if (!run.isEmpty()) {
                    parts.add(run(runType[0], run));
                    run.clear();
                }
                parts.add(renderable);
            }
        });
        if (!run.isEmpty()) {
            parts.add(run(runType[0], run));
        }
        return new TextPlan(parts.toArray());
    }

    private static Run run(RenderType type, List<BakedSheetGlyph.GlyphInstance> glyphs) {
        ByteBuffer records = ByteBuffer.allocateDirect(NativeText.GLYPH * glyphs.size()).order(ByteOrder.nativeOrder());
        long address = MemoryUtil.memAddress(records);
        int vertices = 0;
        for (int i = 0; i < glyphs.size(); i++) {
            vertices += NativeText.record(address + (long) NativeText.GLYPH * i, glyphs.get(i));
        }
        return new Run(type, records, address, glyphs.size(), vertices, glyphs.toArray(new BakedSheetGlyph.GlyphInstance[0]));
    }

    private void render(Target target) {
        Matrix4fc pose = target.cursed$pose();
        int light = target.cursed$light();
        Font.DisplayMode mode = target.cursed$mode();
        for (Object part : this.parts) {
            if (part instanceof Run run) {
                VertexConsumer consumer = target.cursed$builder(run.type);
                if (!NativeText.renderRun(consumer, pose, light, run.address, run.count, run.vertices, run.glyphs)) {
                    for (BakedSheetGlyph.GlyphInstance g : run.glyphs) {
                        NativeText.renderFeature(g, pose, consumer, light, false);
                    }
                }
            } else {
                TextRenderable renderable = (TextRenderable) part;
                NativeText.renderFeature(renderable, pose, target.cursed$builder(renderable.renderType(mode)), light, false);
            }
        }
    }
}
