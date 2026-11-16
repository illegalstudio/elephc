//! Purpose:
//! Generates heap, reference-counting and cycle-collection assembly for elephc.
//!
//! Called from:
//! - The compiler runtime orchestrator through its host emitter adapter.
//!
//! Key details:
//! - This code runs in the compiler and emits assembly; it is never a target-program library.

#![forbid(unsafe_code)]

mod architecture;
mod emitter;
mod abi;
pub mod layout;
pub mod cleanup;
pub use architecture::Arch;
pub use emitter::{Emitter, GraphLayout, GcStatusSelectors};

pub mod decref_any;
pub mod decref_array;
pub mod decref_hash;
pub mod decref_mixed;
pub mod decref_object;
pub mod eval_array_references;
pub mod gc_callable_children;
pub mod gc_candidates;
pub mod gc_collect_budget;
pub mod gc_collect_cycles;
pub mod gc_collect_cycles_x86_64;
pub mod gc_control;
pub mod gc_destructors;
pub mod gc_eval_object_children;
pub mod gc_exceptions;
pub mod gc_frame_children;
pub mod gc_frame_slots;
pub mod gc_host_roots;
pub mod gc_mark_reachable;
pub mod gc_note_child_ref;
pub mod gc_object_children;
pub mod gc_weak_observers;
pub mod hash_layout;
pub mod heap_alloc;
pub mod heap_consolidate;
pub mod heap_debug_check_live;
pub mod heap_debug_fail;
pub mod heap_debug_report;
pub mod heap_debug_validate_free_list;
pub mod heap_free;
pub mod heap_grow_tail;
pub mod heap_kind;
pub mod heap_live_bases;
pub mod incref;
pub mod memory;

pub mod diagnostics;
pub mod values;
pub mod strings;
pub mod buffers;
pub mod reference_cells;
mod value_features;
pub use value_features::ValueFeatures;
pub mod data;
