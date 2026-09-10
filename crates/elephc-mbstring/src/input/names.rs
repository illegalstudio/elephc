//! Purpose:
//! Plans PHP query-name registration independently of native or eval array storage.
//!
//! Called from:
//! - `super::Variables::register()` and its independent PHP query oracles.
//!
//! Key details:
//! - Steps preserve partial parent creation before rejected keys or nesting overflow.
//! - Numeric normalization is shared; hosts own live append counters, COW, and destructors.
//! - Every plan begins at the current output root and stops if a host cannot enter a child.

use crate::arrays::Key;

/// One ordered mutation relative to the output table selected by preceding enter steps.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RegistrationStep {
    /// Resolves or creates an array child; None appends without overwriting an occupied index.
    Enter(Option<Key>),
    /// Writes the pair's binary value in the current table; None requests an append key.
    Store(Option<Key>),
    /// Removes this root key after a nesting failure, even if earlier steps created children.
    RemoveRoot(Key),
}

/// Normalizes a converted name and preserves the ordered mutations performed before any rejection.
/// Hosts execute against live output storage, retaining PHP's partial changes and append failures.
pub fn registration(name: &[u8], max_nesting: i64) -> Vec<RegistrationStep> {
    let original = super::c_string(name);
    let leading = original.iter().take_while(|&&byte| byte == b' ').count();
    let original = &original[leading..];
    let mut name = original.to_vec();
    let bracket = name.iter().position(|&byte| byte == b'[');
    let root_end = bracket.unwrap_or(name.len());
    for byte in &mut name[..root_end] { if matches!(*byte, b' ' | b'.') { *byte = b'_'; } }
    let mut steps = Vec::new();
    if root_end == 0 { return steps; }
    let root = key(&name[..root_end]);
    let Some(mut cursor) = bracket else {
        push(&mut steps, Some(&name), original, false);
        return steps;
    };
    name[cursor] = 0;
    let (mut index, mut depth) = (Some(0), 0i64);
    loop {
        depth += 1;
        if depth > max_nesting {
            steps.push(RegistrationStep::RemoveRoot(root));
            return steps;
        }
        let start = cursor + 1;
        let probe = start + usize::from(name.get(start).is_some_and(|byte| matches!(byte, b' ' | b'\t'..=b'\r')));
        let close = name[probe..].iter().position(|&byte| byte == b']').map(|offset| probe + offset);
        let Some(close) = close else {
            name[start - 1] = b'_';
            for byte in &mut name[start..] { if matches!(*byte, b' ' | b'.' | b'[') { *byte = b'_'; } }
            push(&mut steps, index.map(|start| super::c_string(&name[start..])), original, false);
            return steps;
        };
        let next_index = (close != probe).then_some(start);
        name[close] = 0;
        let current = index.map(|start| super::c_string(&name[start..]));
        if !push(&mut steps, current, original, true) { return steps; }
        index = next_index;
        cursor = close + 1;
        if name.get(cursor) != Some(&b'[') {
            push(&mut steps, index.map(|start| super::c_string(&name[start..])), original, false);
            return steps;
        }
        name[cursor] = 0;
    }
}

/// Adds a normalized enter/store operation unless mangling introduced a forbidden prefix.
fn push(steps: &mut Vec<RegistrationStep>, name: Option<&[u8]>, original: &[u8], enter: bool) -> bool {
    if name.is_some_and(|name| forbidden(name, original)) { return false; }
    let key = name.map(key);
    steps.push(if enter { RegistrationStep::Enter(key) } else { RegistrationStep::Store(key) });
    true
}

/// Normalizes only canonical signed decimal integers fitting PHP's supported 64-bit integer type.
fn key(name: &[u8]) -> Key {
    if let Ok(text) = std::str::from_utf8(name) {
        if let Ok(integer) = text.parse::<i64>() {
            if integer.to_string().as_bytes() == name { return Key::Int(integer); }
        }
    }
    Key::String(name.to_vec())
}

/// Rejects cookie-prefix names introduced by normalization or nested keys of another root name.
fn forbidden(name: &[u8], original: &[u8]) -> bool {
    [b"__Host-".as_slice(), b"__Secure-".as_slice()].iter().any(|prefix| name.starts_with(prefix) && !original.starts_with(prefix))
}
