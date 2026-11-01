"""Valve's KV3 as text (what VRF decompiles: sound events, clips, vdata) read into Python: objects as dicts (keys in
order), arrays as lists, numbers, booleans, strings (a typed one, resource:"x", as its string).

    data = parse(open(path, encoding="utf-8").read())"""
import re

TOKEN = re.compile(r'\s+|//[^\n]*|/\*.*?\*/|<!--.*?-->|"(?:[^"\\]|\\.)*"|[{}\[\]=,]|[^\s{}\[\]=,"]+', re.S)


def tokens(text):
    for m in TOKEN.finditer(text):
        t = m.group(0)
        if t[0].isspace() or t.startswith("//") or t.startswith("/*") or t.startswith("<!--"):
            continue
        yield t


def value(tok):
    if tok.startswith('"'):
        return bytes(tok[1:-1], "utf-8").decode("unicode_escape") if "\\" in tok else tok[1:-1]
    if tok in ("true", "false"):
        return tok == "true"
    if tok == "null":
        return None
    try:
        return int(tok)
    except ValueError:
        pass
    try:
        return float(tok)
    except ValueError:
        return tok


def parse(text):
    toks = list(tokens(text))
    pos = 0

    def any_value():
        nonlocal pos
        t = toks[pos]
        if t == "{":
            pos += 1
            return obj()
        if t == "[":
            pos += 1
            return arr()
        if t == "#" and pos + 1 < len(toks) and toks[pos + 1] == "[":
            # a blob: #[ 00 1A ... ]
            pos += 2
            out = bytearray()
            while toks[pos] != "]":
                out.append(int(toks[pos], 16))
                pos += 1
            pos += 1
            return bytes(out)
        pos += 1
        # a typed value: resource:"x", soundevent:"y" (the prefix and the string are one token or two)
        if not t.startswith('"') and t.endswith(":") and pos < len(toks) and toks[pos].startswith('"'):
            pos += 1
            return value(toks[pos - 1])
        if not t.startswith('"') and ':"' in t:
            return t.split(':"', 1)[1].rstrip('"')
        return value(t)

    def obj():
        nonlocal pos
        out = {}
        while toks[pos] != "}":
            key = toks[pos]
            key = key[1:-1] if key.startswith('"') else key
            pos += 1
            assert toks[pos] == "=", (key, toks[pos])
            pos += 1
            out[key] = any_value()
            if toks[pos] == ",":
                pos += 1
        pos += 1
        return out

    def arr():
        nonlocal pos
        out = []
        while toks[pos] != "]":
            out.append(any_value())
            if toks[pos] == ",":
                pos += 1
        pos += 1
        return out

    return any_value()


if __name__ == "__main__":
    d = parse('<!-- kv3 x --> { a = 1 b = [ 1.5, "x", ] c = { d = resource:"p/q.vpcf" e = true } "f.g" = -2 }')
    assert d == {"a": 1, "b": [1.5, "x"], "c": {"d": "p/q.vpcf", "e": True}, "f.g": -2}, d
    print("ok")
