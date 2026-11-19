//! Purpose:
//! Defines owned reference-cell storage and emits its allocation, cloning and release helpers.
//!
//! Called from:
//! - Object property lowering, managed runtime emission and cycle collection.
//!
//! Key details:
//! - The two payload words stay compatible with borrowed reference addresses.
//! - `REFERENCE_CELL_HEAP_KIND` identifies an owned cell; bits 8 through 14 describe its payload.
//! - Singleton cells separate on clone, while cells with live aliases remain shared.
//! - Exact active boxed-walk borrows are recognized separately from ordinary owner-zero cells.

use elephc_memoir::values::throwable_storage;
use crate::codegen_support::{abi, emit::Emitter, platform::Arch};
use crate::codegen_support::sentinels::UNINITIALIZED_TYPED_PROPERTY_SENTINEL;
use crate::types::PhpType;
use elephc_memoir::reference_cells::{emit_new, emit_value_release, emit_release, emit_clone, emit_owner_lookup, emit_unmanaged_borrow_lookup};

mod property_unset;
mod mixed_payload;
mod mixed_symbol;
mod property_payload;
mod canonical;
mod parameter_bindings;
mod argument_pins;
mod validation_pins;
mod validation_coercion;
mod validation_ops;
mod validation_coordinator;
pub(super) use validation_coordinator::emit as emit_validation_coordinator;
mod validation_commit;
pub(super) use validation_commit::emit as emit_validation_commit;
mod validation_publish;
pub(super) use validation_publish::emit as emit_validation_publish;
mod source_lifecycle;
mod static_cells;
pub(crate) use static_cells::{BINDING_BYTES as STATIC_BINDING_BYTES, load_mixed_value as load_mixed_static_value};
pub(super) use source_lifecycle::emit as emit_source_lifecycle;
pub(super) use source_lifecycle::require_success as require_source_success;
pub(crate) use validation_pins::subtract as subtract_validation_pins;
pub(crate) use canonical::resolve_scalar_property;
pub(crate) use property_unset::emit_owned_property_unset;
pub(crate) use mixed_payload::follow_mixed_reference_slot;
pub(crate) use mixed_symbol::publish_mixed_symbol;
pub(crate) use property_payload::resolve as resolve_property_payload;

/// Byte length of `_unmanaged_reference_escape_msg`, kept beside the data that defines it.
const UNMANAGED_REFERENCE_ESCAPE_MSG_LEN: i64 = 73;

/// Byte length of `_borrowed_reference_return_msg`, kept beside the data that defines it.
const BORROWED_REFERENCE_RETURN_MSG_LEN: i64 = 75;
/// Diagnostic owned by a conversion conflict before the original RHS can be destroyed.
pub(crate) const NATIVE_REFERENCE_CONFLICT_MSG: &str =
    "Cannot assign to a reference held by properties with incompatible types";

/// Diagnostic for a descriptor reference whose caller and selected parameter layouts differ.
pub(crate) const TAGGED_REFERENCE_ARGUMENT_MSG: &str =
    "Unsupported reference argument: caller storage does not match the selected parameter layout";
/// Diagnostic for a replacement that cannot inhabit an existing inline nullable-integer cell.
pub(crate) const TAGGED_REFERENCE_ASSIGNMENT_MSG: &str =
    "Unsupported assignment through a nullable integer reference: value must be int or null";

/// Returns the descriptor tag for the actual stored property representation.
pub(crate) fn payload_tag(php_type: &PhpType) -> i64 {
    match php_type.codegen_repr() {
        PhpType::Str => elephc_memoir::layout::STRING_REFERENCE_PAYLOAD_TAG,
        PhpType::Array(_) => 4,
        PhpType::AssocArray { .. } => 5,
        PhpType::Object(_) => 6,
        PhpType::Mixed | PhpType::Union(_) | PhpType::Iterable => 7,
        PhpType::Callable => 10,
        _ => 0,
    }
}

/// Emits managed reference cells with the same ownership rules on every supported target.
pub(super) fn emit_reference_cells(emitter: &mut Emitter, native_validation: bool) {
    emit_new(emitter);
    super::property_ref_cells::emit(emitter);
    emit_value_release(emitter);
    emit_release(emitter);
    emit_clone(emitter);
    mixed_payload::emit_promote_mixed(emitter, native_validation);
    mixed_payload::emit_retain_mixed(emitter);
    validation_pins::emit(emitter);
    argument_pins::emit(emitter);
    validation_coercion::emit(emitter);
    validation_ops::emit(emitter);
    property_unset::emit_property_unset(emitter, native_validation);
    emit_owner_lookup(emitter);
    parameter_bindings::emit(emitter);
    canonical::emit(emitter);
    emit_unmanaged_borrow_lookup(emitter);
    emit_unmanaged_borrow_escape_error(emitter);
    emit_borrowed_reference_return_error(emitter);
    emit_reference_escape_error(emitter, "__rt_tagged_reference_argument_error",
        "_tagged_reference_argument_msg", TAGGED_REFERENCE_ARGUMENT_MSG.len() as i64);
    emit_reference_escape_error(emitter, "__rt_tagged_reference_assignment_error",
        "_tagged_reference_assignment_msg", TAGGED_REFERENCE_ASSIGNMENT_MSG.len() as i64);
    if native_validation { static_cells::emit(emitter); }
}


















/// Allocates the catchable `Error` used when an unmanaged reference crosses a lifetime boundary.
fn emit_unmanaged_borrow_escape_error(emitter: &mut Emitter) {
    emit_reference_escape_error(
        emitter,
        "__rt_unmanaged_reference_escape_error",
        "_unmanaged_reference_escape_msg",
        UNMANAGED_REFERENCE_ESCAPE_MSG_LEN,
    );
}

/// Allocates the catchable `Error` a by-reference return raises when its place has no cell owner.
///
/// The message is separate from the `array_walk()` borrow escape on purpose: this boundary is
/// reached by any reference whose provenance the compiler could not settle statically, most
/// often an alias relayed through a by-reference parameter, and reporting it as an
/// `array_walk()` problem would misdescribe the program that raised it.
fn emit_borrowed_reference_return_error(emitter: &mut Emitter) {
    emit_reference_escape_error(
        emitter,
        "__rt_borrowed_reference_return_error",
        "_borrowed_reference_return_msg",
        BORROWED_REFERENCE_RETURN_MSG_LEN,
    );
}

/// Emits one `<entry>_new` allocator plus the `<entry>` thrower that publishes and raises it.
pub(crate) fn emit_reference_escape_error(emitter: &mut Emitter, entry: &str, message: &str, length: i64) {
    emit_reference_error_allocator(emitter, &format!("{entry}_new"), "_spl_error_class_id", message, length);
    emitter.blank();
    emitter.label_global(entry);
    abi::emit_frame_prologue(emitter, 16);
    abi::emit_call_label(emitter, &format!("{entry}_new"));
    abi::emit_store_reg_to_symbol(emitter, abi::int_result_reg(emitter), "_exc_value", 0);
    abi::emit_frame_restore(emitter, 16);
    abi::emit_jump(emitter, "__rt_throw_current");
}

/// Emits a compact built-in Throwable owner without publishing or raising it.
fn emit_reference_error_allocator(
    emitter: &mut Emitter, entry: &str, class_id: &str, message: &str, length: i64,
) {
    emitter.blank();
    emitter.label_global(entry);
    abi::emit_frame_prologue(emitter, 32);
    abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 56);
    match emitter.target.arch {
        Arch::AArch64 => {
            elephc_memoir::values::object_storage::emit_allocate_throwable(emitter);
            throwable_storage::emit_class_id(emitter, class_id);
            throwable_storage::emit_static_message(emitter, message, length, throwable_storage::LengthStore::Register);
            throwable_storage::emit_default_fields(emitter, "x0");
        }
        Arch::X86_64 => {
            elephc_memoir::values::object_storage::emit_allocate_throwable_x86_64(emitter);
            throwable_storage::emit_class_id(emitter, class_id);
            throwable_storage::emit_static_message(emitter, message, length, throwable_storage::LengthStore::Immediate);
            throwable_storage::emit_default_fields(emitter, "rax");
        }
    }
    abi::emit_frame_restore(emitter, 32);
    abi::emit_return(emitter);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// The framed clone returns before the separate frameless null path on every supported ABI.
    #[test]
    fn reference_cell_clone_has_distinct_framed_and_frameless_returns() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit_clone(&mut emitter);
            let assembly = emitter.output();
            let (framed, early) = assembly.split_once("__rt_reference_cell_clone_done:").unwrap();
            assert!(framed.trim_end().ends_with("ret"), "{name}: framed path must return before the early-out label");
            assert!(early.trim_start().starts_with("ret"), "{name}: null path must not restore a frame");
        }
    }

    /// Detached canonical cells keep a PHPRef payload, as scalar readers require for descriptor seven.
    #[test]
    fn singleton_canonical_cell_clone_preserves_wrapper_shape_on_every_target() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit_clone(&mut emitter);
            let assembly = emitter.output();
            let select = assembly.split_once("__rt_reference_cell_clone_dispatch:").unwrap().0;
            assert!(select.contains(if name == "linux-x86_64" {
                "jmp __rt_reference_cell_clone_reference"
            } else { "b __rt_reference_cell_clone_reference" }), "{name}: singleton references preserve their physical shape");
            let reference = assembly.split_once("__rt_reference_cell_clone_reference:").unwrap().1
                .split_once("__rt_reference_cell_clone_store:").unwrap().0;
            assert!(reference.contains("__rt_reference_new"), "{name}: allocate an independent wrapper");
            assert!(!reference.contains("__rt_mixed_clone"), "{name}: flattened values cannot inhabit canonical scalar cells");
            assert!(assembly.contains("__rt_reference_cell_clone_retain"), "{name}: live aliases remain shared");
        }
    }

    /// Mach-O conditional branches require local labels even when a helper is emitted in the same object.
    #[test]
    fn reference_cell_dispatch_uses_local_stubs_before_global_tail_calls() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit_reference_cells(&mut emitter, false);
            super::super::arrays::emit_decref_any(&mut emitter);
            let assembly = emitter.output();
            for entry in ["__rt_decref_any", "__rt_callable_descriptor_release",
                "__rt_incref", "__rt_reference_cell_release"] {
                for line in assembly.lines() {
                    let words = line.split_whitespace().collect::<Vec<_>>();
                    if words.len() == 2 && words[1] == entry {
                        assert!(!words[0].starts_with("b.") && !matches!(words[0], "je" | "ja"),
                            "{name}: global conditional branch {line}");
                    }
                }
            }
            for stub in ["__rt_reference_cell_value_heap:", "__rt_reference_cell_value_callable:",
                "__rt_reference_cell_clone_shared:", "__rt_decref_any_reference:"] {
                assert!(assembly.contains(stub), "{name}: missing local dispatch stub {stub}");
            }
        }
    }

    /// Capture enumeration validates each owner with bounded work on every target.
    #[test]
    fn reference_cell_owner_validation_has_constant_work_on_every_target() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit_owner_lookup(&mut emitter);
            let assembly = emitter.output();
            assert_eq!(assembly.matches("__rt_gc_candidate_live").count(), 1, "{name}: {assembly}");
            assert!(!assembly.contains("_heap_buf") && !assembly.contains("_owner_scan"), "{name}: owner lookup traverses allocation headers");
            assert!(assembly.lines().count() < 32, "{name}: owner validation exceeds its constant instruction budget");
        }
    }

    /// All supported emitters expose typed cell allocation, release and graph traversal together.
    #[test]
    fn owned_reference_cells_are_complete_on_every_target() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit_reference_cells(&mut emitter, false);
            super::super::exceptions::emit_local_ref_cell_release(&mut emitter);
            super::super::arrays::emit_decref_any(&mut emitter);
            super::super::arrays::emit_gc_collect_cycles(&mut emitter);
            super::super::arrays::emit_gc_mark_reachable(&mut emitter);
            super::super::arrays::emit_object_free_deep(&mut emitter, crate::codegen_support::RuntimeFeatures::all());
            let assembly = emitter.output();
            for entry in ["__rt_reference_cell_new", "__rt_reference_cell_clone",
                "__rt_reference_cell_value_release", "__rt_reference_cell_free_deep",
                "__rt_reference_cell_is_unmanaged_borrow",
                "__rt_unmanaged_reference_escape_error_new",
                "__rt_unmanaged_reference_escape_error",
                "__rt_tagged_reference_argument_error_new", "__rt_tagged_reference_argument_error",
                "__rt_tagged_reference_assignment_error_new", "__rt_tagged_reference_assignment_error",
                "__rt_local_ref_cell_release_managed", "__rt_gc_collect_cycles_count_reference",
                "__rt_gc_collect_cycles_free_reference", "__rt_gc_mark_reachable_reference"] {
                assert!(assembly.contains(&format!("{entry}:")), "{name}: missing {entry}");
            }
            let (alias, retire, singleton) = if name == "linux-x86_64" {
                ("je __rt_decref_any_reference", "cmp r8, 11", "ja __rt_reference_cell_clone_shared")
            } else {
                ("b.eq __rt_decref_any_reference", "cmp x15, #11", "b.hi __rt_reference_cell_clone_shared")
            };
            assert!(assembly.contains(alias) && assembly.contains(retire), "{name}: object owners use cell release");
            assert!(assembly.contains(singleton), "{name}: live aliases survive object cloning");
        }
    }

    /// Payload tags describe backend storage, not the PHP array or nullable spelling alone.
    #[test]
    fn owned_reference_cell_tags_follow_storage_representation() {
        assert_eq!(payload_tag(&PhpType::Int), 0);
        assert_eq!(payload_tag(&PhpType::Str), 1);
        assert_eq!(payload_tag(&PhpType::Array(Box::new(PhpType::Int))), 4);
        assert_eq!(payload_tag(&PhpType::Mixed), 7);
        assert_eq!(payload_tag(&PhpType::Callable), 10);
    }

    /// Active borrowed cells use exact-address lookup and a catchable Error on every ABI.
    #[test]
    fn unmanaged_reference_escape_guard_is_target_complete() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit_reference_cells(&mut emitter, false);
            let assembly = emitter.output();
            assert!(assembly.contains("_rt_unmanaged_ref_borrow_top"), "{name}");
            assert!(assembly.contains("_unmanaged_reference_escape_msg"), "{name}");
            assert!(assembly.contains("_spl_error_class_id"), "{name}");
            assert!(assembly.contains("__rt_object_handle_acquire"), "{name}");
            assert!(assembly.contains("__rt_throw_current"), "{name}");
            let compare = if name == "linux-x86_64" {
                "cmp rax, r11"
            } else {
                "cmp x0, x10"
            };
            assert!(assembly.contains(compare), "{name}: borrowed lookup must compare exact cells");
        }
    }
}
