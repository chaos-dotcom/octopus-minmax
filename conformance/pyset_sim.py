"""CPython-compatible string hashing and set iteration order.

Derived empirically from CPython 3.11.15 (Objects/setobject.c, Python/pyhash.c) and
verified against the interpreter's own hash table (read with ctypes) and set iteration.
Used by the conformance harness to reproduce the exact ordering that
`set(tariff_ids.lower().split(","))` produces in the Python implementation.
"""
MASK64 = (1 << 64) - 1
LINEAR_PROBES = 9
PERTURB_SHIFT = 5
MINSIZE = 8


def rotl(x, b):
    return ((x << b) | (x >> (64 - b))) & MASK64


def siphash13(data, k0=0, k1=0):
    def single(v0, v1, v2, v3):
        def half(a, b, c, d, s, t):
            a = (a + b) & MASK64
            c = (c + d) & MASK64
            b = rotl(b, s) ^ a
            d = rotl(d, t) ^ c
            a = rotl(a, 32)
            return a, b, c, d
        v0, v1, v2, v3 = half(v0, v1, v2, v3, 13, 16)   # HALF_ROUND(v0,v1,v2,v3,13,16)
        v2, v1, v0, v3 = half(v2, v1, v0, v3, 17, 21)   # HALF_ROUND(v2,v1,v0,v3,17,21)
        return v0, v1, v2, v3

    v0 = 0x736F6D6570736575 ^ k0
    v1 = 0x646F72616E646F6D ^ k1
    v2 = 0x6C7967656E657261 ^ k0
    v3 = 0x7465646279746573 ^ k1
    n = len(data)
    i = 0
    while n - i >= 8:
        m = int.from_bytes(data[i:i + 8], "little")
        v3 ^= m
        v0, v1, v2, v3 = single(v0, v1, v2, v3)
        v0 ^= m
        i += 8
    tail = data[i:]
    b = ((n & 0xFF) << 56) | int.from_bytes(tail + b"\0" * (8 - len(tail)), "little")
    v3 ^= b
    v0, v1, v2, v3 = single(v0, v1, v2, v3)
    v0 ^= b
    v2 ^= 0xFF
    for _ in range(3):
        v0, v1, v2, v3 = single(v0, v1, v2, v3)
    return (v0 ^ v1 ^ v2 ^ v3) & MASK64


def str_internal_bytes(text):
    """The bytes CPython hashes for a str: its internal buffer, len * kind bytes.

    ASCII/Latin-1 strings are stored as 1 byte per character, BMP strings as
    2 (little-endian UTF-16), anything above U+FFFF as 4 (little-endian UTF-32).
    """
    points = [ord(c) for c in text]
    if all(p < 0x100 for p in points):          # Latin-1: one byte per character
        return bytes(points)
    if all(p < 0x10000 for p in points):        # BMP: UTF-16 code unit, little endian
        return b"".join(p.to_bytes(2, "little") for p in points)
    return b"".join(p.to_bytes(4, "little") for p in points)   # UTF-32, little endian


def py_hash_str(text, k0=0, k1=0):
    """Py_hash_t of a str, as CPython computes it for a given hash secret."""
    h = siphash13(str_internal_bytes(text), k0, k1)
    if h == MASK64:          # -1 is reserved
        h = MASK64 - 1
    return h


class PySet:
    """A model of CPython's set: same table, same probing, same iteration order."""

    def __init__(self, k0=0, k1=0):
        self.k0 = k0
        self.k1 = k1
        self.size = MINSIZE
        self.mask = MINSIZE - 1
        self.table = [None] * MINSIZE       # None = empty, DUMMY, or (key, hash)
        self.fill = 0
        self.used = 0

    DUMMY = object()

    def add(self, key):
        h = py_hash_str(key, self.k0, self.k1)
        if h == 0:
            h = 1
        self._add_entry(key, h)

    def _add_entry(self, key, h):
        # Mirrors set_add_entry() in CPython 3.11 Objects/setobject.c: a linear
        # probe over i..i+LINEAR_PROBES, then the perturbed re-probe off i.
        i = h & self.mask
        freeslot = None
        perturb = h
        while True:
            probes = LINEAR_PROBES if (i + LINEAR_PROBES) <= self.mask else 0
            j = i
            while True:
                entry = self.table[j]
                if entry is None:                            # unused slot
                    if freeslot is None:
                        self.table[j] = (key, h)
                        self.fill += 1
                        self.used += 1
                        if (self.fill * 5) < (self.mask * 3):
                            return
                        self._resize(self.used * 2 if self.used > 50000 else self.used * 4)
                        return
                    self.table[freeslot] = (key, h)
                    self.used += 1
                    return
                if entry is not PySet.DUMMY and entry[1] == h and entry[0] == key:
                    return                                   # already present
                if entry is PySet.DUMMY and freeslot is None:
                    freeslot = j
                if probes == 0:
                    break
                probes -= 1
                j += 1
            perturb >>= PERTURB_SHIFT
            i = (i * 5 + 1 + perturb) & self.mask

    def _insert_clean(self, key, h):
        i = h & self.mask
        perturb = h
        while True:
            if self.table[i] is None:
                self.table[i] = (key, h)
                return
            if (i + LINEAR_PROBES) <= self.mask:
                j = 0
                while j < LINEAR_PROBES:
                    j += 1
                    i += 1
                    if self.table[i] is None:
                        self.table[i] = (key, h)
                        return
            perturb >>= PERTURB_SHIFT
            i = (i * 5 + 1 + perturb) & self.mask

    def _resize(self, minused):
        newsize = MINSIZE
        while newsize <= minused:
            newsize <<= 1
        old = self.table
        self.size = newsize
        self.mask = newsize - 1
        self.table = [None] * newsize
        self.fill = self.used
        for entry in old:
            if entry is not None and entry is not PySet.DUMMY:
                self._insert_clean(entry[0], entry[1])

    def iterate(self):
        return [e[0] for e in self.table if e is not None and e is not PySet.DUMMY]


def pyset_order(items, k0=0, k1=0):
    s = PySet(k0, k1)
    for item in items:
        s.add(item)
    return s.iterate()
