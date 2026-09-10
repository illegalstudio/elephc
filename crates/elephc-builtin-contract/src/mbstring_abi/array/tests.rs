//! Purpose:
//! Pins mbstring array wire framing independently of its encoder and checks malformed inputs.
//!
//! Called from:
//! - The neutral builtin contract's focused test harness.
//!
//! Key details:
//! - Binary keys, numeric strings, float bits, aliases, and cycles must survive unchanged.
//! - Invalid metadata fails before consumers can traverse any graph identity.

use super::{ArrayGraph, Key, Value};

/// Creates the byte representation of independently specified little-endian words.
fn words(values: &[u64]) -> Vec<u8> { values.iter().flat_map(|value| value.to_le_bytes()).collect() }

/// Pins a complete graph with an empty node and a self-reference to independent wire words.
#[test]
fn mbstring_array_wire_layout() {
    let input = words(&[1, 2, 0, 1, 1, u64::MAX, 4, 1]);
    let graph = ArrayGraph::new(1, vec![vec![], vec![(Key::Int(-1), Value::Array(1))]]).unwrap();
    assert_eq!(ArrayGraph::decode(&input), Some(graph.clone()));
    assert_eq!(graph.encode(), input);
    assert_eq!(graph.into_compact(), ArrayGraph::new(0, vec![vec![(Key::Int(-1), Value::Array(0))]]).unwrap());
    let mut input = words(&[0, 1, 1, 2, 3]);
    input.extend_from_slice(b"a\0b");
    input.extend_from_slice(&words(&[2, 2]));
    input.extend_from_slice(b"\xff\x80");
    let graph = ArrayGraph::new(0, vec![vec![(Key::String(b"a\0b".to_vec()), Value::String(b"\xff\x80".to_vec()))]]).unwrap();
    assert_eq!(ArrayGraph::decode(&input), Some(graph.clone()));
    assert_eq!(graph.encode(), input);
}

/// Verifies every scalar shape and shared identity, including unaligned binary string cells.
#[test]
fn mbstring_array_wire_preserves_values() {
    let graph = ArrayGraph::new(0, vec![vec![
        (Key::Int(0), Value::Null),
        (Key::String(b"0".to_vec()), Value::Bool(false)),
        (Key::String(b"\xff\0".to_vec()), Value::String(b"\x80\0x".to_vec())),
        (Key::Int(1), Value::Int(i64::MIN)),
        (Key::Int(2), Value::Float(0x8000000000000000)),
        (Key::Int(3), Value::Float(0x7ff8000000000042)),
        (Key::Int(4), Value::Bool(true)),
        (Key::Int(5), Value::Unsupported),
        (Key::Int(6), Value::Array(1)),
        (Key::Int(7), Value::Array(1)),
        (Key::Int(8), Value::String(vec![])),
    ], vec![(Key::String(vec![]), Value::Array(0))]]).unwrap();
    let encoded = graph.encode();
    assert_eq!(ArrayGraph::decode(&encoded), Some(graph));
    for length in 0..encoded.len() { assert_eq!(ArrayGraph::decode(&encoded[..length]), None, "truncation {length}"); }
    let mut trailing = encoded;
    trailing.push(0);
    assert_eq!(ArrayGraph::decode(&trailing), None);
}

/// Rejects allocation-sized counts, dangling nodes, invalid tags, reserved payloads, and duplicates.
#[test]
fn mbstring_array_wire_rejects_malformed_metadata() {
    for input in [
        vec![0, 0], vec![1, 1, 0], vec![0, u64::MAX], vec![0, 1, u64::MAX],
        vec![0, 1, 1, 1, 0, 4, 1], vec![0, 1, 1, 1, 0, 99, 0],
        vec![0, 1, 1, 1, 0, 0, 1], vec![0, 1, 1, 1, 0, 6, 1],
        vec![0, 1, 1, 1, 0, 3, 2], vec![0, 1, 1, 3, 0, 0, 0],
        vec![0, 1, 1, 1, 0, 2, u64::MAX], vec![0, 1, 1, 2, u64::MAX, 0, 0],
        vec![0, 1, 2, 1, 0, 0, 0, 1, 0, 0, 0],
    ] {
        assert_eq!(ArrayGraph::decode(&words(&input)), None, "words {input:?}");
    }
}
