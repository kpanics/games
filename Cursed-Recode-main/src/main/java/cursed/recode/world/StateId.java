package cursed.recode.world;

/** A block state's number in Block.BLOCK_STATE_REGISTRY, kept on the state itself (mixin BlockStateMixin). */
public interface StateId {
    int cursed$id();

    void cursed$setId(int id);
}
