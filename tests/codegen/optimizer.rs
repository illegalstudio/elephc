//! Purpose:
//! Groups the optimizer integration test submodules into the parent suite.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Submodules group focused fixtures for constant folding, constant propagation,
//!   dead-code elimination, checked integer sinks, EIR identity/peephole/CSE/LICM
//!   passes, ownership cleanup, and memory-model-aware propagation hazards.

use crate::support::*;

#[path = "optimizer/abs_int_sink.rs"]
mod abs_int_sink;
#[path = "optimizer/sqrt_add_int_sink.rs"]
mod sqrt_add_int_sink;
#[path = "optimizer/strpos_int_sink.rs"]
mod strpos_int_sink;
#[path = "optimizer/strstr_len_sink.rs"]
mod strstr_len_sink;
#[path = "optimizer/base64_decode_len_sink.rs"]
mod base64_decode_len_sink;
#[path = "optimizer/parse_url_count_sink.rs"]
mod parse_url_count_sink;
#[path = "optimizer/count_chars_count_sink.rs"]
mod count_chars_count_sink;
#[path = "optimizer/strip_tags_len_sink.rs"]
mod strip_tags_len_sink;
#[path = "optimizer/inet_pton_len_sink.rs"]
mod inet_pton_len_sink;
#[path = "optimizer/ip2long_int_sink.rs"]
mod ip2long_int_sink;
#[path = "optimizer/string_len_sink.rs"]
mod string_len_sink;
#[path = "optimizer/str_split_count_sink.rs"]
mod str_split_count_sink;
#[path = "optimizer/json_decode_count_sink.rs"]
mod json_decode_count_sink;
#[path = "optimizer/array_sum_int_sink.rs"]
mod array_sum_int_sink;
#[path = "optimizer/array_map_sum_add_int_sink.rs"]
mod array_map_sum_add_int_sink;
#[path = "optimizer/array_filter_count_hoist.rs"]
mod array_filter_count_hoist;
#[path = "optimizer/array_read_match.rs"]
mod array_read_match;
#[path = "optimizer/leaf_runtime_calls.rs"]
mod leaf_runtime_calls;
#[path = "optimizer/switch_fallthrough.rs"]
mod switch_fallthrough;
#[path = "optimizer/bcmath_number.rs"]
mod bcmath_number;

#[path = "optimizer/boxed_array_snapshots.rs"]
mod boxed_array_snapshots;
mod implode_effects;
mod spread_effects;
mod strip_tags_effects;

#[path = "optimizer/branch_simplification.rs"]
mod branch_simplification;
#[path = "optimizer/branch_conditions.rs"]
mod branch_conditions;
#[path = "optimizer/call_result_alias.rs"]
mod call_result_alias;
#[path = "optimizer/numeric_closure_calls.rs"]
mod numeric_closure_calls;
#[path = "optimizer/constant_folding.rs"]
mod constant_folding;
#[path = "optimizer/constant_propagation.rs"]
mod constant_propagation;
#[path = "optimizer/control_flow_normalization.rs"]
mod control_flow_normalization;
#[path = "optimizer/checked_int_sink.rs"]
mod checked_int_sink;
#[path = "optimizer/buffer_element_incdec.rs"]
mod buffer_element_incdec;
#[path = "optimizer/bounded_mixed_counter.rs"]
mod bounded_mixed_counter;
#[path = "optimizer/method_loop_counters.rs"]
mod method_loop_counters;
#[path = "optimizer/checked_numeric_chain.rs"]
mod checked_numeric_chain;
mod integer_destinations;
mod counter_fast_paths;
mod counter_region_comparisons;
#[path = "optimizer/concat_reset.rs"]
mod concat_reset;
#[path = "optimizer/eir_common_subexpression.rs"]
mod eir_common_subexpression;
#[path = "optimizer/dead_code_elimination.rs"]
mod dead_code_elimination;
#[path = "optimizer/dead_instruction_elimination.rs"]
mod dead_instruction_elimination;
#[path = "optimizer/declaration_reachability.rs"]
mod declaration_reachability;
#[path = "optimizer/dead_store_elimination.rs"]
mod dead_store_elimination;
#[path = "optimizer/eir_constant_propagation.rs"]
mod eir_constant_propagation;
#[path = "optimizer/eir_licm.rs"]
mod eir_licm;
#[path = "optimizer/effects_v2.rs"]
mod effects_v2;
#[path = "optimizer/mbstring.rs"]
mod mbstring;
#[path = "optimizer/identity_arithmetic.rs"]
mod identity_arithmetic;
#[path = "optimizer/generator_yield.rs"]
mod generator_yield;
#[path = "optimizer/peephole.rs"]
mod peephole;
#[path = "optimizer/property_receiver_ownership.rs"]
mod property_receiver_ownership;
#[path = "optimizer/receiver_value_alias.rs"]
mod receiver_value_alias;
#[path = "optimizer/read_result_cleanup.rs"]
mod read_result_cleanup;
#[path = "optimizer/fresh_mixed_transfer.rs"]
mod fresh_mixed_transfer;
#[path = "optimizer/release_local_slot.rs"]
mod release_local_slot;
#[path = "optimizer/inline.rs"]
mod inline;
#[path = "optimizer/method_inline.rs"]
mod method_inline;
#[path = "optimizer/integer_range.rs"]
mod integer_range;
#[path = "optimizer/memory_model_propagation.rs"]
mod memory_model_propagation;
#[path = "optimizer/mem2reg.rs"]
mod mem2reg;
mod buffer_access;
mod buffer_property_reuse;
mod buffer_property_loops;
#[path = "optimizer/warning_globals.rs"]
mod warning_globals;

#[path = "optimizer/loop_optimization.rs"]
mod loop_optimization;
mod rematerialization;
mod machine_peepholes;

#[path = "optimizer/tail_call.rs"]
mod tail_call;

#[path = "optimizer/stack_numeric.rs"]
mod stack_numeric;
#[path = "optimizer/numeric_continuation.rs"]
mod numeric_continuation;
#[path = "optimizer/reused_loop_counters.rs"]
mod reused_loop_counters;
#[path = "optimizer/loop_counter_checked_bounds.rs"]
mod loop_counter_checked_bounds;

#[path = "optimizer/memory_metrics.rs"]
mod memory_metrics;
