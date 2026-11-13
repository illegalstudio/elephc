//! Purpose:
//! Verifies that asynchronous signals observe completed bytecode source statements.
//!
//! Called from:
//! - The Magician library test harness with the `bytecode_slice_` filter.
//!
//! Key details:
//! - A fresh test subprocess isolates real signal dispositions from parallel tests.
//! - A native callback queues SIGUSR1 during an assignment without a timing race.

use super::super::*;
use super::support::*;

/// Queues a signal on the calling thread and returns the descriptor's existing test owner.
unsafe extern "C" fn queue_bytecode_signal(
    descriptor: *mut std::ffi::c_void,
    _args: *mut crate::value::RuntimeCell,
    _argc: i64,
) -> *mut crate::value::RuntimeCell {
    unsafe { libc::raise(libc::SIGUSR1); }
    descriptor.cast()
}

/// A handler queued during the RHS sees the completed assignment at the next source statement.
#[test]
fn bytecode_slice_dispatches_signals_at_statement_boundaries() {
    const CHILD: &str = "ELEPHC_BYTECODE_SIGNAL_PROBE";
    if std::env::var_os(CHILD).is_none() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "interpreter::tests::bytecode_signals::bytecode_slice_dispatches_signals_at_statement_boundaries",
                "--test-threads=1", "--nocapture"])
            .env(CHILD, "1")
            .output().unwrap();
        assert!(output.status.success(), "signal probe failed:\n{}\n{}",
            String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
        return;
    }
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    context.set_global_scope(&mut scope);
    let setup = parse_fragment(b"
$x = 0;
function bytecode_signal_handler($signal) { global $x; echo $x; }
pcntl_signal(SIGUSR1, 'bytecode_signal_handler');
pcntl_async_signals(true);
").unwrap();
    execute_program_with_context(&mut context, &setup, &mut scope, &mut values).unwrap();
    let one = values.int(1).unwrap();
    let native = NativeFunction::new(one.as_ptr().cast(), queue_bytecode_signal, 0);
    assert!(context.define_native_function("bytecode_raise", native).is_ok());
    let program = parse_fragment(b"
if (false) { function bytecode_raise() { return 0; } }
$x = bytecode_raise() + 1;
echo ':';
echo $x;
").unwrap();
    assert!(bytecode::compile_program(&program).is_ok());
    execute_program_with_context(&mut context, &program, &mut scope, &mut values).unwrap();
    let cleanup = parse_fragment(b"pcntl_async_signals(false); pcntl_signal(SIGUSR1, SIG_DFL);").unwrap();
    execute_program_with_context(&mut context, &cleanup, &mut scope, &mut values).unwrap();
    assert_eq!(values.output, "2:2");
}
