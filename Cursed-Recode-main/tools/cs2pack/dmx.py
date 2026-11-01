"""Valve's Datamodel (DMX), binary encoding (up to 9, as VRF writes an animation clip), read into Python: elements
(class, name, attributes), the first the root. The layout is Datamodel.NET's (Artfunkel, MIT): Codecs/Binary.cs.

    root = load(path)            # an Element
    root["animationList"]        # attributes by name"""
import struct
import uuid

# the attribute types of encoding 9, from 1 (arrays from 1 + 2·len)
TYPES9 = ["element", "int", "float", "bool", "string", "binary", "time", "color", "vector2", "vector3", "vector4", "qangle", "quaternion", "matrix", "uint64", "uint8"]
TYPES5 = TYPES9[:14]


class Element(dict):
    def __init__(self, cls, name, guid):
        super().__init__()
        self.cls = cls
        self.name = name
        self.guid = guid

    def __repr__(self):
        return f"<{self.cls} {self.name!r}>"


class Reader:
    def __init__(self, d):
        self.d = d
        self.at = 0

    def take(self, n):
        s = self.d[self.at:self.at + n]
        if len(s) < n:
            raise ValueError("the DMX ends early")
        self.at += n
        return s

    def i32(self):
        return struct.unpack("<i", self.take(4))[0]

    def raw_string(self):
        e = self.d.index(b"\0", self.at)
        s = self.d[self.at:e].decode("utf-8", "replace")
        self.at = e + 1
        return s


def load(path):
    d = open(path, "rb").read()
    head_end = d.index(b"\0")
    header = d[:head_end].decode("ascii", "replace")
    if "binary" not in header:
        raise ValueError("not a binary DMX: " + header)
    version = int(header.split("binary")[1].split()[0])
    r = Reader(d)
    r.at = head_end + 1
    types = TYPES9 if version >= 9 else TYPES5
    strings = []

    def type_of(tid):
        tid -= 1
        if version >= 9 and tid >= len(types) * 2:
            return types[tid - len(types) * 2], True
        if version < 9 and tid >= len(types):
            return types[tid - len(types)], True
        return types[tid], False

    elements = []

    def value(t, raw_string):
        if t == "element":
            i = r.i32()
            if i == -1:
                return None
            if i == -2:
                return ("stub", r.raw_string())
            return ("ref", i)
        if t == "int":
            return r.i32()
        if t == "float":
            return struct.unpack("<f", r.take(4))[0]
        if t == "bool":
            return r.take(1)[0] != 0
        if t == "string":
            if raw_string:
                return r.raw_string()
            return strings[r.i32()]
        if t == "binary":
            return r.take(r.i32())
        if t == "time":
            return r.i32() / 10000.0
        if t == "color":
            return tuple(r.take(4))
        n = {"vector2": 2, "vector3": 3, "vector4": 4, "qangle": 3, "quaternion": 4, "matrix": 16}.get(t)
        if n:
            return struct.unpack(f"<{n}f", r.take(4 * n))
        if t == "uint64":
            return struct.unpack("<Q", r.take(8))[0]
        if t == "uint8":
            return r.take(1)[0]
        raise ValueError("unknown type " + t)

    def attribute():
        t, array = type_of(r.take(1)[0])
        if not array:
            return value(t, version < 4)
        n = r.i32()
        return [value(t, True) for _ in range(n)]

    if version >= 9:
        for k in range(r.i32()):
            for _ in range(r.i32()):
                r.raw_string()
                attribute()
    if version >= 2:
        n = r.i32() if version >= 4 else struct.unpack("<h", r.take(2))[0]
        strings = [r.raw_string() for _ in range(n)]
    count = r.i32()
    for _ in range(count):
        cls = strings[r.i32()] if version >= 5 else r.raw_string()
        name = strings[r.i32()] if version >= 5 else r.raw_string()
        guid = uuid.UUID(bytes_le=r.take(16))
        elements.append(Element(cls, name, guid))
    for e in elements:
        for _ in range(r.i32()):
            name = strings[r.i32()] if version >= 5 else r.raw_string()
            e[name] = attribute()

    def resolve(v):
        if isinstance(v, tuple) and len(v) == 2 and v[0] == "ref":
            return elements[v[1]]
        if isinstance(v, list):
            return [resolve(x) for x in v]
        return v

    for e in elements:
        for k in list(e.keys()):
            e[k] = resolve(e[k])
    return elements[0] if elements else None


def dump(e, depth=0, most=3, seen=None):
    """The tree under `e`, its attributes' kinds (for a look at a file)."""
    seen = seen if seen is not None else set()
    pad = "  " * depth
    if id(e) in seen:
        print(pad + f"(again {e!r})")
        return
    seen.add(id(e))
    print(pad + repr(e))
    if depth >= most:
        return
    for k, v in e.items():
        if isinstance(v, Element):
            print(pad + f"  {k}:")
            dump(v, depth + 2, most, seen)
        elif isinstance(v, list) and v and isinstance(v[0], Element):
            print(pad + f"  {k}: [{len(v)} elements] first:")
            dump(v[0], depth + 2, most, seen)
        elif isinstance(v, list):
            print(pad + f"  {k}: [{len(v)}] {str(v[:3])[:120]}")
        else:
            print(pad + f"  {k} = {str(v)[:120]}")


if __name__ == "__main__":
    import sys
    dump(load(sys.argv[1]), most=int(sys.argv[2]) if len(sys.argv) > 2 else 4)
