package cursed.recode.scene;

import com.mojang.blaze3d.systems.RenderSystem;
import com.mojang.blaze3d.vertex.VertexConsumer;
import com.mojang.renderpearl.api.pipeline.RenderPipeline;
import com.mojang.renderpearl.api.textures.FilterMode;
import com.mojang.renderpearl.api.textures.GpuTextureView;
import java.util.Arrays;
import java.util.List;
import java.util.Objects;
import net.minecraft.client.gui.font.TextRenderable;
import net.minecraft.client.gui.navigation.ScreenRectangle;
import net.minecraft.client.gui.render.TextureSetup;
import net.minecraft.client.renderer.state.gui.GlyphRenderState;
import net.minecraft.client.renderer.state.gui.GuiElementRenderState;
import net.minecraft.client.renderer.state.gui.GuiRenderState;
import org.joml.Matrix3x2fc;
import org.joml.Matrix4f;
import org.jspecify.annotations.Nullable;

/**
 * GUI text: glyphs that follow one another in a layer with the same pipeline, texture and scissor are one element
 * instead of one each (GuiRenderStateMixin). A layer's glyphs are never sorted and the GUI renderer starts a new draw
 * exactly when pipeline, texture or scissor changes, so the draws and the vertices in them stay the game's; the per-glyph
 * walk through the renderer (pipeline, a new TextureSetup, comparisons) happens once per run. The glyphs' vertices go
 * through NativeText (the DLL writes runs of plain glyphs).
 */
public final class GuiGlyphRun implements GuiElementRenderState {
    private final RenderPipeline pipeline;
    private final GpuTextureView view;
    private final @Nullable ScreenRectangle scissor;
    private final TextureSetup textureSetup;
    private Matrix3x2fc[] poses = new Matrix3x2fc[16];
    private TextRenderable[] glyphs = new TextRenderable[16];
    private int count;

    private GuiGlyphRun(RenderPipeline pipeline, GpuTextureView view, @Nullable ScreenRectangle scissor) {
        this.pipeline = pipeline;
        this.view = view;
        this.scissor = scissor;
        // GlyphRenderState.textureSetup.
        this.textureSetup = TextureSetup.singleTextureWithLightmap(view, RenderSystem.getSamplerCache().getClampToEdge(FilterMode.NEAREST));
    }

    /** GuiRenderState.addGlyphToCurrentLayer for a glyph: onto the layer's last run when it fits, else a new run. */
    public static void add(GuiRenderState.Node node, GlyphRenderState glyph) {
        TextRenderable renderable = glyph.renderable();
        RenderPipeline pipeline = renderable.guiPipeline();
        GpuTextureView view = renderable.textureView();
        ScreenRectangle scissor = glyph.scissorArea();
        List<GuiElementRenderState> list = node.glyphStates;
        if (list != null && !list.isEmpty() && list.getLast() instanceof GuiGlyphRun run && run.pipeline == pipeline && run.view == view
                && Objects.equals(run.scissor, scissor)) {
            run.push(glyph.pose(), renderable);
            return;
        }
        GuiGlyphRun run = new GuiGlyphRun(pipeline, view, scissor);
        run.push(glyph.pose(), renderable);
        node.addGlyph(run);
    }

    private void push(Matrix3x2fc pose, TextRenderable renderable) {
        if (this.count == this.glyphs.length) {
            this.poses = Arrays.copyOf(this.poses, this.count * 2);
            this.glyphs = Arrays.copyOf(this.glyphs, this.count * 2);
        }
        this.poses[this.count] = pose;
        this.glyphs[this.count] = renderable;
        this.count++;
    }

    @Override
    public void buildVertices(VertexConsumer vertexConsumer) {
        Matrix3x2fc last = null;
        Matrix4f matrix = null;
        for (int i = 0; i < this.count; i++) {
            Matrix3x2fc pose = this.poses[i];
            if (pose != last) {
                // GlyphRenderState.buildVertices: new Matrix4f().mul(pose), light 15728880, flat.
                matrix = new Matrix4f().mul(pose);
                last = pose;
            }
            NativeText.render(this.glyphs[i], matrix, vertexConsumer, 15728880, true);
        }
        NativeText.flush();
    }

    @Override
    public RenderPipeline pipeline() {
        return this.pipeline;
    }

    @Override
    public TextureSetup textureSetup() {
        return this.textureSetup;
    }

    @Override
    public @Nullable ScreenRectangle scissorArea() {
        return this.scissor;
    }

    @Override
    public @Nullable ScreenRectangle bounds() {
        return null;
    }
}
