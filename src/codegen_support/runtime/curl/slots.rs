//! Purpose:
//! The one place that names the `elephc_curl` C ABI entry points and their runtime
//! function-pointer slots, plus the two-instruction "load the slot, bail if null"
//! sequence every `__rt_curl_*` helper opens with.
//!
//! Called from:
//! - The `__rt_curl_*` helper emitters in this module.
//! - `crate::codegen_support::curl::publish_elephc_curl_function_pointers`, which walks
//!   `CURL_ABI_SLOTS` to fill the slots at a curl call site.
//!
//! Key details:
//! - Keeping the (C name, slot name) pairs in ONE table is what guarantees the publisher
//!   and the consumers cannot drift: a helper that reads a slot nothing publishes would
//!   silently fail closed forever, which is exactly the kind of bug a split table
//!   produces.
//! - The slot symbols are `.comm`-declared (8 bytes, zero-initialized) in
//!   `crate::codegen_support::runtime::data::fixed`; a slot's value is a raw function
//!   pointer, never an offset or an index.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// The `elephc_curl` entry points the runtime reaches, paired with the slot that holds
/// each one's address. Extended by later curl tasks; the publisher walks this same list.
pub(crate) const CURL_ABI_SLOTS: &[(&str, &str)] = &[
    ("elephc_curl_easy_init", "_elephc_curl_easy_init_fn"),
    (
        "elephc_curl_easy_setopt_long",
        "_elephc_curl_easy_setopt_long_fn",
    ),
    (
        "elephc_curl_easy_setopt_str",
        "_elephc_curl_easy_setopt_str_fn",
    ),
    (
        "elephc_curl_easy_setopt_slist",
        "_elephc_curl_easy_setopt_slist_fn",
    ),
    ("elephc_curl_easy_perform", "_elephc_curl_easy_perform_fn"),
    ("elephc_curl_easy_errno", "_elephc_curl_easy_errno_fn"),
    ("elephc_curl_easy_error", "_elephc_curl_easy_error_fn"),
    (
        "elephc_curl_easy_getinfo_long",
        "_elephc_curl_easy_getinfo_long_fn",
    ),
    (
        "elephc_curl_easy_take_body",
        "_elephc_curl_easy_take_body_fn",
    ),
    (
        "elephc_curl_easy_getinfo_double",
        "_elephc_curl_easy_getinfo_double_fn",
    ),
    ("elephc_curl_easy_str_op", "_elephc_curl_easy_str_op_fn"),
    (
        "elephc_curl_easy_take_scratch",
        "_elephc_curl_easy_take_scratch_fn",
    ),
    ("elephc_curl_easy_reset", "_elephc_curl_easy_reset_fn"),
    ("elephc_curl_easy_pause", "_elephc_curl_easy_pause_fn"),
    ("elephc_curl_easy_upkeep", "_elephc_curl_easy_upkeep_fn"),
    (
        "elephc_curl_easy_duphandle",
        "_elephc_curl_easy_duphandle_fn",
    ),
    ("elephc_curl_strerror", "_elephc_curl_strerror_fn"),
    ("elephc_curl_easy_free", "_elephc_curl_easy_free_fn"),
    ("elephc_curl_global_info", "_elephc_curl_global_info_fn"),
    ("elephc_curl_option_kind", "_elephc_curl_option_kind_fn"),
    ("elephc_curl_multi_init", "_elephc_curl_multi_init_fn"),
    ("elephc_curl_multi_add", "_elephc_curl_multi_add_fn"),
    ("elephc_curl_multi_remove", "_elephc_curl_multi_remove_fn"),
    ("elephc_curl_multi_perform", "_elephc_curl_multi_perform_fn"),
    ("elephc_curl_multi_select", "_elephc_curl_multi_select_fn"),
    (
        "elephc_curl_multi_info_read",
        "_elephc_curl_multi_info_read_fn",
    ),
    ("elephc_curl_multi_setopt", "_elephc_curl_multi_setopt_fn"),
    ("elephc_curl_multi_errno", "_elephc_curl_multi_errno_fn"),
    (
        "elephc_curl_multi_strerror",
        "_elephc_curl_multi_strerror_fn",
    ),
    ("elephc_curl_multi_free", "_elephc_curl_multi_free_fn"),
    ("elephc_curl_share_init", "_elephc_curl_share_init_fn"),
    ("elephc_curl_share_setopt", "_elephc_curl_share_setopt_fn"),
    ("elephc_curl_share_errno", "_elephc_curl_share_errno_fn"),
    (
        "elephc_curl_share_strerror",
        "_elephc_curl_share_strerror_fn",
    ),
    (
        "elephc_curl_easy_set_share",
        "_elephc_curl_easy_set_share_fn",
    ),
    (
        // NOT `elephc_curl_share_init_persistent`: that spelling is a textual PREFIX of
        // `elephc_curl_share_init` (this table's own earlier row), which trips this
        // module's `the_shared_runtime_never_references_the_curl_bridge` test — see
        // `crates/elephc-curl/src/share.rs`'s identical comment on the C function itself.
        "elephc_curl_share_persistent_init",
        "_elephc_curl_share_persistent_init_fn",
    ),
    ("elephc_curl_share_free", "_elephc_curl_share_free_fn"),
    ("elephc_curl_mime_new", "_elephc_curl_mime_new_fn"),
    ("elephc_curl_mime_add_part", "_elephc_curl_mime_add_part_fn"),
    (
        "elephc_curl_mime_part_field",
        "_elephc_curl_mime_part_field_fn",
    ),
    ("elephc_curl_mime_post", "_elephc_curl_mime_post_fn"),
    ("elephc_curl_mime_abort", "_elephc_curl_mime_abort_fn"),
];

/// The scratch register each target uses to hold a loaded bridge entry pointer.
///
/// `x9` / `r9` are caller-saved scratch on both targets and are already the registers the
/// crypto helpers use for the identical job, so a reader comparing the two families sees
/// the same shape.
pub(crate) fn entry_reg(emitter: &Emitter) -> &'static str {
    match emitter.target.arch {
        Arch::AArch64 => "x9",
        Arch::X86_64 => "r9",
    }
}

/// Loads the bridge entry pointer for `slot` into the scratch register and branches to
/// `missing_label` when the slot is null (the bridge is not linked).
///
/// Emitted BEFORE the helper marshals its C arguments on x86_64 would clobber them, so
/// every caller marshals first and probes second — except where the helper has no
/// arguments to protect.
pub(crate) fn emit_load_entry_or_branch(emitter: &mut Emitter, slot: &str, missing_label: &str) {
    let reg = entry_reg(emitter);
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(emitter, reg, slot);
            emitter.instruction(&format!("ldr {reg}, [{reg}]")); // load the elephc_curl entry pointer
            emitter.instruction(&format!("cbz {reg}, {missing_label}")); // bridge not linked -> fail closed
        }
        Arch::X86_64 => {
            abi::emit_load_symbol_to_reg(emitter, reg, slot, 0); // load the elephc_curl entry pointer
            emitter.instruction(&format!("test {reg}, {reg}")); // bridge not linked -> fail closed
            emitter.instruction(&format!("jz {missing_label}"));
        }
    }
}

/// Emits the indirect call through the loaded entry pointer.
pub(crate) fn emit_call_entry(emitter: &mut Emitter) {
    let reg = entry_reg(emitter);
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction(&format!("blr {reg}")), // call the elephc_curl entry point
        Arch::X86_64 => emitter.instruction(&format!("call {reg}")), // call the elephc_curl entry point
    }
}

/// Zero-extends a C `int32_t` return into the full result register, so a boolean answer
/// is a clean PHP `0`/`1` rather than whatever the callee left in the upper half (which
/// both ABIs leave unspecified).
pub(crate) fn emit_zero_extend_int_result(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction("mov w0, w0"), // zero-extend the C int32_t answer into x0
        Arch::X86_64 => emitter.instruction("mov eax, eax"), // zero-extend the C int32_t answer into rax
    }
}

/// Sign-extends a C `int32_t` return into the full result register. Used only by
/// `curl_errno()`, whose value is a `CURLcode` rather than a boolean.
pub(crate) fn emit_sign_extend_int_result(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction("sxtw x0, w0"), // sign-extend the CURLcode into x0
        Arch::X86_64 => emitter.instruction("movsxd rax, eax"), // sign-extend the CURLcode into rax
    }
}
