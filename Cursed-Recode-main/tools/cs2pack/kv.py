"""Valve's KeyValues (text: items_game.txt, the games' localisation files) read into Python: a block is a list of
(key, value) pairs in order (keys repeat in Valve's files), a value a string or another block.

    blk = parse(open(path, encoding="utf-8").read())
    get(blk, "items_game", "items")      # the first block under those keys
    pairs(blk)                           # the pairs as they are"""


def tokens(text):
    i = 0
    n = len(text)
    while i < n:
        c = text[i]
        if c in " \t\r\n﻿":
            i += 1
        elif c == "/" and text.startswith("//", i):
            j = text.find("\n", i)
            i = n if j < 0 else j + 1
        elif c in "{}":
            yield c
            i += 1
        elif c == '"':
            j = i + 1
            out = []
            while j < n and text[j] != '"':
                if text[j] == "\\" and j + 1 < n:
                    nxt = text[j + 1]
                    out.append({"n": "\n", "t": "\t", "\\": "\\", '"': '"'}.get(nxt, "\\" + nxt))
                    j += 2
                    continue
                out.append(text[j])
                j += 1
            yield ("s", "".join(out))
            i = j + 1
        elif c == "[":
            # a condition ([$WIN32]) after a value: passed over
            j = text.find("]", i)
            i = n if j < 0 else j + 1
        else:
            j = i
            while j < n and text[j] not in " \t\r\n{}\"":
                j += 1
            yield ("s", text[i:j])
            i = j


def parse(text):
    stack = [[]]
    key = None
    for t in tokens(text):
        if t == "{":
            blk = []
            stack[-1].append((key if key is not None else "", blk))
            stack.append(blk)
            key = None
        elif t == "}":
            if len(stack) > 1:
                stack.pop()
            key = None
        else:
            s = t[1]
            if key is None:
                key = s
            else:
                stack[-1].append((key, s))
                key = None
    return stack[0]


def pairs(blk):
    return blk if isinstance(blk, list) else []


def get(blk, *keys):
    """The first value under these keys (case-insensitive, as Valve's), or None."""
    cur = blk
    for k in keys:
        if not isinstance(cur, list):
            return None
        kl = k.lower()
        nxt = None
        for kk, v in cur:
            if kk.lower() == kl:
                nxt = v
                break
        cur = nxt
        if cur is None:
            return None
    return cur


def all_blocks(blk, key):
    """Every block under `key` (items_game repeats its sections: several "items", "paint_kits"…), merged in order."""
    kl = key.lower()
    out = []
    for k, v in pairs(blk):
        if k.lower() == kl and isinstance(v, list):
            out.extend(v)
    return out


if __name__ == "__main__":
    sample = '''"a" { "b" "1" // c
    "d" { "e" "x\\"y" } "b" "2" }'''
    p = parse(sample)
    assert get(p, "a", "b") == "1"
    assert get(p, "a", "d", "e") == 'x"y'
    assert [v for k, v in get(p, "a") if k == "b"] == ["1", "2"]
    print("ok")
