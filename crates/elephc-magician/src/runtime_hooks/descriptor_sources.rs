//! Purpose:
//! Decodes optional versioned logical declaration records from trusted native descriptors.
//!
//! Called from:
//! - Runtime callable signature metadata acquisition and focused compatibility tests.
//!
//! Key details:
//! - Legacy flags prevent any read beyond the old fixed header.
//! - Physical storage tags never become logical source declarations.

use crate::errors::EvalStatus;
use elephc_generics::Type;
use elephc_semantics::native_callable_sources as layout;

/// Logical declarations in caller-visible parameter order, independent from physical ABI tags.
pub(crate) struct DescriptorSources {
    pub(crate) parameters: Vec<Option<Type>>,
    pub(crate) returned: Option<Type>,
}

/// Decodes an aligned trusted signature and its optional bounded declaration payloads.
///
/// # Safety
/// The fixed signature and every non-null flagged static record must be readable at their declared sizes.
pub(crate) unsafe fn decode(signature: *const u64, count: usize) -> Result<DescriptorSources, EvalStatus> {
    if *signature.add(layout::FLAGS_WORD) & layout::SOURCE_FLAG == 0 {
        return Ok(DescriptorSources { parameters: vec![None; count], returned: None });
    }
    let record = *signature.add(layout::SOURCE_RECORD_WORD) as *const u64;
    if record.is_null() || (record as usize) % std::mem::align_of::<u64>() != 0
        || *record != layout::SOURCE_VERSION || *record.add(1) as usize != count {
        return Err(EvalStatus::AbiMismatch);
    }
    let returned = decode_type(*record.add(2), *record.add(3))?;
    let parameters = *record.add(4) as *const u64;
    if count != 0 && (parameters.is_null() || (parameters as usize) % std::mem::align_of::<u64>() != 0) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let mut result = Vec::with_capacity(count);
    for index in 0..count {
        result.push(decode_type(*parameters.add(index * 2), *parameters.add(index * 2 + 1))?);
    }
    Ok(DescriptorSources { parameters: result, returned })
}

/// Parses one optional static Type payload, preserving nullable, union and nested container structure.
///
/// # Safety
/// A nonzero pointer must be readable for the bounded byte length.
unsafe fn decode_type(pointer: u64, length: u64) -> Result<Option<Type>, EvalStatus> {
    if pointer == 0 && length == 0 { return Ok(None); }
    let length = usize::try_from(length).map_err(|_| EvalStatus::RuntimeFatal)?;
    if pointer == 0 || length == 0 || length > layout::MAX_SOURCE_BYTES {
        return Err(EvalStatus::RuntimeFatal);
    }
    let bytes = std::slice::from_raw_parts(pointer as *const u8, length);
    serde_json::from_slice(bytes).map(Some).map_err(|_| EvalStatus::RuntimeFatal)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A legacy twelve-word signature is read without touching an absent extension slot.
    #[test]
    fn strict_descriptor_sources_legacy_signatures_remain_unknown() {
        let signature=[0_u64;12];let sources=unsafe{decode(signature.as_ptr(),2)}.unwrap();
        assert_eq!(sources.parameters,vec![None,None]);assert_eq!(sources.returned,None);
    }

    /// Structural union/array alternatives round-trip while unknown entries stay absent.
    #[test]
    fn strict_descriptor_sources_decode_structural_terms_and_versions() {
        let ty=Type::Apply("array".into(),vec![Type::atom("int"),Type::Nullable(Box::new(
            Type::Union(vec![Type::atom("string"),Type::atom("int")])))]);
        let encoded=br#"{"Apply":["array",[{"Atom":"int"},{"Nullable":{"Union":[{"Atom":"string"},{"Atom":"int"}]}}]]}"#;
        let parameters=[encoded.as_ptr() as u64,encoded.len() as u64,0,0];
        let record=[layout::SOURCE_VERSION,2,encoded.as_ptr() as u64,encoded.len() as u64,parameters.as_ptr() as u64];
        let mut signature=[0_u64;13];signature[layout::FLAGS_WORD]=layout::SOURCE_FLAG;
        signature[layout::SOURCE_RECORD_WORD]=record.as_ptr() as u64;
        let sources=unsafe{decode(signature.as_ptr(),2)}.unwrap();
        assert_eq!(sources.parameters,vec![Some(ty.clone()),None]);assert_eq!(sources.returned,Some(ty));
        let incompatible=[layout::SOURCE_VERSION+1,record[1],record[2],record[3],record[4]];
        signature[layout::SOURCE_RECORD_WORD]=incompatible.as_ptr() as u64;
        assert!(matches!(unsafe{decode(signature.as_ptr(),2)},Err(EvalStatus::AbiMismatch)));
    }
}
