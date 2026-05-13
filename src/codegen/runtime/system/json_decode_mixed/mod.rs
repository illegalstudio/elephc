use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

mod aarch64;
mod arrays;
mod objects;
mod x86_64;

/// __rt_json_decode_mixed: structural JSON decoder that returns a boxed
/// Mixed cell pointer.
///
/// Wraps the existing string-returning `__rt_json_decode` helper and
/// classifies the result based on the first non-whitespace byte of the
/// original input:
///
///   * `"`            → Mixed(tag=1, ptr, len)
///   * `t`            → Mixed(tag=3, lo=1)         (true)
///   * `f`            → Mixed(tag=3, lo=0)         (false)
///   * `n`            → Mixed(tag=8)               (null)
///   * `-` / digit    → parse as int or float (presence of `.`/`e`/`E`
///                     toggles the float path); ints use `__rt_atoi`
///                     and floats use libc `atof` on a null-terminated
///                     stack copy of the decoded slice.
///   * `[` / `{`      → recursive structural decode via the helper
///                     emitters under `arrays`/`objects`; falls back to
///                     a passthrough Mixed(string) on parser failure.
///   * empty input    → Mixed(tag=8) (null)
///
/// Input ABI:
///   ARM64: x1 = json ptr, x2 = json len
///   x86_64: rax = json ptr, rdx = json len
/// Output: x0/rax = boxed Mixed pointer
pub(crate) fn emit_json_decode_mixed(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        x86_64::emit(emitter);
        arrays::emit_x86_64(emitter);
        objects::emit_x86_64(emitter);
        return;
    }

    aarch64::emit(emitter);
    arrays::emit_aarch64(emitter);
    objects::emit_aarch64(emitter);
}
