//! Purpose:
//! Checks that JSON traversal does not consume native stack per container level.
//!
//! Called from:
//! - The parent interpreter JSON builtin test module.
//!
//! Key details:
//! - A subprocess confines stack-overflow failures to the regression probe.
//! - Flat fake handle storage isolates encoder recursion from runtime destruction.

use super::*;

/// Deep arrays and objects encode on a small thread stack with PHP depth results.
#[test]
fn execute_program_json_deep_containers_keep_native_stack_bounded() {
    const CHILD: &str = "ELEPHC_JSON_DEEP_STACK_CHILD";
    if std::env::var_os(CHILD).is_some() {
        std::thread::Builder::new().stack_size(256 * 1024).spawn(|| {
            for objects in [false, true] {
                let mut values = FakeOps::default();
                let mut root = values.int(1).unwrap();
                for _ in 0..2000 {
                    root = if objects {
                        let object = values.new_object("stdClass").unwrap();
                        values.property_set(object, "next", root).unwrap();
                        object
                    } else {
                        values.alloc(FakeValue::Array(vec![root]))
                    };
                }
                for flags in [0, 512, 512 | 4194304, 512 | 16] {
                    let mut context = ElephcEvalContext::new();
                    let flag_value = values.int(flags).unwrap();
                    let depth = values.int(1).unwrap();
                    let result = eval_json_encode_values_result(
                        &[root, flag_value, depth], &mut context, &mut values,
                    ).expect("depth failure must remain a JSON result");
                    assert_eq!(context.json_last_error(), 1);
                    if flags == 0 {
                        assert!(matches!(values.get(result), FakeValue::Bool(false)));
                    } else {
                        let (open, close) = if objects {
                            ("{\"next\":", "}")
                        } else if flags & 16 != 0 {
                            ("{\"0\":", "}")
                        } else { ("[", "]") };
                        let expected = format!("{}1{}", open.repeat(2000), close.repeat(2000));
                        assert_eq!(values.string_bytes(result).unwrap(), expected.as_bytes());
                    }
                }
            }
        }).unwrap().join().expect("deep traversal thread");
        return;
    }
    let mut command = std::process::Command::new(std::env::current_exe().unwrap());
    command.args([
        "--exact",
        "interpreter::tests::builtins_json::deep::execute_program_json_deep_containers_keep_native_stack_bounded",
        "--nocapture",
    ]).env(CHILD, "1");
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // The deliberately crashing regression must not leave a core dump.
        unsafe {
            command.pre_exec(|| {
                let limit = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
                if libc::setrlimit(libc::RLIMIT_CORE, &limit) == 0 { Ok(()) }
                else { Err(std::io::Error::last_os_error()) }
            });
        }
    }
    let output = command.output().expect("run isolated deep JSON probe");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
}
