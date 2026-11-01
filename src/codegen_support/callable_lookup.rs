//! Purpose:
//! Builds deterministic metadata tables for runtime callable-name resolution.
//! Replaces repeated per-site candidate ladders with compact shared data.
//!
//! Called from:
//! - `crate::codegen::lower_inst::callables` when a callable universe exceeds the inline limit.
//!
//! Key details:
//! - PHP callable names fold ASCII case and accept at most one leading namespace separator.
//! - Table order and open-address placement are deterministic and independent of hash-map order.
//! - Descriptor labels are already globally cached before collision validation runs here.

use crate::codegen_support::callable_dispatch::RuntimeCallableCase;
use crate::codegen_support::data_section::{DataSection, DataWord};

/// Largest callable universe that remains as an inline dispatch ladder.
pub(crate) const INLINE_LOOKUP_LIMIT: usize = 8;
/// Largest callable universe resolved by a compact linear metadata scan.
const LINEAR_LOOKUP_LIMIT: usize = 256;
/// Fixed FNV-1a offset basis used by emitted lookup tables and runtime helpers.
const FNV1A_OFFSET: u64 = 14_695_981_039_346_656_037;
/// Fixed FNV-1a prime used by emitted lookup tables and runtime helpers.
const FNV1A_PRIME: u64 = 1_099_511_628_211;

/// Metadata table selected for one string-callable universe.
#[derive(Clone)]
pub(crate) enum StringLookupTable {
    /// Canonical entries stored consecutively for a runtime linear scan.
    Linear { label: String, count: usize },
    /// Open-addressed buckets indexed by a deterministic FNV-1a hash.
    Hashed { label: String, mask: usize },
}

/// One public instance-method template addressable by receiver class and method name.
pub(crate) struct InstanceLookupCase {
    pub(crate) class_id: u64,
    pub(crate) method_name: String,
    pub(crate) descriptor_label: String,
}

/// One public static-method descriptor addressable by class and method names.
pub(crate) struct StaticLookupCase {
    pub(crate) class_name: String,
    pub(crate) method_name: String,
    pub(crate) descriptor_label: String,
}

/// One invokable-object template addressable by receiver class id.
pub(crate) struct InvokableLookupCase {
    pub(crate) class_id: u64,
    pub(crate) descriptor_label: String,
}

/// Canonical string-callable row before serialization into `DataSection` words.
#[derive(Clone, Debug, Eq, PartialEq)]
struct StringLookupEntry {
    key: Vec<u8>,
    descriptor_label: String,
}

/// Normalizes one PHP callable name for case-insensitive runtime lookup.
fn canonical_name(name: &str) -> Result<Vec<u8>, String> {
    let bytes = name.as_bytes();
    let bytes = bytes.strip_prefix(b"\\").unwrap_or(bytes);
    if bytes.contains(&0) {
        return Err("runtime callable name contains NUL".to_string());
    }
    Ok(bytes
        .iter()
        .map(|byte| {
            if byte.is_ascii_uppercase() {
                byte.to_ascii_lowercase()
            } else {
                *byte
            }
        })
        .collect())
}

/// Computes the fixed 64-bit FNV-1a hash used by open-addressed callable tables.
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(FNV1A_OFFSET, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(FNV1A_PRIME)
    })
}

/// Canonicalizes, sorts, and validates string-callable cases before table emission.
fn canonical_string_entries(
    cases: &[RuntimeCallableCase],
) -> Result<Vec<StringLookupEntry>, String> {
    let mut entries = Vec::with_capacity(cases.len());
    for case in cases {
        let name = case
            .php_name
            .as_deref()
            .ok_or_else(|| "runtime string callable case has no PHP name".to_string())?;
        entries.push(StringLookupEntry {
            key: canonical_name(name)?,
            descriptor_label: case.descriptor_label.clone(),
        });
    }
    entries.sort_by(|left, right| left.key.cmp(&right.key));

    let mut unique = Vec::<StringLookupEntry>::with_capacity(entries.len());
    for entry in entries {
        if let Some(previous) = unique.last() {
            if previous.key == entry.key {
                if previous.descriptor_label == entry.descriptor_label {
                    continue;
                }
                return Err(format!(
                    "runtime callable key '{}' resolves to multiple descriptors",
                    String::from_utf8_lossy(&entry.key)
                ));
            }
        }
        unique.push(entry);
    }
    Ok(unique)
}

/// Emits a deterministic linear or open-addressed table for string-callable cases.
pub(crate) fn emit_string_lookup_table(
    data: &mut DataSection,
    cases: &[RuntimeCallableCase],
) -> Result<StringLookupTable, String> {
    let entries = canonical_string_entries(cases)?;
    if entries.is_empty() {
        return Err("runtime callable metadata table has no entries".to_string());
    }
    if entries.len() <= LINEAR_LOOKUP_LIMIT {
        let count = entries.len();
        let mut words = Vec::with_capacity(entries.len() * 4);
        for entry in entries {
            let (key_label, key_len) = data.add_string(&entry.key);
            words.extend([
                DataWord::Symbol(key_label),
                DataWord::U64(key_len as u64),
                DataWord::Symbol(entry.descriptor_label),
                DataWord::U64(0),
            ]);
        }
        return Ok(StringLookupTable::Linear {
            label: data.add_words(words),
            count,
        });
    }

    let capacity = (entries.len() * 2).next_power_of_two();
    let mask = capacity - 1;
    let mut buckets = vec![None; capacity];
    for entry in entries {
        let hash = fnv1a(&entry.key);
        let mut index = hash as usize & mask;
        while buckets[index].is_some() {
            index = (index + 1) & mask;
        }
        buckets[index] = Some((hash, entry));
    }

    let mut words = Vec::with_capacity(capacity * 5);
    for bucket in buckets {
        if let Some((hash, entry)) = bucket {
            let (key_label, key_len) = data.add_string(&entry.key);
            words.extend([
                DataWord::U64(hash),
                DataWord::Symbol(key_label),
                DataWord::U64(key_len as u64),
                DataWord::Symbol(entry.descriptor_label),
                DataWord::U64(0),
            ]);
        } else {
            words.extend([
                DataWord::U64(0),
                DataWord::U64(0),
                DataWord::U64(0),
                DataWord::U64(0),
                DataWord::U64(0),
            ]);
        }
    }
    Ok(StringLookupTable::Hashed {
        label: data.add_words(words),
        mask,
    })
}

/// Emits a deterministic table keyed by receiver class id and method name.
pub(crate) fn emit_instance_lookup_table(
    data: &mut DataSection,
    cases: Vec<InstanceLookupCase>,
) -> Result<StringLookupTable, String> {
    let mut entries = cases
        .into_iter()
        .map(|case| {
            let method = canonical_name(&case.method_name)?;
            let mut key = case.class_id.to_le_bytes().to_vec();
            key.push(0);
            key.extend_from_slice(&method);
            Ok((key, case.class_id, method, case.descriptor_label))
        })
        .collect::<Result<Vec<_>, String>>()?;
    entries.sort_by(|left, right| left.0.cmp(&right.0));
    entries.dedup_by(|right, left| {
        left.0 == right.0 && left.3 == right.3
    });
    validate_unique_values(
        entries.iter().map(|entry| (&entry.0, entry.3.as_str())),
        "instance callable",
    )?;
    if entries.is_empty() {
        return Err("instance callable metadata table has no entries".to_string());
    }
    if entries.len() <= LINEAR_LOOKUP_LIMIT {
        let count = entries.len();
        let mut words = Vec::with_capacity(1 + count * 6);
        words.push(DataWord::U64(count as u64));
        for (_, class_id, method, descriptor_label) in entries {
            let (method_label, method_len) = data.add_string(&method);
            words.extend([
                DataWord::U64(class_id),
                DataWord::U64(0),
                DataWord::Symbol(method_label),
                DataWord::U64(method_len as u64),
                DataWord::Symbol(descriptor_label),
                DataWord::U64(1),
            ]);
        }
        return Ok(StringLookupTable::Linear {
            label: data.add_words(words),
            count,
        });
    }

    let capacity = (entries.len() * 2).next_power_of_two();
    let mask = capacity - 1;
    let mut buckets = vec![None; capacity];
    for entry in entries {
        let hash = fnv1a(&entry.0);
        let mut index = hash as usize & mask;
        while buckets[index].is_some() {
            index = (index + 1) & mask;
        }
        buckets[index] = Some((hash, entry));
    }
    let mut words = Vec::with_capacity(1 + capacity * 7);
    words.push(DataWord::U64(mask as u64));
    for bucket in buckets {
        if let Some((hash, (_, class_id, method, descriptor_label))) = bucket {
            let (method_label, method_len) = data.add_string(&method);
            words.extend([
                DataWord::U64(hash),
                DataWord::U64(class_id),
                DataWord::U64(0),
                DataWord::Symbol(method_label),
                DataWord::U64(method_len as u64),
                DataWord::Symbol(descriptor_label),
                DataWord::U64(1),
            ]);
        } else {
            words.extend((0..7).map(|_| DataWord::U64(0)));
        }
    }
    Ok(StringLookupTable::Hashed {
        label: data.add_words(words),
        mask,
    })
}

/// Emits a deterministic table keyed by static class and method names.
pub(crate) fn emit_static_lookup_table(
    data: &mut DataSection,
    cases: Vec<StaticLookupCase>,
) -> Result<StringLookupTable, String> {
    let mut entries = cases
        .into_iter()
        .map(|case| {
            let class = canonical_name(&case.class_name)?;
            let method = canonical_name(&case.method_name)?;
            let mut key = class.clone();
            key.push(0);
            key.extend_from_slice(&method);
            Ok((key, class, method, case.descriptor_label))
        })
        .collect::<Result<Vec<_>, String>>()?;
    entries.sort_by(|left, right| left.0.cmp(&right.0));
    entries.dedup_by(|right, left| left.0 == right.0 && left.3 == right.3);
    validate_unique_values(
        entries.iter().map(|entry| (&entry.0, entry.3.as_str())),
        "static callable",
    )?;
    if entries.is_empty() {
        return Err("static callable metadata table has no entries".to_string());
    }
    if entries.len() <= LINEAR_LOOKUP_LIMIT {
        let count = entries.len();
        let mut words = Vec::with_capacity(1 + count * 6);
        words.push(DataWord::U64(count as u64));
        for (_, class, method, descriptor_label) in entries {
            let (class_label, class_len) = data.add_string(&class);
            let (method_label, method_len) = data.add_string(&method);
            words.extend([
                DataWord::Symbol(class_label),
                DataWord::U64(class_len as u64),
                DataWord::Symbol(method_label),
                DataWord::U64(method_len as u64),
                DataWord::Symbol(descriptor_label),
                DataWord::U64(0),
            ]);
        }
        return Ok(StringLookupTable::Linear {
            label: data.add_words(words),
            count,
        });
    }

    let capacity = (entries.len() * 2).next_power_of_two();
    let mask = capacity - 1;
    let mut buckets = vec![None; capacity];
    for entry in entries {
        let hash = fnv1a(&entry.0);
        let mut index = hash as usize & mask;
        while buckets[index].is_some() {
            index = (index + 1) & mask;
        }
        buckets[index] = Some((hash, entry));
    }
    let mut words = Vec::with_capacity(1 + capacity * 7);
    words.push(DataWord::U64(mask as u64));
    for bucket in buckets {
        if let Some((hash, (_, class, method, descriptor_label))) = bucket {
            let (class_label, class_len) = data.add_string(&class);
            let (method_label, method_len) = data.add_string(&method);
            words.extend([
                DataWord::U64(hash),
                DataWord::Symbol(class_label),
                DataWord::U64(class_len as u64),
                DataWord::Symbol(method_label),
                DataWord::U64(method_len as u64),
                DataWord::Symbol(descriptor_label),
                DataWord::U64(0),
            ]);
        } else {
            words.extend((0..7).map(|_| DataWord::U64(0)));
        }
    }
    Ok(StringLookupTable::Hashed {
        label: data.add_words(words),
        mask,
    })
}

/// Emits a deterministic table keyed only by invokable receiver class id.
pub(crate) fn emit_invokable_lookup_table(
    data: &mut DataSection,
    cases: Vec<InvokableLookupCase>,
) -> Result<StringLookupTable, String> {
    let mut entries = cases
        .into_iter()
        .map(|case| {
            (
                case.class_id.to_le_bytes().to_vec(),
                case.class_id,
                case.descriptor_label,
            )
        })
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| left.0.cmp(&right.0));
    entries.dedup_by(|right, left| left.0 == right.0 && left.2 == right.2);
    validate_unique_values(
        entries.iter().map(|entry| (&entry.0, entry.2.as_str())),
        "invokable callable",
    )?;
    if entries.is_empty() {
        return Err("invokable callable metadata table has no entries".to_string());
    }
    if entries.len() <= LINEAR_LOOKUP_LIMIT {
        let count = entries.len();
        let mut words = Vec::with_capacity(1 + count * 6);
        words.push(DataWord::U64(count as u64));
        for (_, class_id, descriptor_label) in entries {
            words.extend([
                DataWord::U64(class_id),
                DataWord::U64(0),
                DataWord::U64(0),
                DataWord::U64(0),
                DataWord::Symbol(descriptor_label),
                DataWord::U64(1),
            ]);
        }
        return Ok(StringLookupTable::Linear {
            label: data.add_words(words),
            count,
        });
    }

    let capacity = (entries.len() * 2).next_power_of_two();
    let mask = capacity - 1;
    let mut buckets = vec![None; capacity];
    for entry in entries {
        let hash = fnv1a(&entry.0);
        let mut index = hash as usize & mask;
        while buckets[index].is_some() {
            index = (index + 1) & mask;
        }
        buckets[index] = Some((hash, entry));
    }
    let mut words = Vec::with_capacity(1 + capacity * 7);
    words.push(DataWord::U64(mask as u64));
    for bucket in buckets {
        if let Some((hash, (_, class_id, descriptor_label))) = bucket {
            words.extend([
                DataWord::U64(hash),
                DataWord::U64(class_id),
                DataWord::U64(0),
                DataWord::U64(0),
                DataWord::U64(0),
                DataWord::Symbol(descriptor_label),
                DataWord::U64(1),
            ]);
        } else {
            words.extend((0..7).map(|_| DataWord::U64(0)));
        }
    }
    Ok(StringLookupTable::Hashed {
        label: data.add_words(words),
        mask,
    })
}

/// Rejects one canonical key that maps to distinct cached descriptor identities.
fn validate_unique_values<'a>(
    entries: impl Iterator<Item = (&'a Vec<u8>, &'a str)>,
    shape: &str,
) -> Result<(), String> {
    let mut previous: Option<(&[u8], &str)> = None;
    for (key, descriptor_label) in entries {
        if let Some((previous_key, previous_label)) = previous {
            if previous_key == key.as_slice() && previous_label != descriptor_label {
                return Err(format!("{} key resolves to multiple descriptors", shape));
            }
        }
        previous = Some((key.as_slice(), descriptor_label));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{canonical_name, canonical_string_entries, fnv1a};
    use crate::codegen_support::callable_dispatch::RuntimeCallableCase;

    /// Builds one minimal named descriptor case for table-builder unit tests.
    fn named_case(name: &str, descriptor_label: &str) -> RuntimeCallableCase {
        RuntimeCallableCase {
            descriptor_label: descriptor_label.to_string(),
            php_name: Some(name.to_string()),
        }
    }

    /// Verifies lookup normalization folds ASCII case and removes only one leading separator.
    #[test]
    fn canonical_name_matches_runtime_rules() {
        assert_eq!(canonical_name("\\Foo\\BAR").unwrap(), b"foo\\bar");
        assert_eq!(canonical_name("\\\\Foo").unwrap(), b"\\foo");
    }

    /// Verifies canonical rows sort by key and merge only identical descriptor identities.
    #[test]
    fn canonical_entries_are_sorted_and_collision_safe() {
        let cases = vec![
            named_case("Zulu", "descriptor_z"),
            named_case("\\alpha", "descriptor_a"),
            named_case("ALPHA", "descriptor_a"),
        ];
        let entries = canonical_string_entries(&cases).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].key, b"alpha");
        assert_eq!(entries[1].key, b"zulu");

        let conflicting = vec![
            named_case("alpha", "descriptor_a"),
            named_case("ALPHA", "descriptor_b"),
        ];
        assert!(canonical_string_entries(&conflicting).is_err());
    }

    /// Pins the fixed FNV-1a implementation used by the runtime hash resolver.
    #[test]
    fn fnv1a_matches_known_vector() {
        assert_eq!(fnv1a(b"hello"), 0xa430_d846_80aa_bd0b);
    }
}
