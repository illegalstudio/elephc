//! Purpose:
//! Freezes the native string, C-string and frame-owner emitters before extraction.
//!
//! Called from:
//! - Compiler assembly regression tests.
//!
//! Key details:
//! - Captured before extraction at 8730c1a83714e0055cff098127834fa506ddcadc.
//! - Fingerprints include internal labels and the contiguous C-string data declarations.

use super::{arrays, callables, data, io, strings};
use crate::codegen_support::{emit::Emitter, platform::Target, value_boxing};
use crate::php_version::PhpVersion;
use crate::types::PhpType;
use sha2::{Digest, Sha256};

/// Captures memory paths through their existing compiler entry points.
#[test]
fn memoir_storage_followup_preserves_native_assembly() {
    let mut fingerprints = Vec::new();
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        let fixed = data::emit_runtime_data_fixed(262_144, target, PhpVersion::default());
        let start = fixed.lines().position(|line| line.starts_with(".comm ") && line.contains("_cstr_buf,")).unwrap();
        let cstr_data = fixed.lines().skip(start).take_while(|line| !line.contains("_eof_flags")).collect::<Vec<_>>();
        assert!(cstr_data.iter().any(|line| line.contains("_cstr2_cap:")));

        for mode in 0..6 {
            let mut emitter = Emitter::new(target);
            emitter.pic_data_refs = mode == 1 || mode == 2;
            emitter.cdylib_boundary = mode == 2 || mode == 3;
            emitter.phpext_output = mode == 4;
            emitter.dead_strip = mode == 5;
            strings::emit_strcopy(&mut emitter);
            strings::emit_str_dup_owned(&mut emitter);
            io::emit_cstr(&mut emitter);
            arrays::emit_array_reduce_boxed(&mut emitter);
            arrays::emit_array_map_boxed(&mut emitter);
            arrays::emit_array_predicate_boxed(&mut emitter);
            arrays::emit_array_walk_boxed(&mut emitter);
            arrays::emit_array_udiff_uintersect(&mut emitter);
            arrays::emit_array_numeric_aggregate(&mut emitter);
            callables::emit_callable_invoke_owned_args(&mut emitter);
            emitter.label_global("__memoir_owned_string_probe");
            value_boxing::emit_box_current_owned_value_as_mixed(&mut emitter, &PhpType::Str);
            let mut labels = emitter.take_internal_labels().into_iter().collect::<Vec<_>>();
            labels.sort();
            let assembly = emitter.output();
            let hash = Sha256::digest(format!("{assembly}\n{labels:?}\n{cstr_data:?}"));
            fingerprints.push(format!("{name} {mode} {hash:x}"));
        }
    }
    assert_eq!(fingerprints.join("\n"), include_str!("memoir_storage_followup_baseline.txt").trim());
}
