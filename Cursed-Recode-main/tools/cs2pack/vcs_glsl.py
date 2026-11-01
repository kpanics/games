"""A CS2 shader's code as GLSL (to read its math, to port it): one static combo of a compiled .vcs (as it lies in the
shaders' archive: the resource, then each combo's bytecode — -5, its size, its packed size, zstd with Valve's
dictionary, then a word, the SPIR-V's length in bytes, the SPIR-V), through the SPIRV-Cross VRF carries (its C API).

    python vcs_glsl.py FILE.vcs DESCRIPTION.kv3 COMBO OUT.glsl

DESCRIPTION.kv3 is VRF's decompile of the same file (its static combos, its bytecode table); COMBO the static combo's
ID (the sum of each S_ combo's value times its m_nComboIndexValue)."""
import ctypes
import os
import struct
import sys

import zstandard

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import kv3  # noqa: E402

SPVC = os.environ.get("SPIRV_CROSS", "spirv-cross.dll")
DICTS = r"D:\CS2\tools\vrfref\vfx_zstd_{}.bin"


def spirv_of(vcs, desc, combo):
    d = open(vcs, "rb").read()
    resource = struct.unpack_from("<I", d, 0)[0]
    k = desc["m_staticComboIDs"].index(combo)
    bc = desc["m_byteCodeData"][desc["m_staticComboData"][k]["m_nByteCodeDataIdx"]]
    blob = d[resource + bc["m_nOffs"]:resource + bc["m_nOffs"] + bc["m_nSize"]]
    marker, size, packed = struct.unpack_from("<iII", blob, 0)
    raw = None
    for did in ("255df362", "2bc2fa87"):
        dic = zstandard.ZstdCompressionDict(open(DICTS.format(did), "rb").read(), dict_type=zstandard.DICT_TYPE_AUTO)
        try:
            raw = zstandard.ZstdDecompressor(dict_data=dic).decompress(blob[12:12 + packed], max_output_size=size)
            break
        except zstandard.ZstdError:
            continue
    if raw is None:
        raise SystemExit("not unpacked")
    length = struct.unpack_from("<I", raw, 4)[0]
    return raw[8:8 + length]


def glsl(spirv, backend=1):
    """Vulkan's GLSL (backend 1), or HLSL (2: for reading, when a block's layout has no GLSL)."""
    words = (ctypes.c_uint32 * (len(spirv) // 4)).from_buffer_copy(spirv[: len(spirv) // 4 * 4])
    lib = ctypes.CDLL(SPVC)
    lib.spvc_context_get_last_error_string.restype = ctypes.c_char_p
    ctx = ctypes.c_void_p()
    assert lib.spvc_context_create(ctypes.byref(ctx)) == 0
    ir = ctypes.c_void_p()
    if lib.spvc_context_parse_spirv(ctx, words, ctypes.c_size_t(len(words)), ctypes.byref(ir)) != 0:
        raise SystemExit("parse failed: %s" % lib.spvc_context_get_last_error_string(ctx))
    comp = ctypes.c_void_p()
    assert lib.spvc_context_create_compiler(ctx, backend, ir, 1, ctypes.byref(comp)) == 0
    opts = ctypes.c_void_p()
    lib.spvc_compiler_create_compiler_options(comp, ctypes.byref(opts))
    if backend == 1:
        # its blocks' layouts as they are, 460
        lib.spvc_compiler_options_set_bool(opts, 10 | 0x2000000, 1)
        lib.spvc_compiler_options_set_uint(opts, 8 | 0x2000000, 460)
    else:
        # shader model 6.0
        lib.spvc_compiler_options_set_uint(opts, 13 | 0x4000000, 60)
    lib.spvc_compiler_install_compiler_options(comp, opts)
    src = ctypes.c_char_p()
    if lib.spvc_compiler_compile(comp, ctypes.byref(src)) != 0:
        error = lib.spvc_context_get_last_error_string(ctx)
        if backend == 1:
            print("no GLSL (%s): HLSL instead" % error.decode("utf-8", "replace"))
            return glsl(spirv, 2)
        raise SystemExit("compile failed: %s" % error)
    return src.value.decode("utf-8")


if __name__ == "__main__":
    vcs, desc_path, combo, out = sys.argv[1], sys.argv[2], int(sys.argv[3]), sys.argv[4]
    desc = kv3.parse(open(desc_path, encoding="utf-8").read())
    text = glsl(spirv_of(vcs, desc, combo))
    open(out, "w", encoding="utf-8").write(text)
    print(out, len(text))
