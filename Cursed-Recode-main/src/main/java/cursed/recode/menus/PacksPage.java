package cursed.recode.menus;

import com.google.gson.JsonArray;
import com.google.gson.JsonObject;
import java.util.ArrayList;
import java.util.List;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.screens.packs.PackSelectionModel;
import net.minecraft.client.gui.screens.packs.PackSelectionScreen;
import net.minecraft.client.gui.screens.packs.TransferableSelectionList;
import net.minecraft.client.renderer.RenderPipelines;
import net.minecraft.network.chat.Component;
import net.minecraft.resources.Identifier;

/**
 * Resource packs (and a new world's data packs): the game's two lists, the available and the chosen, as two columns —
 * each pack its icon (the game draws it), its name, its description, whether it fits this version; a click moves it
 * across (the game asks first if it does not fit), the chosen ones go up and down; the search; the folder; done.
 */
final class PacksPage implements Page {
    final PackSelectionScreen screen;
    final Widgets widgets = new Widgets();
    private final List<Identifier> icons = new ArrayList<>();
    private final List<TransferableSelectionList.PackEntry> entries = new ArrayList<>();

    PacksPage(PackSelectionScreen screen) {
        this.screen = screen;
    }

    private JsonArray list(TransferableSelectionList list) {
        JsonArray out = new JsonArray();
        if (list == null) {
            return out;
        }
        for (Object o : list.children()) {
            if (!(o instanceof TransferableSelectionList.PackEntry e)) {
                continue;
            }
            PackSelectionModel.Entry p = e.pack;
            JsonObject j = new JsonObject();
            j.addProperty("n", this.entries.size());
            this.entries.add(e);
            j.addProperty("id", p.getId());
            j.addProperty("title", p.getTitle().getString());
            j.addProperty("desc", p.getExtendedDescription().getString());
            j.addProperty("icon", this.icons.size());
            this.icons.add(p.getIconTexture());
            j.addProperty("ok", p.getCompatibility().isCompatible());
            if (!p.getCompatibility().isCompatible()) {
                j.addProperty("why", p.getCompatibility().getDescription().getString());
            }
            j.addProperty("sel", p.canSelect());
            j.addProperty("unsel", p.canUnselect());
            j.addProperty("up", p.canMoveUp());
            j.addProperty("down", p.canMoveDown());
            out.add(j);
        }
        return out;
    }

    @Override
    public JsonObject data() {
        this.widgets.clear();
        this.icons.clear();
        this.entries.clear();
        JsonObject o = new JsonObject();
        o.addProperty("title", Widgets.text(this.screen.getTitle()));
        o.addProperty("drop", Component.translatable("pack.dropInfo").getString());
        o.addProperty("available", Component.translatable("pack.available.title").getString());
        o.addProperty("selected", Component.translatable("pack.selected.title").getString());
        if (this.screen.search != null) {
            o.addProperty("searchId", this.widgets.number(this.screen.search, null));
            o.addProperty("search", this.screen.search.getValue());
            o.addProperty("hint", Widgets.text(this.screen.search.hint));
        }
        o.add("left", this.list(this.screen.availablePackList));
        o.add("right", this.list(this.screen.selectedPackList));
        o.addProperty("folder", Component.translatable("pack.openFolder").getString());
        o.addProperty("done", this.screen.doneButton == null || this.screen.doneButton.active);
        return o;
    }

    @Override
    public void act(String verb, String rest) {
        if (this.widgets.act(verb, rest)) {
            return;
        }
        if (verb.equals("folder")) {
            com.mojang.blaze3d.Blaze3D.openPath(this.screen.packDir);
            return;
        }
        int i;
        try {
            i = Integer.parseInt(rest.trim());
        } catch (NumberFormatException e) {
            return;
        }
        if (i < 0 || i >= this.entries.size()) {
            return;
        }
        TransferableSelectionList.PackEntry e = this.entries.get(i);
        switch (verb) {
            case "move" -> e.keyboardSelection();
            case "up" -> e.keyboardMoveUp();
            case "down" -> e.keyboardMoveDown();
            default -> { }
        }
    }

    @Override
    public void extras(GuiGraphicsExtractor graphics, float[] said) {
        double s = Minecraft.getInstance().getWindow().getGuiScale();
        Extras.each((kind, ref, x, y, size, alpha) -> {
            if (kind == 4 && ref >= 0 && ref < this.icons.size() && alpha > 0.02F) {
                float k = size / (float) s / 32.0F;
                graphics.pose().pushMatrix();
                graphics.pose().translate(x / (float) s, y / (float) s);
                graphics.pose().scale(k, k);
                graphics.blit(RenderPipelines.GUI_TEXTURED, this.icons.get(ref), 0, 0, 0.0F, 0.0F, 32, 32, 32, 32, net.minecraft.util.ARGB.white(alpha));
                graphics.pose().popMatrix();
            }
        });
    }
}
