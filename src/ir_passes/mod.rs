//! Purpose:
//! IR-level analyses and transformations over EIR functions. Phase 06 starts
//! here with liveness analysis, the foundation for the linear-scan register
//! allocator. Scalar promotion, peephole, CSE, and LICM live alongside it.
//!
//! Called from:
//! - `crate::pipeline::compile()` after AST-to-EIR lowering, before codegen.
//!
//! Key details:
//! - Passes are either read-only analyses that produce sidecar tables (e.g.
//!   liveness, allocation) or in-place transformations driven by the fixed-point
//!   `driver`, which re-validates each function after every pass in debug/test
//!   builds.

mod allocation;
mod array_fill_loop;
mod array_map_sum_add_int_sink;
mod array_get_bounds;
mod abs_int_sink;
mod array_filter_count_hoist;
mod array_map_sum_cache;
mod array_sum_int_bounds;
mod array_sum_int_direct;
mod array_sum_int_sink;
mod string_len_sink;
mod json_decode_count_sink;
mod json_encode_len_sink;
mod sqrt_add_int_sink;
mod parse_url_count_sink;
mod inet_pton_len_sink;
mod strip_tags_len_sink;
mod strpos_int_sink;
mod count_chars_count_sink;
mod str_split_count_sink;
mod base64_decode_len_sink;
mod mb_catalog_count_sink;
mod mb_detect_order_single_sink;
mod ereg_search_array_result_sink;
mod ereg_search_literal_state_sink;
mod gz_len_hoist;
mod gz_literal_roundtrip_len_sink;
mod hash_algos_count_hoist;
mod hash_const_sink;
mod iconv_get_encoding_len_sink;
mod iconv_mime_literal_sink;
mod iconv_position_const_sink;
mod mb_convert_variables_len_sink;
mod mb_parse_str_literal_sink;
mod mb_strlen_const_local_sink;
mod openssl_cbc_len_sink;
mod openssl_cipher_iv_length_const_sink;
mod mb_strstr_len_const_sink;
mod mb_numericentity_len_sink;
mod mb_strimwidth_len_sink;
mod strstr_len_sink;
mod token_get_all_count_hoist;
mod array_sum_loop;
mod boxed_narrowing;
mod bounded_mixed_counter;
mod branch_simplify;
mod borrowed_receiver_roots;
mod receiver_local_roots;
mod buffer_access;
mod buffer_property_reuse;
mod buffer_property_loops;
mod buffer_property_receivers;
mod guarded_buffer_prefix;
mod guard_facts;
mod by_ref_alias;
mod checked_int_sink;
mod checked_numeric_chain;
mod cfg;
mod clobber;
mod concat_reset;
mod const_fold;
mod count_hoist;
mod cse;
mod dead_inst;
mod dead_store;
mod dominance;
mod driver;
mod fresh_mixed_transfer;
mod identity_arith;
mod inline;
mod numeric_closure_calls;
mod induction;
mod loop_optimize;
mod loop_cfg;
mod integer_range;
mod immutable_local_loads;
mod intervals;
mod licm;
mod local_scope;
mod liveness;
mod loops;
mod mem2reg;
pub(crate) mod numeric_domain;
mod peephole;
mod pure_mbstring_calls;
mod regalloc;
pub(crate) mod rematerialize;
mod rewrite;
mod tail_call;

#[cfg(test)]
mod tests;

pub use allocation::{Allocation, Location};
pub use dominance::{compute_dominance, DominanceInfo};
pub use driver::optimize_module;
pub use intervals::{build_intervals, LiveInterval};
pub use liveness::{compute_liveness, LivenessInfo};
pub(crate) use cfg::successors;
pub(crate) use bounded_mixed_counter::prove_integer_slots;
pub(crate) use by_ref_alias::address_escaping_slots;
pub(crate) use liveness::terminator_uses;
pub use loops::{compute_loops, LoopInfo, NaturalLoop};
pub use regalloc::allocate_registers;
pub use rematerialize::ScalarConstant;
