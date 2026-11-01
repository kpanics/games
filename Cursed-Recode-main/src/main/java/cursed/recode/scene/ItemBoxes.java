package cursed.recode.scene;

import java.util.HashMap;
import java.util.Map;
import net.minecraft.client.resources.model.cuboid.ItemTransform;
import net.minecraft.world.phys.AABB;
import org.joml.Matrix4f;
import org.joml.Vector3fc;

/**
 * Items' model bounding boxes (ItemStackRenderState.getModelBoundingBox), remembered across frames. The game makes its
 * render states anew every frame and so transforms every extent point of the model again for every dropped item, item
 * frame and shelf each frame. The box depends only on each layer's extents (memoized per model), its ItemTransform,
 * the hand and the layer's local transform; with those the same, it is the same box. Render thread only.
 */
public final class ItemBoxes {
    private static final int MAX_LAYERS = 4;
    private static final int LIMIT = 8192;
    private static final Map<Key, AABB> BOXES = new HashMap<>();
    private static final Key PROBE = new Key();

    private ItemBoxes() {
    }

    /** The probe for this item's layers; null when there are too many layers to remember. */
    public static Key probe(int layers, boolean leftHand) {
        if (layers > MAX_LAYERS) {
            return null;
        }
        PROBE.layers = layers;
        PROBE.leftHand = leftHand;
        return PROBE;
    }

    public static AABB get(Key probe) {
        probe.hash();
        return BOXES.get(probe);
    }

    public static void put(Key probe, AABB box) {
        if (BOXES.size() >= LIMIT) {
            BOXES.clear();
        }
        BOXES.put(probe.copy(), box);
    }

    public static final class Key {
        int layers;
        boolean leftHand;
        final Object[] extents = new Object[MAX_LAYERS];
        final Object[] transforms = new Object[MAX_LAYERS];
        final int[] local = new int[MAX_LAYERS * 16];
        private int hash;

        public void layer(int i, Vector3fc[] extents, ItemTransform transform, Matrix4f local) {
            this.extents[i] = extents;
            this.transforms[i] = transform;
            int o = i * 16;
            this.local[o] = Float.floatToRawIntBits(local.m00());
            this.local[o + 1] = Float.floatToRawIntBits(local.m01());
            this.local[o + 2] = Float.floatToRawIntBits(local.m02());
            this.local[o + 3] = Float.floatToRawIntBits(local.m03());
            this.local[o + 4] = Float.floatToRawIntBits(local.m10());
            this.local[o + 5] = Float.floatToRawIntBits(local.m11());
            this.local[o + 6] = Float.floatToRawIntBits(local.m12());
            this.local[o + 7] = Float.floatToRawIntBits(local.m13());
            this.local[o + 8] = Float.floatToRawIntBits(local.m20());
            this.local[o + 9] = Float.floatToRawIntBits(local.m21());
            this.local[o + 10] = Float.floatToRawIntBits(local.m22());
            this.local[o + 11] = Float.floatToRawIntBits(local.m23());
            this.local[o + 12] = Float.floatToRawIntBits(local.m30());
            this.local[o + 13] = Float.floatToRawIntBits(local.m31());
            this.local[o + 14] = Float.floatToRawIntBits(local.m32());
            this.local[o + 15] = Float.floatToRawIntBits(local.m33());
        }

        void hash() {
            int h = this.layers * 31 + (this.leftHand ? 1 : 0);
            for (int i = 0; i < this.layers; i++) {
                h = h * 31 + System.identityHashCode(this.extents[i]);
                h = h * 31 + System.identityHashCode(this.transforms[i]);
                for (int k = i * 16; k < i * 16 + 16; k++) {
                    h = h * 31 + this.local[k];
                }
            }
            this.hash = h;
        }

        Key copy() {
            Key k = new Key();
            k.layers = this.layers;
            k.leftHand = this.leftHand;
            System.arraycopy(this.extents, 0, k.extents, 0, this.layers);
            System.arraycopy(this.transforms, 0, k.transforms, 0, this.layers);
            System.arraycopy(this.local, 0, k.local, 0, this.layers * 16);
            k.hash = this.hash;
            return k;
        }

        @Override
        public int hashCode() {
            return this.hash;
        }

        @Override
        public boolean equals(Object o) {
            if (!(o instanceof Key k) || k.layers != this.layers || k.leftHand != this.leftHand || k.hash != this.hash) {
                return false;
            }
            for (int i = 0; i < this.layers; i++) {
                if (k.extents[i] != this.extents[i] || k.transforms[i] != this.transforms[i]) {
                    return false;
                }
            }
            for (int i = 0; i < this.layers * 16; i++) {
                if (k.local[i] != this.local[i]) {
                    return false;
                }
            }
            return true;
        }
    }
}
