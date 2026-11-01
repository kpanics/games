package cursed.recode.menus;

import com.google.gson.JsonArray;
import com.google.gson.JsonObject;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.components.AbstractButton;
import net.minecraft.client.gui.screens.DeathScreen;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.LivingEntity;
import org.jspecify.annotations.Nullable;

/**
 * Death: the game's words for it (You died! / Game over!), what killed, the score, and its two ways on (respawn or
 * look at the world in hardcore; the title screen), each the game's own button — they wake as the game lets them.
 */
final class DeathPage implements Page {
    final DeathScreen screen;
    final Widgets widgets = new Widgets();
    /** Who killed (the last blow's dealer, a creature: a mob or a player), as it is in the world; kept once seen. */
    private @Nullable Entity killer;
    private boolean looked;

    DeathPage(DeathScreen screen) {
        this.screen = screen;
    }

    @Override
    public JsonObject data() {
        this.widgets.clear();
        JsonObject o = new JsonObject();
        o.addProperty("title", Widgets.text(this.screen.getTitle()));
        o.addProperty("cause", Widgets.text(this.screen.causeOfDeath));
        // "Score: 12": the caption and the number apart
        String[] score = Widgets.split(Widgets.text(this.screen.deathScore));
        o.addProperty("scoreLabel", score[0]);
        o.addProperty("score", score[1]);
        o.addProperty("hardcore", this.screen.hardcore);
        // the one who killed, standing over the screen as itself
        var player = Minecraft.getInstance().player;
        if (!this.looked && player != null) {
            this.looked = true;
            var src = player.getLastDamageSource();
            Entity e = src == null ? null : src.getEntity() != null ? src.getEntity() : src.getDirectEntity();
            if (e instanceof LivingEntity && e != player) {
                this.killer = e;
            }
        }
        if (this.killer != null) {
            o.addProperty("killer", true);
        }
        JsonArray items = new JsonArray();
        for (Object e : this.screen.renderables) {
            if (e instanceof AbstractButton b && b.visible) {
                JsonObject j = this.widgets.describe(b, null, Integer.MAX_VALUE);
                if (j != null) {
                    String k = Widgets.key(b.getMessage());
                    j.addProperty("main", k.equals("deathScreen.respawn") || k.equals("deathScreen.spectate"));
                    items.add(j);
                }
            }
        }
        o.add("items", items);
        return o;
    }

    @Override
    public void act(String verb, String rest) {
        this.widgets.act(verb, rest);
    }

    /** The killer (extras kind 11: the number's fraction how far it is turned), in a dark red light. */
    @Override
    public void extras(GuiGraphicsExtractor graphics, float[] said) {
        Entity k = this.killer;
        if (k == null) {
            return;
        }
        Extras.eachRaw((kind, number, x, y, size, alpha) -> {
            if (kind == 11) {
                ItemStage.MENUS.addEntity(graphics, k, x, y, size, (number - (float) Math.floor(number)) * 360.0F, 6.0F, alpha, 0xE0766E);
            }
        });
    }
}
