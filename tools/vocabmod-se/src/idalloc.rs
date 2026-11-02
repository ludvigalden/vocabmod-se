//! Export-time custom-id allocation: a stable hash of
//! the system and concept code lands inside the system's authored band,
//! collisions resolved by deterministic rehash. Ids are a pure function of
//! the tree, the constants registry, and the fixed allocation order
//! (concepts sorted by system then code) — derived at export time only,
//! never written back, so exported ids stay stable under insertions.

use std::collections::HashSet;

/// The hash's domain separator: a hash value never crosses domains.
const DOMAIN: &[u8] = b"vocabmod-se omop custom id v1\0";

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

fn fnv1a64(state: u64, bytes: &[u8]) -> u64 {
    let mut hash = state;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

/// The stable id for one id-less concept: `first + (hash of system and
/// code) mod span`, rehashing past ids already taken. The taken set makes
/// the result independent of any other consumer's numbering while keeping
/// the sequence reproducible for the fixed allocation order.
pub fn stable_id(band: (u64, u64), system: &str, code: &str, taken: &mut HashSet<u64>) -> u64 {
    let (first, last) = band;
    let span = last - first + 1;
    let mut hash = fnv1a64(fnv1a64(FNV_OFFSET, DOMAIN), system.as_bytes());
    hash = fnv1a64(hash, b"\0");
    hash = fnv1a64(hash, code.as_bytes());
    let mut id = first + hash % span;
    while !taken.insert(id) {
        hash = fnv1a64(hash, b"\0rehash");
        id = first + hash % span;
    }
    id
}
