//! Purpose:
//! Converts recursive PHP array graphs with independent key/value source detection.
//!
//! Called from:
//! - The shared array-capable mb_convert_encoding adapter.
//!
//! Key details:
//! - Every visited string is converted before collision handling; the first key wins.
//! - PHP's recursion protection is cleared by a recursive conversion failure.
//! - Repeated identical active calls prove nontermination and return an explicit failure.
//! - Output nodes hold copied scalars and independent arrays, with no borrowed PHP owners.

use std::collections::{BTreeSet, HashSet};
use super::{ArrayGraph, Key, Value};
use crate::{encoding::{Encoding, Substitute}, text::ConversionSources};

/// A PHP recursion-protection state that would repeat indefinitely before returning.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConversionFailure { NonTerminatingRecursion }

/// Converted values, emitted warnings, and the rejected-unit count for request accounting.
#[derive(Debug, PartialEq, Eq)]
pub struct ArrayConversion {
    pub result: Result<ArrayGraph, ConversionFailure>,
    pub warnings: Vec<&'static str>,
    pub illegal_chars: u64,
}

/// Explicit interpreter work, preserving PHP's depth-first conversion and cleanup order.
enum Work {
    Enter { input: usize, output: usize },
    Entry { input: usize, output: usize, position: usize },
    Leave { input: usize, protection: Vec<usize> },
}

/// Converts an array graph without mutating inputs, including shared or circular references.
pub fn convert_encoding(
    input: &ArrayGraph, to: Encoding, sources: &ConversionSources, strict: bool, substitute: Substitute,
) -> ArrayConversion {
    let ((result, warnings), illegal_chars) = crate::encoding::errors::measure(|| convert(input, to, sources, strict, substitute));
    ArrayConversion { result, warnings, illegal_chars }
}

/// Executes PHP's protection-state transitions and copies each retained result into a new graph.
fn convert(
    input: &ArrayGraph, to: Encoding, sources: &ConversionSources, strict: bool, substitute: Substitute,
) -> (Result<ArrayGraph, ConversionFailure>, Vec<&'static str>) {
    let mut output = vec![Vec::new()];
    let mut keys = vec![HashSet::new()];
    let mut protected = BTreeSet::new();
    let mut active_calls = HashSet::new();
    let mut work = vec![Work::Enter { input: input.root(), output: 0 }];
    let mut warnings = Vec::new();
    while let Some(next) = work.pop() {
        match next {
            Work::Enter { input: array, output: target } => {
                if protected.remove(&array) {
                    warnings.push("mb_convert_encoding(): Cannot convert recursively referenced values");
                    continue;
                }
                protected.insert(array);
                let protection: Vec<_> = protected.iter().copied().collect();
                if !active_calls.insert((array, protection.clone())) {
                    return (Err(ConversionFailure::NonTerminatingRecursion), warnings);
                }
                work.push(Work::Leave { input: array, protection });
                work.push(Work::Entry { input: array, output: target, position: 0 });
            }
            Work::Leave { input: array, protection } => {
                protected.remove(&array);
                active_calls.remove(&(array, protection));
            }
            Work::Entry { input: array, output: target, position } => {
                let Some((key, value)) = input.arrays()[array].get(position) else { continue; };
                work.push(Work::Entry { input: array, output: target, position: position + 1 });
                let key = match key {
                    Key::Int(value) => Key::Int(*value),
                    Key::String(bytes) => match convert_string(bytes, to, sources, strict, substitute, &mut warnings) {
                        Some(bytes) => Key::String(bytes), None => continue,
                    },
                };
                let converted = match value {
                    Value::String(bytes) => match convert_string(bytes, to, sources, strict, substitute, &mut warnings) {
                        Some(bytes) => Value::String(bytes), None => continue,
                    },
                    Value::Array(child) => {
                        let next = output.len();
                        output.push(Vec::new());
                        keys.push(HashSet::new());
                        work.push(Work::Enter { input: *child, output: next });
                        Value::Array(next)
                    }
                    Value::Unsupported => {
                        warnings.push("mb_convert_encoding(): Object is not supported");
                        continue;
                    }
                    scalar => scalar.clone(),
                };
                if keys[target].insert(key.clone()) { output[target].push((key, converted)); }
            }
        }
    }
    (Ok(ArrayGraph::new(0, output).expect("converted keys and nodes are valid").into_compact()), warnings)
}

/// Converts one key or value independently and records PHP's warning when detection fails.
fn convert_string(
    input: &[u8], to: Encoding, sources: &ConversionSources, strict: bool, substitute: Substitute,
    warnings: &mut Vec<&'static str>,
) -> Option<Vec<u8>> {
    let output = sources.convert(input, to, strict, substitute);
    if output.is_none() { warnings.push("mb_convert_encoding(): Unable to detect character encoding"); }
    output
}
