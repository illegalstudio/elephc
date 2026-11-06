//! Purpose:
//! Verifies target-wide ownership boundaries in emitted recursive JSON decoders.
//!
//! Called from:
//! - The library test harness through `super::emit_json_decode_mixed()`.
//!
//! Key details:
//! - Every target must defer throws until recursive cleanup and transfer fresh containers.
//! - Executable tests independently verify PHP output and actual heap balance.

use super::*;
use crate::codegen_support::platform::Target;

/// All five ABIs expose one outer throw boundary and release fresh parser owners.
#[test]
fn json_decode_owners_all_targets_have_one_throw_boundary() {
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        for pic in [false, true] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emitter.pic_data_refs = pic;
            emit_json_decode_mixed(&mut emitter);
            let assembly = emitter.output();
            let x86 = name == "linux-x86_64";
            let call = if x86 { "    call " } else { "    bl " };
            assert_eq!(assembly.matches("__rt_json_decode_mixed:\n").count(), 1, "{name}, pic={pic}");
            assert_eq!(assembly.matches(&format!("{call}__rt_json_decode_mixed\n")).count(), 0,
                "{name}, pic={pic}: recursive frames must not publish exceptions");
            assert_eq!(assembly.matches(&format!("{call}__rt_json_box_owned_container\n")).count(), 6,
                "{name}, pic={pic}: empty and populated array/hash/object boxing must transfer");
            let boundary = assembly.split("__rt_json_decode_mixed:\n").nth(1).unwrap()
                .split("__rt_json_box_owned_container:\n").next().unwrap();
            assert!(boundary.find(&format!("{call}__rt_json_decode_mixed_value")).unwrap()
                < boundary.find(&format!("{call}__rt_json_throw_error")).unwrap(),
                "{name}, pic={pic}: throwing must follow recursive cleanup");
            for stem in ["__rt_json_decode_array_real_propagate", "__rt_json_decode_object_real_propagate"] {
                let label = format!("{stem}{}:\n", if x86 { "_x" } else { "" });
                let cleanup = assembly.split(&label).nth(1).unwrap();
                let release = if stem.contains("array") { "__rt_decref_array" } else { "__rt_decref_hash" };
                assert!(cleanup.find(&format!("{call}{release}")).unwrap() < cleanup.find("ret").unwrap(),
                    "{name}, pic={pic}: partial container must retire before failure return");
            }
        }
    }
}

/// Only validated string tokens enter the legacy unescaper on every direct and PIC ABI.
#[test]
fn json_decode_nonstring_tokens_skip_legacy_unescape_all_targets() {
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        for pic in [false, true] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emitter.pic_data_refs = pic;
            emit_json_decode_mixed(&mut emitter);
            let assembly = emitter.output();
            let call = if name == "linux-x86_64" { "    call " } else { "    bl " };
            let value = assembly.split("__rt_json_decode_mixed_value:\n").nth(1).unwrap();
            let (dispatch, string) = value.split_once("__rt_json_decode_mixed_string:\n").unwrap();
            let string = string.split("__rt_json_decode_mixed_array:\n").next().unwrap();
            assert!(!dispatch.contains(&format!("{call}__rt_json_decode\n")), "{name}, pic={pic}");
            let validate = string.find(&format!("{call}__rt_json_validate_string")).unwrap();
            let unescape = string.find(&format!("{call}__rt_json_decode\n")).unwrap();
            assert!(validate < unescape, "{name}, pic={pic}: malformed strings must fail before unescaping");
            assert_eq!(value.matches(&format!("{call}__rt_json_decode\n")).count(), 1,
                "{name}, pic={pic}: scalar and container branches use their already trimmed slices");
        }
    }
}
