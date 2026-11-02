//! Purpose:
//! Answers the `Generator` iteration protocol for generators the interpreter created,
//! on behalf of the generated runtime's `__rt_gen_*` helpers.
//!
//! Called from:
//! - The generated runtime, through the pointer installed in
//!   `_elephc_eval_generator_protocol_fn`.
//!
//! Key details:
//! - `Generator` is an AOT class, so a generator the INTERPRETER created is
//!   indistinguishable from a native one at a `foreach`, an `iterator_to_array()` or any
//!   other native iteration site. The native helpers drive it through the Fiber fields a
//!   `Generator` object carries at offsets 184..224 — fields an interpreted generator never
//!   filled, because its state lives in an `EvalGeneratorFrame` keyed by object identity.
//!   Reading them yields whatever the object storage happens to hold, which is how
//!   `foreach` over an interpreted generator reached `__rt_mixed_unbox` on a non-pointer.
//! - The hook therefore answers BEFORE the fiber fields are touched, and answers only for
//!   identities an eval context has registered as a generator; every other receiver falls
//!   through to the native path, so a program with no interpreted generator pays one load
//!   and one null check per protocol step.

use crate::abi::{ElephcEvalContext, ABI_VERSION};
use crate::ffi::dynamic_destructors::dynamic_object_owner_context;
use crate::interpreter::eval_generator_protocol_result;
use crate::interpreter::RuntimeValueOps;
use crate::runtime_hooks::{self, ElephcRuntimeOps};
use crate::value::{RuntimeCell, RuntimeCellHandle};

/// Protocol operation codes shared with the emitted `__rt_gen_*` probes.
///
/// The generated assembly passes these as an immediate, so the numbering is ABI: keep it in
/// step with `src/codegen_support/runtime/generators/coro.rs`.
const OP_CURRENT: u64 = 0;
const OP_KEY: u64 = 1;
const OP_VALID: u64 = 2;
const OP_NEXT: u64 = 3;
const OP_REWIND: u64 = 4;
const OP_SEND: u64 = 5;
const OP_THROW: u64 = 6;
const OP_GET_RETURN: u64 = 7;

/// Answer meaning "a Throwable escaped this protocol step; `out` holds the boxed cell".
///
/// The caller is always COMPILED code -- an interpreted `foreach` drives its own generators
/// without this bridge -- so nothing above would ever look at a Throwable left on the eval
/// context. The generated probe unboxes what `out` carries into `_exc_value` and enters
/// `__rt_throw_current`, exactly as the dynamic-callable invoker does for its status 3.
const PROTOCOL_THROWN: u64 = 2;

/// Installs the eval generator-protocol callback into the generated runtime.
pub(crate) fn install_generator_protocol_hook() {
    unsafe {
        runtime_hooks::install_generator_protocol_hook(
            __elephc_eval_generator_protocol as *const () as usize,
        );
    }
}

/// Returns the PHP method name one protocol opcode stands for.
fn protocol_method_name(op: u64) -> Option<&'static str> {
    match op {
        OP_CURRENT => Some("current"),
        OP_KEY => Some("key"),
        OP_VALID => Some("valid"),
        OP_NEXT => Some("next"),
        OP_REWIND => Some("rewind"),
        OP_SEND => Some("send"),
        OP_THROW => Some("throw"),
        OP_GET_RETURN => Some("getreturn"),
        _ => None,
    }
}

/// Answers one `Generator` protocol step for an interpreter-created generator.
///
/// Returns 1 when this generator belongs to an eval context and `out` was written, and 0
/// when it does not, which tells the caller to use its native fiber path. `out` receives a
/// boxed Mixed cell the caller then owns for `current`, `key`, `send`, `throw` and
/// `getReturn`; a plain 0/1 for `valid`; and zero for `next` and `rewind`.
///
/// # Safety
/// `generator` must be null or a live elephc runtime object pointer, `argument` must be null
/// or a borrowed boxed Mixed cell, and `out` must point at a writable native word.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_generator_protocol(
    generator: *mut RuntimeCell,
    op: u64,
    argument: *mut RuntimeCell,
    out: *mut u64,
) -> u64 {
    crate::ffi::util::trace_eval_ffi_entry("__elephc_eval_generator_protocol");
    std::panic::catch_unwind(|| unsafe { generator_protocol_inner(generator, op, argument, out) })
        .unwrap_or(0)
}

/// Executes the callback body after the exported ABI shim has installed a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_generator_protocol`.
unsafe fn generator_protocol_inner(
    generator: *mut RuntimeCell,
    op: u64,
    argument: *mut RuntimeCell,
    out: *mut u64,
) -> u64 {
    if generator.is_null() || out.is_null() {
        return 0;
    }
    let Some(method_name) = protocol_method_name(op) else {
        return 0;
    };
    let identity = generator as u64;
    let Some(context) = dynamic_object_owner_context(identity) else {
        return 0;
    };
    let Some(context) = (unsafe { context.as_mut() }) else {
        return 0;
    };
    if context.abi_version() != ABI_VERSION {
        return 0;
    }
    // Ownership of an identity is published for closures and eval-declared objects too, so a
    // registered owner is not by itself proof this receiver is an interpreted generator.
    if !context.has_eval_generator(identity) {
        return 0;
    }
    let context_ptr = context as *mut ElephcEvalContext;
    let mut values = ElephcRuntimeOps::with_context(context_ptr);
    let argument = (!argument.is_null()).then(|| RuntimeCellHandle::from_raw(argument));
    let context = unsafe { &mut *context_ptr };
    let result =
        eval_generator_protocol_result(identity, method_name, argument, context, &mut values);
    let answer = match result {
        Ok(Some(value)) => value,
        // A protocol step the interpreter refuses (`rewind()` after the generator advanced,
        // `getReturn()` before it returned) or one whose BODY threw records its Throwable on
        // the context. Hand it back for native unwinding: the compiled `foreach` that asked
        // has no bridge frame above it to find one there, and without this a generator body
        // that threw left that loop spinning on a generator which could neither advance nor
        // say why. Answering "handled, null" when nothing was recorded keeps the native fiber
        // path — whose fields this generator never had — out of the picture either way.
        Ok(None) | Err(_) => {
            if let Some(thrown) = context.take_pending_throw() {
                unsafe { *out = thrown.as_ptr() as u64 };
                return PROTOCOL_THROWN;
            }
            unsafe { *out = 0 };
            return 1;
        }
    };
    let written = match op {
        OP_NEXT | OP_REWIND => {
            let _ = values.release(answer);
            0
        }
        OP_VALID => {
            let valid = values.truthy(answer).unwrap_or(false);
            let _ = values.release(answer);
            u64::from(valid)
        }
        _ => answer.as_ptr() as u64,
    };
    unsafe { *out = written };
    1
}
