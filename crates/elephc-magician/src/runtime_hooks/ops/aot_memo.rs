//! Purpose:
//! Memoizes the generated runtime's AOT reflection lookups, which are linear table scans.
//!
//! Called from:
//! - `super::reflection`, around each `__elephc_eval_reflection_*` bridge call.
//!
//! Key details:
//! - Only compile-time-constant answers are cached; nothing here observes runtime state.

use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::BuildHasherDefault;
use std::thread::LocalKey;

use rustc_hash::FxHasher;

/// One memo table: a `class\0member` key to the answer the generated table gave.
///
/// FxHash rather than SipHash: the keys are compile-time names, nothing adversarial, and a HIT
/// is the hot path -- a Symfony request makes several hundred of them, and SipHash plus a
/// per-character key build made the hit cost more than the answer is worth.
pub(super) type MemoTable<V> = RefCell<HashMap<String, V, BuildHasherDefault<FxHasher>>>;

/// Declares a thread-local memo table for one reflection query.
macro_rules! memo_table {
    ($name:ident, $value:ty) => {
        thread_local! {
            pub(super) static $name: MemoTable<$value> = RefCell::new(HashMap::default());
        }
    };
}

memo_table!(METHOD_FLAGS, Option<u64>);
memo_table!(METHOD_DECLARING_CLASS, Option<String>);
memo_table!(PROPERTY_FLAGS, Option<u64>);
memo_table!(PROPERTY_DECLARING_CLASS, Option<String>);
memo_table!(CLASS_FLAGS, Option<u64>);
memo_table!(CLASS_SOURCE_FILE, Option<String>);
memo_table!(CANONICAL_CLASS_NAME, Option<String>);
// Whether the generated class, interface, trait or enum table names a type, keyed by name and a
// `#class`/`#interface`/`#trait`/`#enum` tag. Each probe is a whole-table scan with a
// `__rt_strcasecmp` per same-length row, and autoloading asks it for every class it resolves.
memo_table!(KIND_EXISTS, bool);
// Whole member-name lists, keyed by class and an `AotMemberNameKind` tag. The value is owned Rust
// strings copied out of the generated array: no runtime handle and no arena pointer survives here,
// so nothing in this table can outlive -- or alias -- the request storage the `--web` boundary
// wipes.
memo_table!(MEMBER_NAMES, Vec<String>);

/// Answers an AOT name-list query (methods, properties, interfaces, traits) as a FRESH PHP list,
/// scanning the generated table at most once per class.
///
/// Each scan walks the whole table with a `__rt_strcasecmp` per row, and constructing a single
/// reflector asks for several lists: a third of every `new ReflectionProperty` went there, a
/// dozen times per Symfony request. The list is a constant of the program; only the returned
/// array is new, so every caller still owns and may mutate what it gets.
pub(super) fn memoized_name_array<V: crate::interpreter::RuntimeValueOps>(
    values: &mut V,
    tag: &'static str,
    class_name: &str,
    scan: impl FnOnce() -> Result<crate::value::RuntimeCellHandle, crate::errors::EvalStatus>,
) -> Result<crate::value::RuntimeCellHandle, crate::errors::EvalStatus> {
    let names = memoized(&MEMBER_NAMES, class_name, tag, || {
        let array = scan()?;
        let names = name_array_to_vec(values, array);
        let released = values.release(array);
        let names = names?;
        released?;
        Ok(names)
    })?;
    let mut array = values.array_new(names.len())?;
    for (position, name) in names.iter().enumerate() {
        let key = values.int(position as i64)?;
        let value = values.string(name)?;
        let stored = values.array_set(array, key, value);
        let _ = values.release(value);
        let _ = values.release(key);
        array = stored?;
    }
    Ok(array)
}

/// Copies a generated name list into owned strings, releasing every handle it reads.
fn name_array_to_vec<V: crate::interpreter::RuntimeValueOps>(
    values: &mut V,
    array: crate::value::RuntimeCellHandle,
) -> Result<Vec<String>, crate::errors::EvalStatus> {
    let len = values.array_len(array)?;
    let mut names = Vec::with_capacity(len);
    for position in 0..len {
        let key = values.int(position as i64)?;
        let value = values.array_get(array, key);
        let _ = values.release(key);
        let value = value?;
        let bytes = values.string_bytes(value);
        let _ = values.release(value);
        names.push(String::from_utf8(bytes?).map_err(|_| crate::errors::EvalStatus::RuntimeFatal)?);
    }
    Ok(names)
}

/// Builds the lookup key. A class name folds case, as PHP resolves it; the member half folds
/// only when `fold_member` says PHP does (methods), because property names are case-SENSITIVE:
/// `$x` and `$X` are two properties, and folding them let the first answer stand for both.
///
/// The NUL separator cannot occur in a PHP class or member name, so `A\0b` and `A\0` + `b`
/// are the only way two distinct pairs could collide, and neither is expressible. The fold is
/// byte-wise: `to_ascii_lowercase` leaves every non-ASCII byte alone, so a UTF-8 name stays
/// valid without decoding it character by character.
fn memo_key(class_name: &str, member_name: &str, fold_member: bool) -> String {
    let mut key = String::with_capacity(class_name.len() + member_name.len() + 1);
    key.push_str(class_name);
    key.push('\0');
    key.push_str(member_name);
    let fold_len = if fold_member { key.len() } else { class_name.len() };
    key[..fold_len].make_ascii_lowercase();
    key
}

/// Whether the memo is active, so one binary can be measured with it and without it.
///
/// A memo that removes work can still lose, and comparing two *builds* cannot show that: the
/// generated program differs between them, which is how a 25% win first read as a 2x regression
/// here. One binary answering both ways is the only clean A/B, and it stays for the next one --
/// `ELEPHC_AOT_REFLECTION_MEMO=0` restores the unmemoized lookups.
fn memo_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| {
        !matches!(
            std::env::var("ELEPHC_AOT_REFLECTION_MEMO").as_deref(),
            Ok("0") | Ok("off") | Ok("false")
        )
    })
}

/// Answers one AOT reflection query from the memo, calling the generated table at most once.
///
/// Every `__elephc_eval_reflection_*` entry point walks its metadata table row by row and calls
/// `__rt_strcasecmp` on each candidate. The table is a compile-time constant -- one row per
/// method or property the binary carries -- so a whole-framework build turns every query into a
/// scan of tens of thousands of rows. Profiling a Symfony `--web` worker put **39% of a rendered
/// request** inside those five scans and the case-insensitive compares they drive.
///
/// The tables cannot change while the process runs: they live in read-only generated data, and
/// the bridge entry points read nothing else. That makes each query a pure function of its two
/// names, so the first answer is the only answer, and it is kept for the life of the thread.
/// Classes declared at runtime are absent from the tables and stay absent, so a cached miss is
/// as durable as a cached hit -- the callers that must also see runtime classes consult runtime
/// state themselves, before or after asking here.
///
/// Errors are never cached: an `EvalStatus` reports the state of the *call*, not of the table.
pub(super) fn memoized<V, E>(
    cache: &'static LocalKey<MemoTable<V>>,
    class_name: &str,
    member_name: &str,
    compute: impl FnOnce() -> Result<V, E>,
) -> Result<V, E>
where
    V: Clone,
{
    memoized_with_fold(cache, class_name, member_name, true, compute)
}

/// [`memoized`] for a type-EXISTENCE query, keeping only a positive answer.
///
/// Whether a compiled interface (or probe-only class) exists is load state, not a constant: it
/// turns true when its source activates, which an autoloader's skipped `include` of that source
/// does (`__elephc_eval_source_activate`). Symfony's `Preloader` asked
/// `ReflectionClass(DependencyInjection\ContainerInterface)` before the file was activated; the
/// cached `false` then outlived the activation, the autoloader re-included the file through
/// Composer's `vendor/composer/../` spelling, and the interpreter declared it itself and failed on
/// its compiled parent. A type never stops existing within a process, so `true` stays cached.
pub(super) fn memoized_existence<E>(
    class_name: &str,
    tag: &str,
    compute: impl FnOnce() -> Result<bool, E>,
) -> Result<bool, E> {
    if !memo_enabled() {
        return compute();
    }
    let key = memo_key(class_name, tag, true);
    if KIND_EXISTS.with(|table| table.borrow().get(&key).copied()) == Some(true) {
        return Ok(true);
    }
    let value = compute()?;
    if value {
        KIND_EXISTS.with(|table| {
            table.borrow_mut().insert(key, true);
        });
    }
    Ok(value)
}

/// [`memoized`] for a PROPERTY query: the member half of the key keeps its case.
pub(super) fn memoized_property<V, E>(
    cache: &'static LocalKey<MemoTable<V>>,
    class_name: &str,
    property_name: &str,
    compute: impl FnOnce() -> Result<V, E>,
) -> Result<V, E>
where
    V: Clone,
{
    memoized_with_fold(cache, class_name, property_name, false, compute)
}

fn memoized_with_fold<V, E>(
    cache: &'static LocalKey<MemoTable<V>>,
    class_name: &str,
    member_name: &str,
    fold_member: bool,
    compute: impl FnOnce() -> Result<V, E>,
) -> Result<V, E>
where
    V: Clone,
{
    if !memo_enabled() {
        return compute();
    }
    let key = memo_key(class_name, member_name, fold_member);
    // The borrow ends before `compute` runs: the bridge call below re-enters this crate, and a
    // borrow held across it would be a panic rather than a miss.
    if let Some(hit) = cache.with(|table| table.borrow().get(&key).cloned()) {
        return Ok(hit);
    }
    let value = compute()?;
    cache.with(|table| {
        table.borrow_mut().insert(key, value.clone());
    });
    Ok(value)
}
