//! Purpose:
//! Deletes the `tmpfile()` handles still open when the script ends, for the wasm32-wasi backend.
//!
//! Called from:
//! - `crate::codegen_wasm::plan::generate()` to register the teardown routine.
//! - `crate::codegen_wasm::context` (`emit_main_shutdown`) and the non-`main` `exit()` path
//!   in `crate::codegen_wasm::inst`, which call it at the end of the script-end teardown.
//!
//! Key details:
//! - php deletes a temporary file when its handle is freed, which includes the end of the
//!   script. The file this backend creates lives in the preopened directory under a generated
//!   name (WASI preview1 cannot unlink an open file), and only `fclose` removed it, so a handle
//!   nobody closed left `.elephc_tmpNNN` behind.
//! - The routine reads the per-fd temp-file table `files.rs` already maintains (one byte per
//!   fd, 0 = not a temp file, n = name index n-1; cleared by `fclose`), so no new bookkeeping
//!   exists: every entry still set belongs to a handle that was never closed.
//! - Emitted only for a command module that can call `tmpfile()`; every other module keeps
//!   exactly the code it had.

use super::files::{FD_EXT_PTR, FD_LIMIT, FD_TABLE_INLINE, TMP_NAME, TMP_NAME_LEN};
use super::wat::WatModule;
use crate::ir::{Immediate, Module, RuntimeCallTarget, RuntimeFnId};

/// Symbol of the routine that deletes the temp files still open at script end.
pub(super) const RT_TMPFILES_TEARDOWN_NAME: &str = "__rt_tmpfiles_teardown";

/// Whether any function of the module calls `tmpfile()` directly.
pub(super) fn module_uses_tmpfile(module: &Module) -> bool {
    super::function::all_functions(module).any(|function| {
        function.instructions.iter().any(|inst| {
            let Some(Immediate::RuntimeCall(target)) = inst.immediate.as_ref() else {
                return false;
            };
            let target = match target {
                RuntimeCallTarget::Function(id) => *id,
                RuntimeCallTarget::ProfiledFunction { target, .. } => *target,
                _ => return false,
            };
            target == RuntimeFnId::Tmpfile
        })
    })
}

/// Whether the teardown exists in this module: a command module that calls `tmpfile()`.
pub(super) fn module_has_tmpfile_teardown(module: &Module) -> bool {
    module.functions.iter().any(|function| function.flags.is_main) && module_uses_tmpfile(module)
}

/// Emits `__rt_tmpfiles_teardown`: closes and unlinks every temp file whose handle is still open.
///
/// The extension block of the fd table exists only once an fd past the inline range was handed
/// out, so it is scanned only when its pointer is set (`__rt_fd_addr` would allocate it).
pub(super) fn emit_tmpfiles_teardown(wm: &mut WatModule, module: &Module) {
    if !module_has_tmpfile_teardown(module) {
        return;
    }
    wm.add_raw_func(&format!(
        r#"(func ${name}
  (local $fd i32) (local $end i32) (local $slot i32) (local $index i32)
  (local.set $end (i32.const {inline}))
  (if (i32.load (i32.add (global.get $__float_scratch) (i32.const {ext})))
    (then (local.set $end (i32.const {limit}))))
  (local.set $fd (i32.const 3))                                    ;; the standard streams are never temp files
  (block $done (loop $scan
    (br_if $done (i32.ge_u (local.get $fd) (local.get $end)))
    (local.set $slot (call $__rt_fd_addr (local.get $fd) (i32.const 1)))
    (if (local.get $slot)
      (then
        (local.set $index (i32.load8_u (local.get $slot)))
        (if (local.get $index)
          (then
            (i32.store8 (local.get $slot) (i32.const 0))
            (drop (call $wasi_fd_close (local.get $fd)))
            (call $__rt_tmp_name (i32.sub (local.get $index) (i32.const 1)))
            (drop (call $__rt_unlink
              (i32.add (global.get $__float_scratch) (i32.const {tmp_name}))
              (i64.const {tmp_len})))))))
    (local.set $fd (i32.add (local.get $fd) (i32.const 1)))
    (br $scan))))
"#,
        name = RT_TMPFILES_TEARDOWN_NAME,
        inline = FD_TABLE_INLINE,
        limit = FD_LIMIT,
        ext = FD_EXT_PTR,
        tmp_name = TMP_NAME,
        tmp_len = TMP_NAME_LEN,
    ));
}
