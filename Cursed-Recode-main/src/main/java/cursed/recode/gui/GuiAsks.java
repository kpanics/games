package cursed.recode.gui;

import cursed.recode.render.Native;
import net.minecraft.client.Minecraft;
import net.minecraft.client.resources.language.LanguageInfo;

import java.util.Map;

/**
 * What the client's menu and HUD ask of the game (gui::call "poll" in native/src/gui/mod.rs), a line each, its fields
 * by tabs: the clipboard to copy to or read, the game's language to change, the languages there are.
 */
public final class GuiAsks {
    private GuiAsks() {
    }

    public static void poll(Minecraft mc) {
        String s = Native.guiCall("poll");
        if (s == null || s.isEmpty()) {
            return;
        }
        for (String line : s.split("\n")) {
            String[] f = line.split("\t", 2);
            String arg = f.length > 1 ? f[1] : "";
            switch (f[0]) {
                case "copy" -> mc.keyboardHandler.setClipboard(arg);
                case "paste" -> Native.guiCall("paste\t" + mc.keyboardHandler.getClipboard());
                case "lang" -> cursed.recode.menus.MenuHost.language(arg);
                case "chat" -> cursed.recode.hud.NChat.act(arg);
                case "langs" -> {
                    // code, name, region: a language each (fields by U+1F, languages by U+1E)
                    StringBuilder b = new StringBuilder();
                    var lm = mc.getLanguageManager();
                    for (Map.Entry<String, LanguageInfo> e : lm.getLanguages().entrySet()) {
                        if (!b.isEmpty()) {
                            b.append('\u001e');
                        }
                        b.append(e.getKey()).append('\u001f').append(e.getValue().name()).append('\u001f').append(e.getValue().region());
                    }
                    Native.guiCall("langs\t" + lm.getSelected() + "\t" + b);
                }
                default -> {
                }
            }
        }
    }
}
