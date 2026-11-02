//! Purpose:
//! Hash-map and hash-set aliases for the compiler's INTERNAL keying, using a fast
//! non-cryptographic hasher instead of std's SipHash.
//!
//! Called from:
//! - `crate::optimize::reachability` and the other phases a sampled profile puts SipHash on top of.
//!
//! Key details:
//! - Same API as `std::collections::HashMap`/`HashSet` except that there is no `new()`:
//!   construct with `FastMap::default()`, which is what `HashMap::new()` does anyway.
//! - NOT for anything that reaches a user or a file. See the safety argument below.

use std::hash::BuildHasherDefault;

use rustc_hash::FxHasher;

/// A `HashMap` keyed with `FxHasher` rather than SipHash.
///
/// WHY. A sampled profile of one Symfony `--web` build (release compiler, `sample -mayDie` at
/// 1 ms, four windows) put `core::hash::sip::Hasher::write` at the TOP of the leaf frames in every
/// window: 4,164 leaf samples in the pruning/lowering window against 617 for `seed_live_methods`,
/// the hottest elephc frame beside it, with `hash_one::<String>` at 682 and `String::clone` at 412.
/// The compiler keys almost everything on `String` and `(String, String, bool)` — class names,
/// method triples, symbol keys — and paid a cryptographic hash on every probe. `FxHasher` is what
/// rustc uses for the same shape of key.
///
/// WHY IT IS SAFE TO SWAP, and this is the part that has to be argued rather than assumed, because
/// changing a hasher changes ITERATION ORDER. It is safe because the order is ALREADY not
/// deterministic: std's `RandomState` seeds every map separately, so two runs of the compiler —
/// or two maps in one run — iterate differently. A deterministic hasher only replaces a random
/// order with a fixed one; it cannot make an order-independent output order-dependent.
///
/// This comment once went further and called the emitted assembly order-independent, because six
/// `--emit-asm` runs of one fixture were `cmp`-identical. The fixture was too small: two builds of
/// the Symfony app differed in three places that emitted in a map's order — the loop storage
/// contracts, the extern declarations and the source-activation table. They sort now, and
/// `ir_lower::tests::lowering_is_deterministic_across_runs` guards the first.
///
/// The one place order is known to leak today is diagnostics: two runs emit the same "Unused
/// variable" warnings in different sequence. That is a pre-existing defect, it is why gates in
/// this repository compare SORTED stderr, and it is unaffected by which hasher produced the order.
/// If a test starts failing on this swap it has found an order-dependent OUTPUT, which is a bug to
/// fix where the output is produced — by sorting there — and never by keeping SipHash.
///
/// NOT FOR HASHING UNTRUSTED INPUT. `FxHasher` is not collision-resistant. Every key here is a
/// symbol the compiler itself derived from a source file it is compiling; there is no adversary
/// choosing keys to degrade a lookup, and a program crafted to collide would only slow its own
/// compilation.
pub type FastMap<K, V> = std::collections::HashMap<K, V, BuildHasherDefault<FxHasher>>;

/// A `HashSet` keyed with `FxHasher` rather than SipHash. See [`FastMap`] for the argument.
pub type FastSet<T> = std::collections::HashSet<T, BuildHasherDefault<FxHasher>>;
