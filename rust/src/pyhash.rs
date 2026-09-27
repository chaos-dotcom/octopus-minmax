//! CPython-compatible string hashing and `set` ordering.
//!
//! `BotOrchestrator._load_tariffs_from_ids` iterates `set(tariff_ids.lower().split(","))`,
//! so the order of the tariff lines in a notification and in the log is a Python
//! hash-table artefact.  This module reproduces CPython 3.11's behaviour exactly:
//! SipHash-1-3 with the interpreter's hash secret, the set's linear-probe table and its
//! resize rule.
//!
//! `PYTHONHASHSEED` selects the secret: 0 (or unset) is CPython's all-zero secret, any
//! other value is fed through CPython's `lcg_urandom`, so a Rust run can reproduce the
//! ordering of a reference run started with the same seed.

const LINEAR_PROBES: usize = 9;
const PERTURB_SHIFT: u32 = 5;
const MINSIZE: usize = 8;

fn rotl(value: u64, bits: u32) -> u64 {
    (value << bits) | (value >> (64 - bits))
}

#[inline]
fn half(a: u64, b: u64, c: u64, d: u64, s: u32, t: u32) -> (u64, u64, u64, u64) {
    let a = a.wrapping_add(b);
    let c = c.wrapping_add(d);
    let b = rotl(b, s) ^ a;
    let d = rotl(d, t) ^ c;
    (rotl(a, 32), b, c, d)
}

/// One CPython `SINGLE_ROUND`: HALF_ROUND(v0,v1,v2,v3,13,16) then HALF_ROUND(v2,v1,v0,v3,17,21).
#[inline]
fn single(v0: u64, v1: u64, v2: u64, v3: u64) -> (u64, u64, u64, u64) {
    let (v0, v1, v2, v3) = half(v0, v1, v2, v3, 13, 16);
    let (a, b, c, d) = half(v2, v1, v0, v3, 17, 21);
    (c, b, a, d)
}

/// SipHash-1-3 exactly as CPython's `siphash13()` implements it.
pub fn siphash13(data: &[u8], k0: u64, k1: u64) -> u64 {
    let mut v0 = k0 ^ 0x736f6d6570736575;
    let mut v1 = k1 ^ 0x646f72616e646f6d;
    let mut v2 = k0 ^ 0x6c7967656e657261;
    let mut v3 = k1 ^ 0x7465646279746573;

    let length = data.len();
    let mut offset = 0usize;
    while length - offset >= 8 {
        let mut word = [0u8; 8];
        word.copy_from_slice(&data[offset..offset + 8]);
        let mi = u64::from_le_bytes(word);
        v3 ^= mi;
        let (a, b, c, d) = single(v0, v1, v2, v3);
        v0 = a;
        v1 = b;
        v2 = c;
        v3 = d;
        v0 ^= mi;
        offset += 8;
    }

    let mut tail = [0u8; 8];
    tail[..length - offset].copy_from_slice(&data[offset..]);
    let b = ((length as u64) << 56) | u64::from_le_bytes(tail);

    v3 ^= b;
    let (a, bb, c, d) = single(v0, v1, v2, v3);
    v0 = a;
    v1 = bb;
    v2 = c;
    v3 = d;
    v0 ^= b;
    v2 ^= 0xff;
    for _ in 0..3 {
        let (a, bb, c, d) = single(v0, v1, v2, v3);
        v0 = a;
        v1 = bb;
        v2 = c;
        v3 = d;
    }
    v0 ^ v1 ^ v2 ^ v3
}

/// The bytes CPython hashes for a `str`: its internal `len * kind` byte buffer.
fn str_internal_bytes(text: &str) -> Vec<u8> {
    let points: Vec<u32> = text.chars().map(|ch| ch as u32).collect();
    if points.iter().all(|point| *point < 0x100) {
        points.iter().map(|point| *point as u8).collect()
    } else if points.iter().all(|point| *point < 0x10000) {
        points.iter().flat_map(|point| (*point as u16).to_le_bytes()).collect()
    } else {
        points.iter().flat_map(|point| point.to_le_bytes()).collect()
    }
}

/// CPython's `hash(str)` for the given hash secret.
pub fn hash_str(text: &str, k0: u64, k1: u64) -> u64 {
    let hash = siphash13(&str_internal_bytes(text), k0, k1);
    if hash == u64::MAX {
        u64::MAX - 1
    } else {
        hash
    }
}

/// CPython's `lcg_urandom()`, which turns `PYTHONHASHSEED` into a hash secret.
pub fn hash_secret_from_seed(seed: u64) -> (u64, u64) {
    let mut state = seed;
    let mut secret = [0u8; 16];
    for chunk in secret.chunks_mut(4) {
        state = state.wrapping_mul(214013).wrapping_add(2531011) & 0xffff_ffff;
        let bytes = (state as u32).to_le_bytes();
        chunk.copy_from_slice(&bytes[..chunk.len()]);
    }
    let k0 = u64::from_le_bytes(secret[0..8].try_into().unwrap());
    let k1 = u64::from_le_bytes(secret[8..16].try_into().unwrap());
    (k0, k1)
}

/// The hash secret for a `PYTHONHASHSEED` value, matching CPython's defaults.
pub fn secret_from_env(value: Option<&str>) -> (u64, u64) {
    match value {
        None | Some("") => (0, 0),
        Some(text) => match text.parse::<u64>() {
            Ok(0) => (0, 0),
            Ok(seed) => hash_secret_from_seed(seed),
            Err(_) => (0, 0),
        },
    }
}

/// Insert a key into a CPython-style table: `set_insert_clean()`.
fn insert_clean(table: &mut [Option<usize>], mask: usize, index: usize, hash: u64) {
    let mut i = (hash as usize) & mask;
    let mut perturb = hash;
    loop {
        if table[i].is_none() {
            table[i] = Some(index);
            return;
        }
        if i + LINEAR_PROBES <= mask {
            let mut step = 0usize;
            while step < LINEAR_PROBES {
                step += 1;
                i += 1;
                if table[i].is_none() {
                    table[i] = Some(index);
                    return;
                }
            }
        }
        perturb >>= PERTURB_SHIFT;
        i = (i * 5 + 1 + perturb as usize) & mask;
    }
}

/// A model of CPython's `set` that yields the same iteration order.
pub struct PySet {
    table: Vec<Option<usize>>,
    hashes: Vec<u64>,
    keys: Vec<String>,
    mask: usize,
    fill: usize,
    used: usize,
    k0: u64,
    k1: u64,
}

impl PySet {
    pub fn new(k0: u64, k1: u64) -> Self {
        PySet {
            table: vec![None; MINSIZE],
            hashes: Vec::new(),
            keys: Vec::new(),
            mask: MINSIZE - 1,
            fill: 0,
            used: 0,
            k0,
            k1,
        }
    }

    pub fn contains(&self, key: &str) -> bool {
        self.keys.iter().any(|existing| existing == key)
    }

    /// `set.add(key)` - `set_add_entry()` with no deleted slots.
    pub fn insert(&mut self, key: &str) {
        if self.contains(key) {
            return;
        }
        let mut hash = hash_str(key, self.k0, self.k1);
        if hash == 0 {
            hash = 1;
        }
        self.keys.push(key.to_string());
        self.hashes.push(hash);
        let index = self.keys.len() - 1;

        let mut i = (hash as usize) & self.mask;
        let mut perturb = hash;
        loop {
            let probes = if i + LINEAR_PROBES <= self.mask { LINEAR_PROBES } else { 0 };
            let mut remaining = probes as isize;
            let mut j = i;
            loop {
                if self.table[j].is_none() {
                    self.table[j] = Some(index);
                    self.fill += 1;
                    self.used += 1;
                    if self.fill * 5 < self.mask * 3 {
                        return;
                    }
                    let minused = if self.used > 50000 { self.used * 2 } else { self.used * 4 };
                    self.resize(minused);
                    return;
                }
                if remaining == 0 {
                    break;
                }
                remaining -= 1;
                j += 1;
            }
            perturb >>= PERTURB_SHIFT;
            i = (i * 5 + 1 + perturb as usize) & self.mask;
        }
    }

    /// `set_table_resize()`.
    fn resize(&mut self, minused: usize) {
        let mut newsize = MINSIZE;
        while newsize <= minused {
            newsize <<= 1;
        }
        let old = std::mem::replace(&mut self.table, vec![None; newsize]);
        self.mask = newsize - 1;
        self.fill = self.used;
        for entry in old.into_iter().flatten() {
            let hash = self.hashes[entry];
            insert_clean(&mut self.table, self.mask, entry, hash);
        }
    }

    /// The order `list(set)` would produce: ascending table slot.
    pub fn iter(&self) -> Vec<String> {
        self.table.iter().flatten().map(|index| self.keys[*index].clone()).collect()
    }
}

/// `list(set(items))` with CPython's ordering.
pub fn set_order(items: &[String], k0: u64, k1: u64) -> Vec<String> {
    let mut set = PySet::new(k0, k1);
    for item in items {
        set.insert(item);
    }
    set.iter()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn siphash_matches_cpython_for_seed_zero() {
        // Values printed by CPython 3.11.15 with PYTHONHASHSEED=0.
        let cases = [
            ("cosy-fix", 505831266368816629u64),
            ("cosy", 10937482595783418607),
            ("agile", 12365232839462557772),
            ("go", 10645139858423779437),
            ("flexible", 17965073166173226059),
            ("zdoc9is0j8", 2211519025859178424),
        ];
        for (text, expected) in cases {
            assert_eq!(hash_str(text, 0, 0), expected, "hash of {:?}", text);
        }
    }

    #[test]
    fn set_order_matches_cpython() {
        let ids = ["cosy-fix", "cosy", "agile", "go", "flexible"]
            .iter()
            .map(|item| item.to_string())
            .collect::<Vec<_>>();
        assert_eq!(
            set_order(&ids, 0, 0),
            vec!["flexible", "agile", "go", "cosy", "cosy-fix"]
                .iter()
                .map(|item| item.to_string())
                .collect::<Vec<_>>()
        );
    }
}
