"""Valve's compiled material expressions (a shader variable's m_pCompiledExpression: how a material's numbers become
the shader's — the pattern's transform from its scale, rotation and offset…) read back as text, after VRF's VfxEval
(MIT): a stack machine of literals, material parameters (by the MurmurHash2 of their lowercased names), local
variables, operators, functions and branches. Fully bracketed (the reading, not the beauty, matters).

    python vfxeval.py HEX [NAME ...]        the names the parameters may have (to hash)"""
import struct
import sys

FUNCS = [("sin", 1), ("cos", 1), ("tan", 1), ("frac", 1), ("floor", 1), ("ceil", 1), ("saturate", 1), ("clamp", 3), ("lerp", 3), ("dot4", 2),
         ("dot3", 2), ("dot2", 2), ("log", 1), ("log2", 1), ("log10", 1), ("exp", 1), ("exp2", 1), ("sqrt", 1), ("rsqrt", 1), ("sign", 1),
         ("abs", 1), ("pow", 2), ("step", 2), ("smoothstep", 3), ("float4", 4), ("float3", 3), ("float2", 2), ("time", 0), ("min", 2), ("max", 2),
         ("SrgbLinearToGamma", 1), ("SrgbGammaToLinear", 1), ("random", 2), ("normalize", 1), ("length", 1), ("sqr", 1), ("rotation2d", 1),
         ("rotate2d", 2), ("sincos", 1), ("TextureSize", 1), ("TextureAverageColor", 1), ("MatrixIdentity", 0), ("MatrixScale", 1),
         ("MatrixTranslate", 1), ("MatrixAxisAngle", 1), ("MatrixAxisToAxis", 2), ("MatrixMultiply", 2), ("MatrixColorCorrect", 1),
         ("MatrixColorCorrect2", 2), ("MatrixColorTint", 1), ("normalize_safe", 1), ("Remap01ScaleOffset", 1), ("radians", 1), ("degrees", 1),
         ("MatrixColorTint2", 2), ("MatrixColorTint3", 3), ("RemapVal", 5), ("RemapValClamped", 5)]
OPS = {0x0A: "||", 0x0B: "&&", 0x0D: "==", 0x0E: "!=", 0x0F: ">", 0x10: ">=", 0x11: "<", 0x12: "<=", 0x13: "+", 0x14: "-", 0x15: "*", 0x16: "/", 0x17: "%"}


def murmur2(data, seed=0x31415926):
    m = 0x5BD1E995
    h = (seed ^ len(data)) & 0xFFFFFFFF
    i = 0
    while len(data) - i >= 4:
        k = struct.unpack_from("<I", data, i)[0]
        k = (k * m) & 0xFFFFFFFF
        k ^= k >> 24
        k = (k * m) & 0xFFFFFFFF
        h = (h * m) & 0xFFFFFFFF
        h ^= k
        i += 4
    rest = data[i:]
    if len(rest) == 3:
        h ^= rest[2] << 16
    if len(rest) >= 2:
        h ^= rest[1] << 8
    if len(rest) >= 1:
        h ^= rest[0]
        h = (h * m) & 0xFFFFFFFF
    h ^= h >> 13
    h = (h * m) & 0xFFFFFFFF
    h ^= h >> 15
    return h


def token(name):
    return murmur2(name.lower().encode("utf-8"))


def swizzle(b):
    s = "".join("xyzw"[(b >> (i * 2)) & 3] for i in range(4))
    while len(s) > 1 and s[-1] == s[-2]:
        s = s[:-1]
    return s


def decompile(blob, names=()):
    tokens = {token(n): n for n in names}
    stack = []
    lines = []
    exits = []  # (position, kind) markers: where a branch's blocks end
    locals_ = {}
    pos = 0

    def local(i):
        return locals_.setdefault(i, f"v{len(locals_)}")

    while pos < len(blob):
        # branches finished here: combine condition ? a : b
        while exits and exits[-1][0] == pos:
            _, true_first = exits.pop()
            second, first, cond = stack.pop(), stack.pop(), stack.pop()
            t, f = (first, second) if true_first else (second, first)
            stack.append(f"({cond} ? {t} : {f})")
        op = blob[pos]
        pos += 1
        if op == 0x02:  # JUMP: the end of the first block marks where the branch exits
            target = struct.unpack_from("<H", blob, pos)[0]
            pos += 2
            kind = exits.pop()[1] if exits and exits[-1][0] == -1 else True
            exits.append((target + 1 - 1 + 0, kind))
            exits[-1] = (target, kind)
        elif op == 0x04:  # BRANCH
            when_true, _ = struct.unpack_from("<HH", blob, pos)
            pos += 4
            exits.append((-1, when_true == pos))
        elif op == 0x06:
            fid = blob[pos]
            pos += 2
            name, n = FUNCS[fid]
            args = [stack.pop() for _ in range(n)][::-1]
            stack.append(f"{name}({', '.join(args)})")
        elif op == 0x07:
            stack.append(repr(struct.unpack_from("<f", blob, pos)[0]))
            pos += 4
        elif op == 0x08:
            lines.append(f"{local(blob[pos])} = {stack.pop()};")
            pos += 1
        elif op == 0x09:
            stack.append(local(blob[pos]))
            pos += 1
        elif op in (0x0C, 0x18):
            stack.append(("!" if op == 0x0C else "-") + f"({stack.pop()})")
        elif op in OPS:
            b, a = stack.pop(), stack.pop()
            stack.append(f"({a} {OPS[op]} {b})")
        elif op in (0x19, 0x1D, 0x1F):
            t = struct.unpack_from("<I", blob, pos)[0]
            pos += 4
            n = tokens.get(t, f"PARAM[{t:08x}]")
            stack.append(f"exists({n})" if op == 0x1F else n)
        elif op == 0x1A:
            stack.append(f"FEAT[{blob[pos]}]")
            pos += 1
        elif op == 0x22:
            stack.append(f"SYS[{blob[pos]}]")
            pos += 1
        elif op == 0x1E:
            stack.append(f"{stack.pop()}.{swizzle(blob[pos])}")
            pos += 1
        elif op == 0x00:
            lines.append(f"return {stack.pop()};")
            break
        else:
            raise ValueError(f"opcode {op:#x} at {pos - 1}")
    return "\n".join(lines)


if __name__ == "__main__":
    print(decompile(bytes.fromhex(sys.argv[1]), sys.argv[2:]))
