//! Purpose:
//! Emits logical callable declarations separately from descriptor storage and reflection metadata.
//!
//! Called from:
//! - The callable descriptor signature record builder.
//!
//! Key details:
//! - Shared Type terms preserve array contents and finite structural alternatives.
//! - The side record owns only static data and retains no PHP values.

use super::{DataSection, DataWord, FunctionSig};
use crate::types::native_type_spec::{logical_parameter_type, logical_type};

/// Serializes exact neutral declaration terms independently from the physical type/size table.
pub(super) fn record(data: &mut DataSection, sig: &FunctionSig, indexes: &[usize]) -> String {
    let mut parameters = Vec::new();
    for index in indexes {
        let source = if sig.declared_params.get(*index).copied().unwrap_or(false) {
            sig.param_type_exprs.get(*index).and_then(Option::as_ref)
                .map(logical_parameter_type)
                .or_else(|| sig.params.get(*index).and_then(|(_, ty)| logical_type(ty, false)))
        } else { None };
        parameters.extend(source_words(data, source.as_ref()));
    }
    let parameters = if parameters.is_empty() { DataWord::U64(0) }
        else { DataWord::Symbol(data.add_words(parameters)) };
    let returned = sig.declared_return.then(|| logical_type(&sig.return_type, true)).flatten();
    let mut words = vec![
        DataWord::U64(elephc_semantics::native_callable_sources::SOURCE_VERSION),
        DataWord::U64(indexes.len() as u64),
    ];
    words.extend(source_words(data, returned.as_ref()));
    words.push(parameters);
    data.add_words(words)
}

/// Stores an optional declaration as JSON of the shared structural Type vocabulary.
fn source_words(data: &mut DataSection, source: Option<&elephc_generics::Type>) -> [DataWord; 2] {
    let Some(source) = source else { return [DataWord::U64(0), DataWord::U64(0)]; };
    let encoded = serde_json::to_vec(source).expect("neutral Type is JSON serializable");
    let (label, length) = data.add_string(&encoded);
    [DataWord::Symbol(label), DataWord::U64(length as u64)]
}
