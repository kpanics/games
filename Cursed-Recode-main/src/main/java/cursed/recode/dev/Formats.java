package cursed.recode.dev;

import com.mojang.blaze3d.vertex.DefaultVertexFormat;
import com.mojang.renderpearl.api.vertex.VertexFormat;
import java.util.IdentityHashMap;
import java.util.Map;

/** A vertex format's name among DefaultVertexFormat's (the dev counts of the upload). */
public final class Formats {
    private static final Map<VertexFormat, String> NAMES = new IdentityHashMap<>();

    private Formats() {
    }

    public static String name(VertexFormat f) {
        String name = NAMES.get(f);
        if (name == null) {
            name = "other";
            for (java.lang.reflect.Field field : DefaultVertexFormat.class.getFields()) {
                try {
                    if (field.get(null) == f) {
                        name = field.getName();
                        break;
                    }
                } catch (IllegalAccessException ignored) {
                }
            }
            NAMES.put(f, name);
        }
        return name;
    }
}
