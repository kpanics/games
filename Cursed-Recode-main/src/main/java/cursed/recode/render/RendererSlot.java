package cursed.recode.render;

/**
 * An entity type's renderer, kept on the type itself (EntityTypeMixin, EntityRenderDispatcherMixin): the game looks it
 * up in a map for every entity every frame, and every entity in range is checked for being seen each frame (12.10:
 * the lookup a fifth of that check). `generation` is the dispatcher's renderers' (they are made anew on a reload).
 */
public interface RendererSlot {
    Object cursed$renderer();

    int cursed$generation();

    void cursed$keep(Object renderer, int generation);
}
