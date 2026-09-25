//! Purpose:
//! Converts the value graph of mb_convert_variables with one shared source encoding.
//!
//! Called from:
//! - The mbstring variable-reference invocation adapter.
//!
//! Key details:
//! - Detection sees every string value in PHP traversal order but never array keys.
//! - Conversion preserves array identities and key bytes, and rejects active cycles.
//! - The caller owns writeback; this module returns a detached graph and illegal-unit count.

use std::collections::HashSet;

use elephc_builtin_contract::mbstring_abi::array::{ArrayGraph, Value};

use crate::detect;
use crate::encoding::{self, Encoding, Substitute};

/// A failure that PHP reports as a warning and a false result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure { Recursive, Undetectable }

/// The common source and detached converted values, plus request error accounting.
#[derive(Debug)]
pub struct Conversion {
    pub result: Result<(Encoding, ArrayGraph), Failure>,
    pub illegal_chars: u64,
}

/// Converts one graph whose root entries are the caller's by-reference variables.
/// Every reachable string value participates in a single source guess; keys are untouched.
pub fn convert(
    graph: &ArrayGraph, to: Encoding, candidates: &[Encoding], strict: bool,
    order_significant: bool, substitute: Substitute,
) -> Conversion {
    let result = collect_strings(graph).and_then(|strings| {
        let from = match candidates {
            [only] => *only,
            many => detect::guess_many(&strings, many, strict, order_significant)
                .ok_or(Failure::Undetectable)?,
        };
        Ok(from)
    });
    let (result, illegal_chars) = encoding::errors::measure(|| {
        result.and_then(|from| convert_values(graph, from, to, substitute).map(|graph| (from, graph)))
    });
    Conversion { result, illegal_chars }
}

/// Collects borrowed string values through references while rejecting active array cycles.
fn collect_strings(graph: &ArrayGraph) -> Result<Vec<&[u8]>, Failure> {
    let mut strings = Vec::new();
    let mut active = HashSet::new();
    let mut stack = vec![(graph.root(), 0usize)];
    active.insert(graph.root());
    while let Some((array, position)) = stack.last_mut() {
        let Some((_, value)) = graph.arrays()[*array].get(*position) else {
            active.remove(array);
            stack.pop();
            continue;
        };
        *position += 1;
        match value {
            Value::String(bytes) => strings.push(bytes.as_slice()),
            Value::Array(child) => {
                if !active.insert(*child) { return Err(Failure::Recursive); }
                stack.push((*child, 0));
            }
            _ => {},
        }
    }
    Ok(strings)
}

/// Changes each reachable string exactly once so aliases retain one converted array identity.
fn convert_values(
    graph: &ArrayGraph, from: Encoding, to: Encoding, substitute: Substitute,
) -> Result<ArrayGraph, Failure> {
    let mut arrays = graph.arrays().to_vec();
    let mut active = HashSet::from([graph.root()]);
    let mut converted = HashSet::new();
    let mut stack = vec![(graph.root(), 0usize)];
    while let Some((array, position)) = stack.last_mut() {
        let Some((_, value)) = arrays[*array].get_mut(*position) else {
            active.remove(array);
            converted.insert(*array);
            stack.pop();
            continue;
        };
        *position += 1;
        match value {
            Value::String(bytes) => *bytes = to.encode_conversion(bytes, from, substitute),
            Value::Array(child) if active.contains(child) => return Err(Failure::Recursive),
            Value::Array(child) if !converted.contains(child) => {
                let child = *child;
                active.insert(child);
                stack.push((child, 0));
            }
            _ => {},
        }
    }
    Ok(ArrayGraph::new(graph.root(), arrays).expect("converted values keep graph structure"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use elephc_builtin_contract::mbstring_abi::array::Key;

    /// Uses one source for all variables and leaves keys in their original byte encoding.
    #[test]
    fn values_share_detection_and_keep_keys() {
        let graph = ArrayGraph::new(0, vec![
            vec![(Key::Int(0), Value::Array(1)), (Key::Int(1), Value::String(b"plain".to_vec()))],
            vec![(Key::String(vec![0xe9]), Value::String(vec![0xe9]))],
        ]).unwrap();
        let utf8 = Encoding::lookup(b"UTF-8").unwrap();
        let latin1 = Encoding::lookup(b"ISO-8859-1").unwrap();
        let converted = convert(&graph, utf8, &[utf8, latin1], true, true, Substitute::default());
        let (from, graph) = converted.result.unwrap();
        assert_eq!(from, latin1);
        assert_eq!(graph.arrays()[1][0].0, Key::String(vec![0xe9]));
        assert_eq!(graph.arrays()[1][0].1, Value::String("é".as_bytes().to_vec()));
    }

    /// Rejects a recursive value graph before exposing a partial conversion.
    #[test]
    fn active_cycle_is_rejected() {
        let graph = ArrayGraph::new(0, vec![vec![(Key::Int(0), Value::Array(0))]]).unwrap();
        let utf8 = Encoding::lookup(b"UTF-8").unwrap();
        assert_eq!(convert(&graph, utf8, &[utf8], false, true, Substitute::default()).result.err(),
            Some(Failure::Recursive));
    }
}
