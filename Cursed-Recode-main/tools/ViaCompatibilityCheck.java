package cursed.recode.via;

/** Run with assertions enabled; exercises policy without starting Minecraft or joining servers. */
public final class ViaCompatibilityCheck {
    public static void main(String[] args) {
        String host = "compatibility.invalid";
        String decode = "Failed to decode packet 'clientbound/minecraft:initialize_border'";
        assert Compatibility.automatic(host, 777) == 777;
        assert !Compatibility.failed(host, 777, 777, 100, decode) : "manual version must win";
        assert !Compatibility.failed(host, 777, 0, 16_000, decode) : "no reconnect after settling";
        assert !Compatibility.failed(host, 777, 0, 100, "Timed out");
        assert !Compatibility.failed(host, 777, 0, 100, "You are banned");
        assert !Compatibility.failed(host, 774, 0, 100, decode) : "a fallback never retries";
        assert Compatibility.failed(host, 777, 0, 100, decode);
        assert Compatibility.automatic("COMPATIBILITY.INVALID", 777) == 774;
        assert Compatibility.automatic(host, 767) == 767 : "never upgrade an older server";
        assert Compatibility.automatic("another.invalid", 777) == 777;
        assert !Compatibility.failed(host, 777, 0, 100, decode) : "no retry loop";
        Compatibility.reset(host);
        assert Compatibility.automatic(host, 777) == 777;
        assert Compatibility.failed(host, 776, 0, 100, "Failed to decode packet 'clientbound/minecraft:set_default_spawn_position'");
        Compatibility.reset(host);
        System.out.println("Via compatibility policy: all checks passed");
    }
}
