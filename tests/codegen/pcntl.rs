//! Purpose:
//! End-to-end codegen coverage for target-aware PCNTL constants and process-control builtins.
//!
//! Called from:
//! - `cargo test --test codegen_tests pcntl` through Rust's test harness.
//!
//! Key details:
//! - Constant expectations are selected for the host target because signal and errno values follow libc.

use crate::support::*;

/// Verifies common PCNTL constants resolve through namespaced PHP code and emit target values.
#[test]
fn test_pcntl_common_constants_are_target_aware() {
    let out = compile_and_run(
        "<?php namespace Demo; echo \\SIGCHLD . '|' . \\PCNTL_EAGAIN . '|' . \\WNOHANG;",
    );

    #[cfg(target_os = "macos")]
    assert_eq!(out, "20|35|1");
    #[cfg(target_os = "linux")]
    assert_eq!(out, "17|11|1");
}

/// Treats a PCNTL constant reference itself as extension usage for linker identity.
#[test]
fn test_pcntl_constant_auto_loads_extension() {
    let out = compile_and_run(
        "<?php echo (defined('SIGCHLD') ? 'defined' : 'missing') . '|'
            . (extension_loaded('pcntl') ? 'loaded' : 'missing');",
    );
    assert_eq!(out, "defined|loaded");
}

/// Verifies Linux-only PCNTL namespace and siginfo constants compile to their libc values.
#[cfg(target_os = "linux")]
#[test]
fn test_pcntl_linux_only_constants() {
    let out = compile_and_run(
        "<?php echo CLONE_NEWNS . '|' . SI_QUEUE . '|' . P_PIDFD . '|' . WNOWAIT;",
    );
    assert_eq!(out, "131072|-1|3|16777216");
}

/// Verifies macOS-only Darwin priority constants compile to their libc values.
#[cfg(target_os = "macos")]
#[test]
fn test_pcntl_macos_only_constants() {
    let out = compile_and_run("<?php echo PRIO_DARWIN_BG . '|' . PRIO_DARWIN_THREAD;");
    assert_eq!(out, "4096|3");
}

/// Verifies scalar PCNTL calls lower through the bridge and auto-load its extension identity.
#[test]
fn test_pcntl_scalar_bridge_and_extension_loading() {
    let out = compile_and_run(
        "<?php
        $message = pcntl_strerror(PCNTL_EINVAL);
        echo (strlen($message) > 0 ? 'message' : 'empty') . '|';
        echo pcntl_alarm(0) . '|';
        echo pcntl_errno() . ':' . pcntl_get_last_error() . '|';
        echo (extension_loaded('pcntl') ? 'loaded' : 'missing');",
    );
    assert_eq!(out, "message|0|0:0|loaded");
}

/// Preserves checker-resolved PCNTL result types when an untyped closure returns them directly.
#[test]
fn test_pcntl_untyped_closure_preserves_builtin_return_type() {
    let out = compile_and_run(
        "<?php
        $message = function () { return pcntl_strerror(22); };
        $registered = function () { return pcntl_signal(SIGUSR1, SIG_IGN); };
        echo gettype($message()) . ':' . (strlen($message()) > 0 ? 'message' : 'empty') . '|';
        echo gettype($registered()) . ':' . ($registered() ? 'true' : 'false');
        pcntl_signal(SIGUSR1, SIG_DFL);",
    );
    assert_eq!(out, "string:message|boolean:true");
}

/// Reports both PCNTL and POSIX as loaded surfaces when their shared bridge is linked.
#[test]
fn test_pcntl_bridge_reports_posix_extension_surface() {
    let out = compile_and_run(
        "<?php $signal = SIGCHLD;
        echo extension_loaded('pcntl') ? 'pcntl|' : 'bad|';
        echo extension_loaded('posix') ? 'posix' : 'bad';",
    );
    assert_eq!(out, "pcntl|posix");
}

/// Exposes PHP's target-derived OS family constant alongside `PHP_OS`.
#[test]
fn test_pcntl_php_os_family_matches_host() {
    let out = compile_and_run("<?php echo PHP_OS . '|' . PHP_OS_FAMILY;");
    #[cfg(target_os = "macos")]
    assert_eq!(out, "Darwin|Darwin");
    #[cfg(target_os = "linux")]
    assert_eq!(out, "Linux|Linux");
}

/// Exposes the same target-derived OS family inside opaque eval fragments.
#[test]
fn test_pcntl_eval_php_os_family_matches_host() {
    let out = compile_and_run(
        "<?php echo eval('return (defined(\"PHP_OS_FAMILY\") ? PHP_OS_FAMILY : \"missing\");');",
    );
    #[cfg(target_os = "macos")]
    assert_eq!(out, "Darwin");
    #[cfg(target_os = "linux")]
    assert_eq!(out, "Linux");
}

/// Prunes target-unavailable PCNTL calls behind literal availability and OS-family guards.
#[test]
fn test_pcntl_target_guards_prune_unavailable_calls_before_typecheck() {
    #[cfg(target_os = "linux")]
    let source = "<?php
        function guarded_target_call(): string {
            if (function_exists('pcntl_getqos_class')) { pcntl_getqos_class(); return 'bad'; }
            return 'nested';
        }
        if (function_exists('pcntl_getqos_class')) { pcntl_getqos_class(); }
        else { echo 'function|'; }
        if (PHP_OS_FAMILY === 'Darwin') { pcntl_getqos_class(); }
        else { echo 'family|'; }
        echo guarded_target_call();";
    #[cfg(target_os = "macos")]
    let source = "<?php
        function guarded_target_call(): string {
            if (function_exists('pcntl_getcpu')) { pcntl_getcpu(); return 'bad'; }
            return 'nested';
        }
        if (function_exists('pcntl_getcpu')) { pcntl_getcpu(); }
        else { echo 'function|'; }
        if (PHP_OS_FAMILY === 'Linux') { pcntl_getcpu(); }
        else { echo 'family|'; }
        echo guarded_target_call();";
    assert_eq!(compile_and_run(source), "function|family|nested");
}

/// Allows a user polyfill to own a registry name when that builtin is unavailable on the target.
#[test]
fn test_pcntl_target_unavailable_builtin_can_be_polyfilled() {
    #[cfg(target_os = "linux")]
    let source = "<?php
        if (!function_exists('pcntl_getqos_class')) {
            function pcntl_getqos_class(): int { return 7; }
        }
        echo pcntl_getqos_class();";
    #[cfg(target_os = "macos")]
    let source = "<?php
        if (!function_exists('pcntl_getcpu')) {
            function pcntl_getcpu(): int { return 7; }
        }
        echo pcntl_getcpu();";
    assert_eq!(compile_and_run(source), "7");
}

/// Reports a retained target polyfill as defined to later `function_exists()` probes.
#[test]
fn test_pcntl_target_polyfill_updates_later_function_exists() {
    #[cfg(target_os = "linux")]
    let source = "<?php
        if (!function_exists('pcntl_getqos_class')) {
            function pcntl_getqos_class(): int { return 7; }
        }
        var_dump(function_exists('pcntl_getqos_class'));
        var_dump(pcntl_getqos_class());";
    #[cfg(target_os = "macos")]
    let source = "<?php
        if (!function_exists('pcntl_getcpu')) {
            function pcntl_getcpu(): int { return 7; }
        }
        var_dump(function_exists('pcntl_getcpu'));
        var_dump(pcntl_getcpu());";
    assert_eq!(compile_and_run(source), "bool(true)\nint(7)\n");
}

/// Resolves a namespaced guarded polyfill before falling back to a global target builtin.
#[test]
fn test_pcntl_namespaced_target_unavailable_builtin_can_be_polyfilled() {
    #[cfg(target_os = "linux")]
    let source = "<?php namespace App;
        if (!function_exists('pcntl_getqos_class')) {
            function pcntl_getqos_class(): int { return 7; }
        }
        echo pcntl_getqos_class();";
    #[cfg(target_os = "macos")]
    let source = "<?php namespace App;
        if (!function_exists('pcntl_getcpu')) {
            function pcntl_getcpu(): int { return 7; }
        }
        echo pcntl_getcpu();";
    assert_eq!(compile_and_run(source), "7");
}

/// Returns the previous alarm's remaining time while cancelling it.
#[test]
fn test_pcntl_alarm_returns_previous_remaining_seconds() {
    let out = compile_and_run(
        "<?php
        echo pcntl_alarm(3) . '|';
        echo pcntl_alarm(0) > 0 ? 'remaining' : 'missing';",
    );
    assert_eq!(out, "0|remaining");
}

/// Creates process groups and sessions through the shared POSIX bridge.
#[test]
fn test_pcntl_posix_process_group_and_session_helpers() {
    let out = compile_and_run(
        r#"<?php
        $group_child = pcntl_fork();
        if ($group_child === 0) {
            echo \PoSiX_SeTpGiD(0, 0) ? 'group' : 'group-failed';
            exit(0);
        }
        pcntl_waitpid($group_child, $status);
        echo '|';
        $session_child = pcntl_fork();
        if ($session_child === 0) {
            echo \PoSiX_SeTsId() > 0 ? 'session' : 'session-failed';
            exit(0);
        }
        pcntl_waitpid($session_child, $status);"#,
    );
    assert_eq!(out, "group|session");
}

/// Detaches through Elephc's daemon helper while retaining the test output descriptors.
#[test]
fn test_pcntl_daemon_preserves_requested_process_state() {
    let out = compile_and_run(
        r#"<?php
        if (!\PcNtL_DaEmOn(no_chdir: true, no_close: true)) {
            echo 'failed';
            exit(1);
        }
        echo 'daemon';"#,
    );
    assert_eq!(out, "daemon");
}

/// Exposes process-group and session helpers through Magician's PCNTL bridge bindings.
#[test]
fn test_pcntl_eval_process_group_and_session_helpers() {
    let out = compile_and_run(
        r#"<?php
        echo eval('
            $group_child = pcntl_fork();
            if ($group_child === 0) {
                echo posix_setpgid(process_id: 0, process_group_id: 0) ? "group" : "failed";
                exit(0);
            }
            pcntl_waitpid($group_child, $status);
            echo "|";
            $session_child = pcntl_fork();
            if ($session_child === 0) {
                echo posix_setsid() > 0 ? "session" : "failed";
                exit(0);
            }
            pcntl_waitpid($session_child, $status);
        ');"#,
    );
    assert_eq!(out, "group|session");
}

/// Detaches from inside eval while preserving descriptors requested by named arguments.
#[test]
fn test_pcntl_eval_daemon_preserves_requested_process_state() {
    let out = compile_and_run(
        r#"<?php
        echo eval('
            if (!pcntl_daemon(no_chdir: true, no_close: true)) {
                return "failed";
            }
            return "daemon";
        ');"#,
    );
    assert_eq!(out, "daemon");
}

/// Verifies target-native wait status helpers preserve boolean and mixed result encodings.
#[test]
fn test_pcntl_wait_status_decoders() {
    let out = compile_and_run(
        "<?php
        $exit = 23 << 8;
        echo pcntl_wifexited($exit) . '|';
        echo pcntl_wexitstatus($exit) . '|';
        echo pcntl_wifsignaled(15) . '|';
        echo pcntl_wtermsig(15) . '|';
        echo pcntl_wifstopped(127) . '|';
        echo pcntl_wstopsig((SIGSTOP << 8) | 127) . '|';
        echo pcntl_wifcontinued(65535);",
    );
    #[cfg(target_os = "macos")]
    assert_eq!(out, "1|23|1|15|1|17|");
    #[cfg(target_os = "linux")]
    assert_eq!(out, "1|23|1|15|1|19|1");
}

/// Verifies priority lookup returns an integer without confusing a valid `-1` with failure.
#[test]
fn test_pcntl_getpriority_returns_int() {
    let out = compile_and_run(
        "<?php $priority = pcntl_getpriority(); echo is_int($priority) ? 'int' : 'failure';",
    );
    assert_eq!(out, "int");
}

/// Rejects unsupported dynamic priority modes in both compiled and eval calls.
#[test]
fn test_pcntl_priority_rejects_dynamic_invalid_modes() {
    let out = compile_and_run(
        r#"<?php
        $mode = 999;
        try { pcntl_getpriority(0, $mode); }
        catch (ValueError $error) { echo 'get|'; }
        try { pcntl_setpriority(0, 0, $mode); }
        catch (ValueError $error) { echo 'set|'; }
        echo eval('
            $mode = 999;
            try { pcntl_getpriority(0, $mode); }
            catch (ValueError $error) { echo "eval-get|"; }
            try { pcntl_setpriority(0, 0, $mode); }
            catch (ValueError $error) { echo "eval-set"; }
        ');"#,
    );
    assert_eq!(out, "get|set|eval-get|eval-set");
}

/// Emits php-src's warnings before returning false for missing priority targets.
#[test]
fn test_pcntl_priority_os_failures_emit_warnings() {
    let output = compile_and_run_capture(
        "<?php
        var_dump(pcntl_getpriority(999999));
        var_dump(pcntl_setpriority(0, 999999));",
    );
    assert_eq!(output.stdout, "bool(false)\nbool(false)\n");
    assert!(
        output.stderr.contains(
            "Warning: pcntl_getpriority(): Error 3: No process was located using the given parameters"
        ),
        "{}",
        output.stderr
    );
    assert!(
        output.stderr.contains(
            "Warning: pcntl_setpriority(): Error 3: No process was located using the given parameters"
        ),
        "{}",
        output.stderr
    );
}

/// Reapplies the current process priority without requiring elevated privileges.
#[test]
fn test_pcntl_setpriority_reapplies_current_priority() {
    let out = compile_and_run(
        "<?php
        $priority = pcntl_getpriority();
        echo is_int($priority) ? 'int|' : 'bad|';
        echo pcntl_setpriority($priority) ? 'set' : 'failure';",
    );
    assert_eq!(out, "int|set");
}

/// Changes and restores the current signal mask while materializing the prior set by reference.
#[test]
fn test_pcntl_signal_mask_round_trip() {
    let out = compile_and_run(
        "<?php
        $old = [];
        $blocked = pcntl_sigprocmask(SIG_BLOCK, [SIGUSR1], $old);
        echo ($blocked ? 'blocked' : 'bad') . '|';
        echo (is_array($old) ? 'array' : 'bad') . '|';
        echo (pcntl_sigprocmask(SIG_SETMASK, $old) ? 'restored' : 'bad');",
    );
    assert_eq!(out, "blocked|array|restored");
}

/// Coerces numeric strings and ignores associative keys in literal PCNTL signal sets.
#[test]
fn test_pcntl_signal_masks_accept_php_coercible_literal_arrays() {
    let out = compile_and_run(
        "<?php
        echo pcntl_sigprocmask(SIG_BLOCK, ['9']) ? 'numeric|' : 'bad|';
        echo pcntl_sigprocmask(SIG_BLOCK, ['term' => SIGTERM]) ? 'assoc|' : 'bad|';
        echo pcntl_sigprocmask(SIG_UNBLOCK, ['term' => SIGTERM]) ? 'done' : 'bad';",
    );
    assert_eq!(out, "numeric|assoc|done");
}

/// Applies signal-value coercion to variables and associative storage, not only inline literals.
#[test]
fn test_pcntl_signal_masks_accept_php_coercible_variable_arrays() {
    let out = compile_and_run_capture(
        "<?php
        $indexed = ['9abc'];
        $assoc = ['term' => '15'];
        echo pcntl_sigprocmask(SIG_BLOCK, $indexed) ? 'indexed|' : 'bad|';
        echo pcntl_sigprocmask(SIG_BLOCK, $assoc) ? 'assoc|' : 'bad|';
        echo pcntl_sigprocmask(SIG_UNBLOCK, [9, 15]) ? 'done' : 'bad';",
    );
    assert_eq!(out.stdout, "indexed|assoc|done");
    assert!(
        out.stderr.contains("Warning: A non-numeric value encountered"),
        "{}",
        out.stderr
    );
}

/// Emits PHP's float-string precision deprecation while still coercing the signal number.
#[test]
fn test_pcntl_signal_float_string_precision_loss_is_deprecated() {
    let out = compile_and_run_capture(
        "<?php
        $signals = ['9.7'];
        echo pcntl_sigprocmask(SIG_BLOCK, $signals) ? 'blocked|' : 'bad|';
        echo pcntl_sigprocmask(SIG_UNBLOCK, [9]) ? 'done' : 'bad';",
    );
    assert_eq!(out.stdout, "blocked|done");
    assert!(
        out.stderr.contains(
            "Deprecated: Implicit conversion from float-string \"9.7\" to int loses precision"
        ),
        "{}",
        out.stderr
    );
}

/// Emits PHP's real-float precision deprecation in AOT and eval signal arrays.
#[test]
fn test_pcntl_signal_float_precision_loss_is_deprecated() {
    let out = compile_and_run_capture(
        r#"<?php
        $signals = [9.7];
        echo pcntl_sigprocmask(SIG_BLOCK, $signals) ? 'aot|' : 'bad|';
        pcntl_sigprocmask(SIG_UNBLOCK, [9]);
        echo eval('
            $signals = [9.7];
            $ok = pcntl_sigprocmask(SIG_BLOCK, $signals);
            pcntl_sigprocmask(SIG_UNBLOCK, [9]);
            return $ok ? "eval" : "bad";
        ');"#,
    );
    assert_eq!(out.stdout, "aot|eval");
    assert_eq!(
        out.stderr
            .matches("Deprecated: Implicit conversion from float 9.7 to int loses precision")
            .count(),
        2,
        "{}",
        out.stderr
    );
}

/// Emits cast warnings for non-representable signal floats and both diagnostics for NaN.
#[test]
fn test_pcntl_signal_nonrepresentable_float_diagnostics() {
    let out = compile_and_run_capture(
        r#"<?php
        $n = $argc;
        foreach ([INF * $n, NAN * $n, 1e20 * $n] as $signal) {
            try { pcntl_sigprocmask(SIG_BLOCK, [$signal]); }
            catch (ValueError $error) { echo "value|"; }
        }"#,
    );
    assert_eq!(out.stdout, "value|value|value|");
    assert!(
        out.stderr
            .contains("Warning: The float INF is not representable as an int, cast occurred"),
        "{}",
        out.stderr
    );
    assert!(
        out.stderr
            .contains("Warning: The float NAN is not representable as an int, cast occurred"),
        "{}",
        out.stderr
    );
    assert!(
        out.stderr
            .contains("Deprecated: Implicit conversion from float NAN to int loses precision"),
        "{}",
        out.stderr
    );
    assert!(
        out.stderr.contains(
            "Warning: The float 1.0E+20 is not representable as an int, cast occurred"
        ),
        "{}",
        out.stderr
    );
    assert_eq!(out.stderr.matches("Warning: The float ").count(), 3);
    assert_eq!(
        out.stderr
            .matches("Deprecated: Implicit conversion from float ")
            .count(),
        1
    );
}

/// Throws `TypeError`, rather than a signal-range `ValueError`, for eval nonnumeric strings.
#[test]
fn test_pcntl_eval_signal_mask_rejects_nonnumeric_string_with_type_error() {
    let out = compile_and_run(
        r#"<?php echo eval('
            try { pcntl_sigprocmask(SIG_BLOCK, ["abc"]); }
            catch (TypeError $error) { return "type"; }
            catch (ValueError $error) { return "value"; }
            return "bad";
        ');"#,
    );
    assert_eq!(out, "type");
}

/// Warns and uses the leading numeric prefix for eval signal-set strings.
#[test]
fn test_pcntl_eval_signal_mask_accepts_leading_numeric_string() {
    let out = compile_and_run_capture(
        r#"<?php echo eval('
            $signals = ["9abc"];
            $ok = pcntl_sigprocmask(SIG_BLOCK, $signals);
            pcntl_sigprocmask(SIG_UNBLOCK, [9]);
            return $ok ? "blocked" : "bad";
        ');"#,
    );
    assert_eq!(out.stdout, "blocked");
    assert!(
        out.stderr.contains("Warning: A non-numeric value encountered"),
        "{}",
        out.stderr
    );
}

/// Emits PHP's precision-loss deprecation while coercing an eval float string signal.
#[test]
fn test_pcntl_eval_signal_float_string_precision_loss_is_deprecated() {
    let out = compile_and_run_capture(
        r#"<?php echo eval('
            $signals = ["9.7"];
            $ok = pcntl_sigprocmask(SIG_BLOCK, $signals);
            pcntl_sigprocmask(SIG_UNBLOCK, [9]);
            return $ok ? "blocked" : "bad";
        ');"#,
    );
    assert_eq!(out.stdout, "blocked");
    assert!(
        out.stderr.contains(
            "Deprecated: Implicit conversion from float-string \"9.7\" to int loses precision"
        ),
        "{}",
        out.stderr
    );
}

/// Throws the exact catchable type error for object signal-set values in AOT and eval.
#[test]
fn test_pcntl_signal_mask_rejects_object_values_at_runtime() {
    let out = compile_and_run(
        r#"<?php
        class SignalObject {}
        try { pcntl_sigprocmask(SIG_BLOCK, [new SignalObject()]); }
        catch (TypeError $error) { echo $error->getMessage() . "|"; }
        echo eval('
            class EvalSignalObject {}
            try { pcntl_sigprocmask(SIG_BLOCK, [new EvalSignalObject()]); }
            catch (TypeError $error) { return $error->getMessage(); }
            return "bad";
        ');"#,
    );
    assert_eq!(
        out,
        "pcntl_sigprocmask(): Argument #2 ($signals) signals must be of type int, SignalObject given|\
pcntl_sigprocmask(): Argument #2 ($signals) signals must be of type int, EvalSignalObject given"
    );
}

/// Throws the same catchable `TypeError` for nonnumeric strings in AOT variable arrays.
#[test]
fn test_pcntl_aot_signal_mask_rejects_nonnumeric_variable_string() {
    let out = compile_and_run(
        "<?php
        $signals = ['abc'];
        try { pcntl_sigprocmask(SIG_BLOCK, $signals); }
        catch (TypeError $error) { echo 'type'; }
        catch (ValueError $error) { echo 'value'; }",
    );
    assert_eq!(out, "type");
}

/// Keeps named optional PCNTL outputs absent instead of lowering default arrays as lvalues.
#[cfg(target_os = "linux")]
#[test]
fn test_pcntl_named_optional_outputs_remain_omitted() {
    let out = compile_and_run(
        "<?php
        $masked = pcntl_sigprocmask(mode: SIG_BLOCK, signals: [SIGUSR1]);
        $timed = pcntl_sigtimedwait(signals: [SIGUSR2], nanoseconds: 1);
        pcntl_sigprocmask(mode: SIG_UNBLOCK, signals: [SIGUSR1]);
        $status = 7;
        $waited = pcntl_wait(status: $status, flags: WNOHANG);
        echo ($masked ? 'masked' : 'bad') . '|';
        echo ($timed === false ? 'timeout' : 'bad') . '|';
        echo ($waited === -1 ? 'waited' : 'bad');",
    );
    assert_eq!(out, "masked|timeout|waited");
}

/// Raises PHP's runtime mask and signal-array `ValueError`s for dynamic inputs.
#[test]
fn test_pcntl_signal_mask_rejects_dynamic_invalid_values() {
    let out = compile_and_run(
        "<?php
        $mode = 999;
        $signals = [SIGUSR1];
        try { pcntl_sigprocmask($mode, $signals); }
        catch (ValueError $error) { echo 'mode|'; }
        $mode = SIG_BLOCK;
        $signals = [];
        try { pcntl_sigprocmask($mode, $signals); }
        catch (ValueError $error) { echo 'empty|'; }
        $signals = [999];
        try { pcntl_sigprocmask($mode, $signals); }
        catch (ValueError $error) { echo 'range'; }",
    );
    assert_eq!(out, "mode|empty|range");
}

/// Registers a callable, retrieves it, and dispatches SIGALRM with stable siginfo.
#[test]
fn test_pcntl_signal_handler_dispatch_and_lookup() {
    let out = compile_and_run(
        "<?php
        function handle_alarm(int $signal, array $info): void {
            echo 'handled:' . $signal . ':' . $info['signo'] . '|';
        }
        echo (pcntl_signal(SIGALRM, 'handle_alarm') ? 'set' : 'bad') . '|';
        $handler = pcntl_signal_get_handler(SIGALRM);
        echo (is_callable($handler) ? 'callable' : 'bad') . '|';
        pcntl_alarm(1);
        sleep(2);
        echo (pcntl_signal_dispatch() ? 'dispatched' : 'bad') . '|';
        echo (pcntl_signal(SIGALRM, SIG_DFL) ? 'reset' : 'bad');",
    );
    assert_eq!(out, "set|callable|handled:14:14|dispatched|reset");
}

/// Preserves the PHP-visible string, Closure, and array shapes returned by handler lookup.
#[test]
fn test_pcntl_signal_get_handler_preserves_callable_value_shape() {
    let out = compile_and_run(
        r#"<?php
        function namedHandler(int $signal): void {}
        pcntl_signal(SIGUSR1, 'namedHandler');
        $named = pcntl_signal_get_handler(SIGUSR1);
        var_dump($named);
        echo gettype($named) . '|' . (int) is_string($named) . '|'
            . (int) is_callable($named) . '|' . (int) ($named === null) . "\n";

        $closure = function(int $signal): void {};
        pcntl_signal(SIGUSR1, $closure);
        $returnedClosure = pcntl_signal_get_handler(SIGUSR1);
        var_dump($returnedClosure);
        echo gettype($returnedClosure) . '|' . (int) is_object($returnedClosure) . '|'
            . (int) is_callable($returnedClosure) . '|' . (int) ($returnedClosure === null) . "\n";

        class SignalFixture { public static function handle(int $signal): void {} }
        pcntl_signal(SIGUSR1, ['SignalFixture', 'handle']);
        $method = pcntl_signal_get_handler(SIGUSR1);
        echo gettype($method) . '|' . (int) is_array($method) . '|'
            . (int) is_callable($method) . '|' . (int) ($method === null);
        pcntl_signal(SIGUSR1, SIG_DFL);"#,
    );
    assert_eq!(
        out,
        "string(12) \"namedHandler\"\n\
string|1|1|0\n\
object(Closure)#1 (0) {\n}\n\
object|1|1|0\n\
array|1|1|0"
    );
}

/// Lets eval introspect the original PHP value installed by compiled AOT signal code.
#[test]
fn test_pcntl_eval_reads_aot_installed_handler_value() {
    let out = compile_and_run(
        r#"<?php
        function crossBackendHandler(int $signal): void {}
        pcntl_signal(SIGUSR1, 'crossBackendHandler');
        echo eval('
            $handler = pcntl_signal_get_handler(SIGUSR1);
            echo gettype($handler) . "|" . (int) is_string($handler);
            return is_callable($handler) ? "|1" : "|0";
        ');
        pcntl_signal(SIGUSR1, SIG_DFL);"#,
    );
    assert_eq!(out, "string|1|1");
}

/// Turns an unknown runtime handler name into PHP's catchable `TypeError`.
#[test]
fn test_pcntl_signal_unknown_runtime_handler_is_catchable() {
    let out = compile_and_run(
        "<?php
        function knownSignalHandler(int $signal): void {}
        $handler = 'missingSignalHandler';
        try { pcntl_signal(SIGUSR1, $handler); }
        catch (TypeError $error) { echo get_class($error) . '|'; }
        echo 'still-alive';",
    );
    assert_eq!(out, "TypeError|still-alive");
}

/// Preserves the SIGALRM-aware restart default for a named call that omits the third argument.
#[test]
fn test_pcntl_signal_named_omitted_restart_uses_signal_aware_default() {
    let dir = make_cli_test_dir("elephc_pcntl_signal_named_restart");
    let (user_asm, _, _) = compile_source_to_asm_with_options(
        "<?php pcntl_signal(signal: SIGALRM, handler: SIG_IGN);",
        &dir,
        8_388_608,
        false,
        false,
    );
    let _ = fs::remove_dir_all(&dir);
    #[cfg(target_arch = "aarch64")]
    assert!(user_asm.contains("cmp x0, #14"));
    #[cfg(target_arch = "x86_64")]
    assert!(user_asm.contains("cmp QWORD PTR [rsp + 48], 14"));
}

/// Returns integer signal dispositions exactly as registered.
#[test]
fn test_pcntl_signal_get_handler_returns_integer_dispositions() {
    let out = compile_and_run(
        "<?php
        pcntl_signal(SIGUSR1, SIG_IGN);
        echo pcntl_signal_get_handler(SIGUSR1) === SIG_IGN ? 'ignore|' : 'bad|';
        pcntl_signal(SIGUSR1, SIG_DFL);
        echo pcntl_signal_get_handler(SIGUSR1) === SIG_DFL ? 'default' : 'bad';",
    );
    assert_eq!(out, "ignore|default");
}

/// Returns true when explicit dispatch runs before any signal handler registration.
#[test]
fn test_pcntl_signal_dispatch_without_registered_handlers() {
    let out = compile_and_run(
        "<?php echo pcntl_signal_dispatch() ? 'aot' : 'bad';
        echo eval('return pcntl_signal_dispatch() ? \"|eval\" : \"|bad\";');",
    );
    assert_eq!(out, "aot|eval");
}

/// Rejects booleans as signal dispositions in both AOT and eval execution.
#[test]
fn test_pcntl_signal_rejects_boolean_handler() {
    let out = compile_and_run(
        r#"<?php
        try { pcntl_signal(SIGALRM, true); }
        catch (TypeError $error) { echo $error->getMessage() . "|"; }
        echo eval('
            try { pcntl_signal(SIGALRM, false); }
            catch (TypeError $error) { return $error->getMessage(); }
            return "bad";
        ');"#,
    );
    assert_eq!(
        out,
        "pcntl_signal(): Argument #2 ($handler) must be of type callable|int, true given|\
pcntl_signal(): Argument #2 ($handler) must be of type callable|int, false given"
    );
}

/// Recognizes a returned closure handler as an instance of PHP's `Closure` class.
#[test]
fn test_pcntl_signal_get_handler_closure_is_instanceof_closure() {
    let out = compile_and_run(
        "<?php
        $direct = function(): void {};
        echo get_class($direct) . '|';
        pcntl_signal(SIGUSR1, function(int $signal): void {});
        $handler = pcntl_signal_get_handler(SIGUSR1);
        echo get_class($handler) . '|';
        echo $handler instanceof Closure ? 'closure' : 'bad';
        pcntl_signal(SIGUSR1, SIG_DFL);",
    );
    assert_eq!(out, "Closure|Closure|closure");
}

/// Raises a catchable PHP type error for a non-scalar restart-syscalls argument.
#[test]
fn test_pcntl_signal_rejects_array_restart_flag_without_backend_error() {
    let out = compile_and_run(
        "<?php
        try { pcntl_signal(SIGUSR1, SIG_IGN, []); }
        catch (TypeError $error) { echo get_class($error); }",
    );
    assert_eq!(out, "TypeError");
}

/// Coerces integer and heterogeneous argv values before replacing the forked process.
#[test]
fn test_pcntl_exec_coerces_scalar_argument_values() {
    let out = compile_and_run(
        r#"<?php
        $child = pcntl_fork();
        if ($child === 0) { pcntl_exec('/bin/echo', [1, 2]); exit(1); }
        pcntl_waitpid($child, $status);
        $child = pcntl_fork();
        if ($child === 0) { $port = 123; pcntl_exec('/bin/echo', ['port', $port]); exit(1); }
        pcntl_waitpid($child, $status);"#,
    );
    assert_eq!(out, "1 2\nport 123\n");
}

/// Coerces nullable indexed-array storage without dropping null entries from argv.
#[test]
fn test_pcntl_exec_coerces_nullable_argument_values() {
    let out = compile_and_run(
        r#"<?php
        function replace_with_nullable_echo(?int $value): void {
            pcntl_exec('/bin/echo', [$value, 3]);
        }
        $child = pcntl_fork();
        if ($child === 0) { replace_with_nullable_echo(null); exit(1); }
        pcntl_waitpid($child, $status);"#,
    );
    assert_eq!(out, " 3\n");
}

/// Coerces associative environment values before `execve` copies them.
#[test]
fn test_pcntl_exec_coerces_scalar_environment_values() {
    let out = compile_and_run(
        r#"<?php
        $child = pcntl_fork();
        if ($child === 0) { pcntl_exec('/usr/bin/env', [], ['K' => 123, 'EMPTY' => null]); exit(1); }
        pcntl_waitpid($child, $status);"#,
    );
    assert_eq!(out, "K=123\nEMPTY=\n");
}

/// Stringifies nested arrays in homogeneous and heterogeneous AOT exec arguments.
#[test]
fn test_pcntl_exec_stringifies_nested_array_arguments() {
    let out = compile_and_run_capture(
        r#"<?php
        $child = pcntl_fork();
        if ($child === 0) { pcntl_exec('/bin/echo', [[1], [2]]); exit(1); }
        pcntl_waitpid($child, $status);
        $child = pcntl_fork();
        if ($child === 0) { pcntl_exec('/bin/echo', [[], 'tail']); exit(1); }
        pcntl_waitpid($child, $status);"#,
    );
    assert_eq!(out.stdout, "Array Array\nArray tail\n");
    assert_eq!(
        out.stderr.matches("Warning: Array to string conversion").count(),
        3,
        "{}",
        out.stderr
    );
}

/// Stringifies nested arrays and resources in heterogeneous AOT exec environments.
#[test]
fn test_pcntl_exec_stringifies_array_and_resource_environment_values() {
    let out = compile_and_run_capture(
        r#"<?php
        $resource = fopen('php://memory', 'r');
        $child = pcntl_fork();
        if ($child === 0) {
            pcntl_exec('/usr/bin/env', [], ['NESTED' => [], 'RESOURCE' => $resource]);
            exit(1);
        }
        pcntl_waitpid($child, $status);"#,
    );
    assert!(out.stdout.contains("NESTED=Array\n"), "{}", out.stdout);
    assert!(
        out.stdout.contains("RESOURCE=Resource id #"),
        "{}",
        out.stdout
    );
    assert_eq!(
        out.stderr.matches("Warning: Array to string conversion").count(),
        1,
        "{}",
        out.stderr
    );
}

/// Uses `__toString()` for object values supplied through heterogeneous exec arrays.
#[test]
fn test_pcntl_exec_coerces_stringable_object_values() {
    let out = compile_and_run(
        r#"<?php
        class ExecStringable {
            public function __toString(): string { return 'object'; }
        }
        $child = pcntl_fork();
        if ($child === 0) {
            pcntl_exec('/bin/echo', ['value', new ExecStringable()]);
            exit(1);
        }
        pcntl_waitpid($child, $status);"#,
    );
    assert_eq!(out, "value object\n");
}

/// Throws PHP's catchable `Error`, with the concrete class name, for non-stringable exec values.
#[test]
fn test_pcntl_exec_non_stringable_object_throws_error() {
    let out = compile_and_run(
        r#"<?php
        class ExecPlainObject {}
        try { pcntl_exec('/bin/echo', [new ExecPlainObject()]); }
        catch (Error $error) { echo get_class($error) . ':' . $error->getMessage(); }
        catch (TypeError $error) { echo 'wrong'; }"#,
    );
    assert_eq!(
        out,
        "Error:Object of class ExecPlainObject could not be converted to string"
    );
}

/// Enforces PHP's current-thread-only Darwin priority selector rule in AOT and eval.
#[cfg(target_os = "macos")]
#[test]
fn test_pcntl_darwin_thread_priority_requires_zero_process_id() {
    let out = compile_and_run(
        r#"<?php
        try { pcntl_getpriority(5, PRIO_DARWIN_THREAD); }
        catch (ValueError $error) { echo 'aot-get|'; }
        try { pcntl_setpriority(0, 5, PRIO_DARWIN_THREAD); }
        catch (ValueError $error) {
            echo (str_contains($error->getMessage(), 'provided as second parameter') ? 'aot-set|' : 'bad|');
        }
        echo eval('
            try { pcntl_getpriority(5, PRIO_DARWIN_THREAD); }
            catch (ValueError $error) { echo "eval-get|"; }
            try { pcntl_setpriority(0, 5, PRIO_DARWIN_THREAD); }
            catch (ValueError $error) {
                return str_contains($error->getMessage(), "provided as second parameter") ? "eval-set" : "bad";
            }
            return "bad";
        ');"#,
    );
    assert_eq!(out, "aot-get|aot-set|eval-get|eval-set");
}

/// Automatically dispatches a pending signal after a normal EIR safe point.
#[test]
fn test_pcntl_async_signals_dispatch_at_safe_points() {
    let out = compile_and_run(
        "<?php
        function handle_async_alarm(int $signal, array $info): void {
            echo 'async:' . $signal . '|';
        }
        pcntl_signal(SIGALRM, 'handle_async_alarm');
        echo (pcntl_async_signals(true) ? 'old-on' : 'old-off') . '|';
        pcntl_alarm(1);
        sleep(2);
        echo (pcntl_async_signals(false) ? 'was-on' : 'bad') . '|';
        pcntl_signal(SIGALRM, SIG_DFL);",
    );
    assert_eq!(out, "old-off|async:14|was-on|");
}

/// Treats an explicit nullable async-signals argument as a query without changing state.
#[test]
fn test_pcntl_async_signals_explicit_null_queries_state() {
    let out = compile_and_run(
        "<?php
        pcntl_async_signals(true);
        echo (pcntl_async_signals(null) ? 'old-on' : 'bad') . '|';
        echo (pcntl_async_signals() ? 'still-on' : 'bad') . '|';
        echo (pcntl_async_signals(false) ? 'reset' : 'bad');",
    );
    assert_eq!(out, "old-on|still-on|reset");
}

/// Returns false on a Linux timed signal wait and preserves an existing info output.
#[cfg(target_os = "linux")]
#[test]
fn test_pcntl_timed_signal_wait_timeout_preserves_info() {
    let out = compile_and_run(
        "<?php
        $info = ['old' => 42];
        $received = pcntl_sigtimedwait([SIGUSR1], $info, 0, 1);
        echo (!$received ? 'timeout' : 'bad') . '|' . $info['old'];",
    );
    assert_eq!(out, "timeout|42");
}

/// Receives a blocked alarm synchronously through the AOT `sigwaitinfo` bridge.
#[cfg(target_os = "linux")]
#[test]
fn test_pcntl_sigwaitinfo_receives_blocked_alarm() {
    let out = compile_and_run(
        "<?php
        pcntl_sigprocmask(SIG_BLOCK, [SIGALRM], $old);
        pcntl_alarm(1);
        $received = pcntl_sigwaitinfo([SIGALRM], $info);
        pcntl_sigprocmask(SIG_SETMASK, $old);
        echo $received . '|' . $info['signo'];",
    );
    assert_eq!(out, "14|14");
}

/// Raises PHP's timed-wait `ValueError`s for invalid values held in dynamic variables.
#[cfg(target_os = "linux")]
#[test]
fn test_pcntl_timed_signal_wait_rejects_dynamic_invalid_timeouts() {
    let out = compile_and_run(
        "<?php
        $seconds = -1;
        $nanoseconds = 0;
        try { pcntl_sigtimedwait([SIGUSR1], $info, $seconds, $nanoseconds); }
        catch (ValueError $error) { echo 'seconds|'; }
        $seconds = 0;
        $nanoseconds = 1000000000;
        try { pcntl_sigtimedwait([SIGUSR1], $info, $seconds, $nanoseconds); }
        catch (ValueError $error) { echo 'nanoseconds|'; }
        $nanoseconds = 0;
        try { pcntl_sigtimedwait([SIGUSR1], $info, $seconds, $nanoseconds); }
        catch (ValueError $error) { echo 'zero'; }",
    );
    assert_eq!(out, "seconds|nanoseconds|zero");
}

/// Reads and reapplies the current Linux CPU affinity through indexed integer arrays.
#[cfg(target_os = "linux")]
#[test]
fn test_pcntl_linux_cpu_affinity_round_trip() {
    let out = compile_and_run(
        "<?php
        $cpu = pcntl_getcpu();
        $mask = pcntl_getcpuaffinity();
        echo ($cpu >= 0 ? 'cpu' : 'bad') . '|';
        echo (count($mask) > 0 ? 'mask' : 'bad') . '|';
        echo (pcntl_setcpuaffinity(cpu_ids: [$cpu]) ? 'set' : 'bad') . '|';
        echo (count($mask) > 0 ? 'mask' : 'bad');",
    );
    assert_eq!(out, "cpu|mask|set|mask");
}

/// Raises PHP's CPU-affinity `ValueError`s for dynamic invalid arrays and process ids.
#[cfg(target_os = "linux")]
#[test]
fn test_pcntl_linux_cpu_affinity_rejects_dynamic_invalid_values() {
    let out = compile_and_run(
        r#"<?php
        $empty = [];
        try { pcntl_setcpuaffinity(0, $empty); }
        catch (ValueError $error) { echo 'empty|'; }
        $invalid = 99999999;
        try { pcntl_setcpuaffinity(0, [$invalid]); }
        catch (ValueError $error) { echo 'cpu|'; }
        $valid = pcntl_getcpu();
        try { pcntl_setcpuaffinity(99999999, [$valid]); }
        catch (ValueError $error) { echo 'pid|'; }
        echo eval('
            $empty = [];
            try { pcntl_setcpuaffinity(0, $empty); }
            catch (ValueError $error) { return "eval"; }
            return "bad";
        ');"#,
    );
    assert_eq!(out, "empty|cpu|pid|eval");
}

/// Exercises Linux namespace validation without changing namespace state.
#[cfg(target_os = "linux")]
#[test]
fn test_pcntl_linux_namespace_operations_match_php_errors() {
    let out = compile_and_run(
        "<?php
        $unshared = pcntl_unshare(0);
        echo (is_bool($unshared) ? 'bool' : 'bad') . '|';
        try { pcntl_setns(99999999, CLONE_NEWNET); }
        catch (ValueError $error) { echo 'invalid|'; }
        try { pcntl_setns(0, CLONE_NEWNET); }
        catch (ValueError $error) { echo 'zero'; }",
    );
    assert_eq!(out, "bool|invalid|zero");
}

/// Raises PHP's invalid-flags `ValueError` for dynamic Linux unshare input.
#[cfg(target_os = "linux")]
#[test]
fn test_pcntl_linux_unshare_rejects_invalid_flags() {
    let out = compile_and_run(
        r#"<?php
        $flags = 1;
        try { pcntl_unshare($flags); }
        catch (ValueError $error) { echo 'aot|'; }
        echo eval('
            $flags = 1;
            try { pcntl_unshare($flags); }
            catch (ValueError $error) { return "eval"; }
            return "bad";
        ');"#,
    );
    assert_eq!(out, "aot|eval");
}

/// Keeps Linux-only PCNTL functions absent from the Darwin PHP-visible surface.
#[cfg(target_os = "macos")]
#[test]
fn test_pcntl_linux_functions_are_not_visible_on_macos() {
    let out = compile_and_run(
        "<?php echo function_exists('pcntl_getcpu') ? 'visible' : 'absent';",
    );
    assert_eq!(out, "absent");
}

/// Exercises Darwin QoS enum injection, getter identity, and explicit setter selection.
#[cfg(target_os = "macos")]
#[test]
fn test_pcntl_macos_qos_class_round_trip() {
    let out = compile_and_run(
        "<?php
        echo (enum_exists('Pcntl\\\\QosClass') ? 'enum' : 'missing') . '|';
        echo count(Pcntl\\QosClass::cases()) . '|';
        $before = pcntl_getqos_class();
        pcntl_setqos_class($before);
        echo (pcntl_getqos_class() === $before ? 'same' : 'changed') . '|';
        pcntl_setqos_class(Pcntl\\QosClass::Default);
        echo (pcntl_getqos_class() === Pcntl\\QosClass::Default ? 'default' : 'bad');",
    );
    assert_eq!(out, "enum|5|same|default");
}

/// Preserves php-src's catchable Error when Darwin rejects a QoS change after fork.
#[cfg(target_os = "macos")]
#[test]
fn test_pcntl_macos_qos_set_failure_throws_error() {
    let out = compile_and_run(
        "<?php
        $current = pcntl_getqos_class();
        $pid = pcntl_fork();
        if ($pid === 0) {
            try {
                pcntl_setqos_class($current);
                echo 'missing';
            } catch (Error $error) {
                echo $error->getMessage();
            }
            exit(0);
        }
        pcntl_waitpid($pid, $status);
        echo '|' . pcntl_wexitstatus($status);",
    );
    assert_eq!(out, "pcntl_setqos_class failed|0");
}

/// Keeps Darwin-only QoS functions and their enum absent from Linux PHP-visible metadata.
#[cfg(target_os = "linux")]
#[test]
fn test_pcntl_macos_qos_surface_is_not_visible_on_linux() {
    let out = compile_and_run(
        "<?php
        echo function_exists('pcntl_getqos_class') ? 'visible' : 'absent';
        echo '|';
        echo function_exists('pcntl_setqos_class') ? 'visible' : 'absent';
        echo '|';
        echo enum_exists('Pcntl\\\\QosClass') ? 'visible' : 'absent';",
    );
    assert_eq!(out, "absent|absent|absent");
}

/// Replaces a forked child with `/bin/sh`, preserving argv order and the explicit environment.
#[test]
fn test_pcntl_exec_replaces_child_with_arguments_and_environment() {
    let out = compile_and_run(
        r#"<?php
        $pid = pcntl_fork();
        if ($pid === 0) {
            pcntl_exec(
                '/bin/sh',
                ['-c', 'printf "%s" "$PCNTL_EXEC_ENV"'],
                ['PCNTL_EXEC_ENV' => 'ready'],
            );
            exit(99);
        }
        pcntl_waitpid($pid, $status);
        echo '|' . pcntl_wexitstatus($status);"#,
    );
    assert_eq!(out, "ready|0");
}

/// Keeps an omitted named environment absent so `pcntl_exec()` inherits the process environment.
#[test]
fn test_pcntl_exec_named_omitted_environment_is_inherited() {
    let out = compile_and_run(
        r#"<?php
        $pid = pcntl_fork();
        if ($pid === 0) {
            pcntl_exec(
                path: '/bin/sh',
                args: ['-c', 'if [ -n "$CARGO_MANIFEST_DIR" ]; then printf inherited; else printf cleared; fi'],
            );
            exit(99);
        }
        pcntl_waitpid($pid, $status);
        echo '|' . pcntl_wexitstatus($status);"#,
    );
    assert_eq!(out, "inherited|0");
}

/// Keeps an omitted static-spread environment absent instead of materializing its empty default.
#[test]
fn test_pcntl_exec_static_spread_omitted_environment_is_inherited() {
    let out = compile_and_run(
        r#"<?php
        $pid = pcntl_fork();
        if ($pid === 0) {
            pcntl_exec(...[
                'path' => '/bin/sh',
                'args' => ['-c', 'if [ -n "$CARGO_MANIFEST_DIR" ]; then printf inherited; else printf cleared; fi'],
            ]);
            exit(99);
        }
        pcntl_waitpid($pid, $status);
        echo '|' . pcntl_wexitstatus($status);"#,
    );
    assert_eq!(out, "inherited|0");
}

/// Returns false and preserves the bridge errno when process replacement fails.
#[test]
fn test_pcntl_exec_failure_returns_false_and_records_errno() {
    let out = compile_and_run_capture(
        "<?php
        $ok = pcntl_exec('/definitely/missing/elephc-pcntl');
        echo (!$ok ? 'false' : 'bad') . '|' . pcntl_get_last_error();",
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "false|2");
    assert!(
        out.stderr.contains(
            "Warning: pcntl_exec(): Error has occurred: (errno 2) No such file or directory"
        ),
        "unexpected stderr: {}",
        out.stderr
    );
}

/// Raises PHP's precise catchable `ValueError`s for every embedded-NUL exec input position.
#[test]
fn test_pcntl_exec_rejects_embedded_null_bytes() {
    let out = compile_and_run(
        r#"<?php
        try { pcntl_exec("/bin/echo\0bad"); }
        catch (ValueError $error) { echo $error->getMessage() . "|"; }
        try { pcntl_exec("/bin/echo", ["bad\0arg"]); }
        catch (ValueError $error) { echo $error->getMessage() . "|"; }
        try { pcntl_exec("/bin/echo", [], ["bad\0name" => "ok"]); }
        catch (ValueError $error) { echo $error->getMessage() . "|"; }
        try { pcntl_exec("/bin/echo", [], ["NAME" => "bad\0value"]); }
        catch (ValueError $error) { echo $error->getMessage() . "|"; }
        try { pcntl_exec("/bin/echo", [], ["bad\0name" => "bad\0value"]); }
        catch (ValueError $error) { echo $error->getMessage(); }"#,
    );
    assert_eq!(
        out,
        "pcntl_exec(): Argument #1 ($path) must not contain any null bytes|\
pcntl_exec(): Argument #2 ($args) individual argument must not contain null bytes|\
pcntl_exec(): Argument #3 ($env_vars) name for environment variable must not contain null bytes|\
pcntl_exec(): Argument #3 ($env_vars) value for environment variable must not contain null bytes|\
pcntl_exec(): Argument #3 ($env_vars) value for environment variable must not contain null bytes"
    );
}

/// Terminates fatally, even under suppression, when the OS rejects an uncatchable signal.
#[test]
fn test_pcntl_signal_uncatchable_signal_is_fatal() {
    for (signal, number) in [("SIGKILL", libc::SIGKILL), ("SIGSTOP", libc::SIGSTOP)] {
        let out = compile_and_run_capture(&format!(
            "<?php @pcntl_signal({signal}, function() {{}}); echo 'still alive';"
        ));
        assert!(!out.success, "{signal} registration unexpectedly succeeded");
        assert_eq!(out.stdout, "");
        assert!(
            out.stderr.contains(&format!(
                "Fatal error: Error installing signal handler for {number}"
            )),
            "unexpected stderr for {signal}: {}",
            out.stderr
        );
    }

    let eval = compile_and_run_capture(
        r#"<?php eval('pcntl_signal(SIGKILL, function() {}); echo "still alive";');
        echo "outer survived";"#,
    );
    assert!(!eval.success, "eval SIGKILL registration unexpectedly succeeded");
    assert_eq!(eval.stdout, "");
    assert!(
        eval.stderr.contains(&format!(
            "Fatal error: Error installing signal handler for {}",
            libc::SIGKILL
        )),
        "unexpected eval stderr: {}",
        eval.stderr
    );
}

/// Throws PHP's target-aware `ValueError` for an invalid handler-lookup signal.
#[test]
fn test_pcntl_signal_get_handler_rejects_invalid_signal() {
    let out = compile_and_run_capture("<?php pcntl_signal_get_handler(999999);");
    assert!(!out.success, "invalid signal unexpectedly succeeded");
    #[cfg(target_os = "macos")]
    assert!(
        out.stdout.contains(
            "Uncaught ValueError: pcntl_signal_get_handler(): Argument #1 ($signal) must be between 1 and 31"
        ),
        "unexpected stdout: {}",
        out.stdout
    );
    #[cfg(target_os = "linux")]
    assert!(
        out.stdout.contains(
            "Uncaught ValueError: pcntl_signal_get_handler(): Argument #1 ($signal) must be between 1 and 64"
        ),
        "unexpected stdout: {}",
        out.stdout
    );
}

/// Forks and reaps a real child through `pcntl_waitpid`, proving by-reference status writeback.
#[test]
fn test_pcntl_fork_waitpid_round_trip() {
    let out = compile_and_run(
        "<?php
        $pid = pcntl_fork();
        if ($pid === 0) { exit(23); }
        $status = 0;
        $waited = pcntl_waitpid($pid, $status);
        echo ($waited === $pid ? 'pid' : 'bad') . '|';
        echo pcntl_wifexited($status) . '|' . pcntl_wexitstatus($status);",
    );
    assert_eq!(out, "pid|1|23");
}

/// Forks and reaps a real child through the any-child `pcntl_wait` entry point.
#[test]
fn test_pcntl_fork_wait_round_trip() {
    let out = compile_and_run(
        "<?php
        $pid = pcntl_fork();
        if ($pid === 0) { exit(31); }
        $status = 0;
        $waited = pcntl_wait($status);
        echo ($waited === $pid ? 'pid' : 'bad') . '|';
        echo pcntl_wifexited($status) . '|' . pcntl_wexitstatus($status);",
    );
    assert_eq!(out, "pid|1|31");
}

/// Populates previously undefined status and usage outputs with PHP-compatible value types.
#[test]
fn test_pcntl_waitpid_populates_resource_usage_outputs() {
    let out = compile_and_run(
        "<?php
        $pid = pcntl_fork();
        if ($pid === 0) { exit(19); }
        $waited = pcntl_waitpid(
            process_id: $pid,
            status: $status,
            flags: 0,
            resource_usage: $usage,
        );
        echo ($waited === $pid ? 'pid' : 'bad') . '|';
        echo pcntl_wexitstatus($status) . '|';
        echo count($usage) . '|';
        echo is_int($usage['ru_utime.tv_sec']) ? 'int' : 'bad';",
    );
    assert_eq!(out, "pid|19|17|int");
}

/// Overwrites pre-existing incompatible wait output storage like PHP by-reference parameters.
#[test]
fn test_pcntl_waitpid_overwrites_incompatible_output_types() {
    let out = compile_and_run(
        "<?php
        $pid = pcntl_fork();
        if ($pid === 0) { exit(17); }
        $status = 'old';
        $usage = 'old';
        pcntl_waitpid(
            resource_usage: $usage,
            status: $status,
            process_id: $pid,
            flags: 0,
        );
        echo pcntl_wexitstatus($status) . '|';
        echo (is_array($usage) && count($usage) === 17 ? 'usage' : 'bad');",
    );
    assert_eq!(out, "17|usage");
}

/// Preserves status and replaces resource usage with an empty array when `waitpid()` fails.
#[test]
fn test_pcntl_waitpid_failure_preserves_status_and_empties_usage() {
    let out = compile_and_run(
        "<?php
        $status = 41;
        $usage = ['old' => 1];
        $pid = pcntl_waitpid(99999999, $status, WNOHANG, $usage);
        echo $pid . '|' . $status . '|' . count($usage) . '|';
        echo (array_key_exists('old', $usage) ? 'old' : 'noold') . '|';
        echo (array_key_exists('ru_maxrss', $usage) ? 'rss' : 'norss');",
    );
    assert_eq!(out, "-1|41|0|noold|norss");
}

/// Leaves wait outputs untouched by a live child's zero-result `WNOHANG` poll.
#[test]
fn test_pcntl_waitpid_wnohang_live_child_preserves_status_and_empties_usage() {
    let out = compile_and_run(
        "<?php
        $pid = pcntl_fork();
        if ($pid === 0) { sleep(1); exit(13); }
        $status = 41;
        $usage = ['old' => 1];
        $polled = pcntl_waitpid($pid, $status, WNOHANG, $usage);
        echo $polled . '|' . $status . '|' . count($usage) . '|';
        pcntl_waitpid($pid, $status);
        echo pcntl_wexitstatus($status);",
    );
    assert_eq!(out, "0|41|0|13");
}

/// Gives the forked child a private signal queue so its alarm cannot be dispatched by the parent.
#[test]
fn test_pcntl_fork_child_signal_queue_is_process_local() {
    let out = compile_and_run(
        "<?php
        $parentSeen = 0;
        pcntl_signal(SIGALRM, function(int $signal) use (&$parentSeen): void {
            $parentSeen = $signal;
        });
        $pid = pcntl_fork();
        if ($pid === 0) {
            pcntl_alarm(1);
            sleep(2);
            exit(0);
        }
        pcntl_waitpid($pid, $status);
        pcntl_signal_dispatch();
        pcntl_signal(SIGALRM, SIG_DFL);
        echo 'parent_seen=' . $parentSeen . '|child_exit=' . pcntl_wexitstatus($status);",
    );
    assert_eq!(out, "parent_seen=0|child_exit=0");
}

/// Restores dispatch state after a handler throws so a later signal can invoke it again.
#[test]
fn test_pcntl_handler_exception_restores_dispatch_state() {
    let out = compile_and_run(
        "<?php
        pcntl_signal(SIGALRM, function(): void { throw new RuntimeException('alarm'); });
        pcntl_alarm(1);
        sleep(2);
        try { pcntl_signal_dispatch(); } catch (RuntimeException $error) { echo 'caught|'; }
        pcntl_alarm(1);
        sleep(2);
        try { pcntl_signal_dispatch(); } catch (RuntimeException $error) { echo 'caught2'; }
        pcntl_signal(SIGALRM, SIG_DFL);",
    );
    assert_eq!(out, "caught|caught2");
}

/// Rejects a Fiber context switch while a signal handler owns dispatch state.
#[test]
fn test_pcntl_handler_cannot_switch_fibers() {
    let out = compile_and_run(
        "<?php
        pcntl_signal(SIGALRM, function(): void {
            $fiber = new Fiber(function(): void {});
            try { $fiber->start(); }
            catch (FiberError $error) { echo $error->getMessage(); }
        });
        pcntl_alarm(1);
        sleep(2);
        pcntl_signal_dispatch();
        pcntl_signal(SIGALRM, SIG_DFL);",
    );
    assert_eq!(out, "Cannot switch fibers in current execution context");
}

/// Defers signals raised by a handler until the next explicit snapshot dispatch.
#[test]
fn test_pcntl_dispatch_defers_nested_signal_arrivals() {
    let out = compile_and_run(
        "<?php
        extern \"System\" {
            function getpid(): int;
            function kill(int $pid, int $signal): int;
        }
        $seen = '';
        pcntl_signal(SIGUSR1, function() use (&$seen): void {
            $seen .= 'first';
            kill(getpid(), SIGUSR2);
        });
        pcntl_signal(SIGUSR2, function() use (&$seen): void { $seen .= ':second'; });
        kill(getpid(), SIGUSR1);
        pcntl_signal_dispatch();
        echo $seen . '|';
        pcntl_signal_dispatch();
        echo $seen;
        pcntl_signal(SIGUSR1, SIG_DFL);
        pcntl_signal(SIGUSR2, SIG_DFL);",
    );
    assert_eq!(out, "first|first:second");
}

/// Reaps a real child through `pcntl_waitid()` and exposes target-aware siginfo fields.
#[test]
fn test_pcntl_waitid_populates_signal_info() {
    let out = compile_and_run(
        "<?php
        $pid = pcntl_fork();
        if ($pid === 0) { exit(37); }
        $ok = pcntl_waitid(idtype: P_PID, id: $pid, info: $info, flags: WEXITED);
        echo ($ok ? 'ok' : 'bad') . '|';
        echo $info['status'] . '|';
        echo ($info['pid'] === $pid ? 'pid' : 'bad') . '|';
        echo count($info) . '|' . $info['signo'];",
    );
    #[cfg(target_os = "macos")]
    assert_eq!(out, "ok|37|pid|6|20");
    #[cfg(target_os = "linux")]
    assert_eq!(out, "ok|37|pid|8|17");
}

/// Exposes PHP 8.5 `pcntl_waitid()` resource usage through Linux's raw syscall.
#[cfg(target_os = "linux")]
#[test]
fn test_pcntl_waitid_populates_php_85_resource_usage() {
    let out = compile_and_run(
        "<?php
        $pid = pcntl_fork();
        if ($pid === 0) { exit(0); }
        $ok = pcntl_waitid(
            idtype: P_PID,
            id: $pid,
            flags: WEXITED,
            resource_usage: $usage,
        );
        echo ($ok ? 'ok' : 'bad') . '|';
        echo count($usage) . '|';
        echo is_int($usage['ru_utime.tv_sec']) ? 'int' : 'bad';",
    );
    assert_eq!(out, "ok|17|int");
}

/// Initializes PHP 8.5 waitid usage to an empty array when the syscall fails.
#[cfg(target_os = "linux")]
#[test]
fn test_pcntl_waitid_failure_empties_php_85_resource_usage() {
    let out = compile_and_run(
        "<?php
        $info = ['old' => 1];
        $usage = ['old' => 1];
        $ok = pcntl_waitid(P_PID, 99999999, $info, WEXITED | WNOHANG, $usage);
        echo ($ok ? 'bad' : 'false') . '|';
        echo $info['old'] . '|' . count($usage);",
    );
    assert_eq!(out, "false|1|0");
}

/// Preserves PHP's Linux waitid key order and floating-point clock fields.
#[cfg(target_os = "linux")]
#[test]
fn test_pcntl_waitid_uses_mixed_siginfo_values_and_php_key_order() {
    let out = compile_and_run(
        "<?php
        $pid = pcntl_fork();
        if ($pid === 0) { exit(0); }
        pcntl_waitid(P_PID, $pid, $info, WEXITED);
        echo implode(',', array_keys($info)) . '|';
        echo (is_float($info['utime']) && is_float($info['stime']) ? 'float' : 'bad');",
    );
    assert_eq!(out, "signo,errno,code,status,utime,stime,pid,uid|float");
}

/// Leaves an existing info output untouched when `pcntl_waitid()` fails.
#[test]
fn test_pcntl_waitid_failure_preserves_info_output() {
    let out = compile_and_run(
        "<?php
        $info = ['old' => 41];
        $ok = pcntl_waitid(P_PID, 99999999, $info, WEXITED | WNOHANG);
        echo ($ok ? 'bad' : 'false') . '|' . $info['old'];",
    );
    assert_eq!(out, "false|41");
}

/// Executes scalar PCNTL adapters through runtime eval, including failure errno propagation.
#[test]
fn test_pcntl_eval_scalar_status_and_exec_failure() {
    let out = compile_and_run_capture(
        r#"<?php
        echo eval('
            $status = 29 << 8;
            $failed = pcntl_exec("/definitely/missing/elephc-pcntl-eval");
            pcntl_async_signals(true);
            $nullable = pcntl_async_signals(null) && pcntl_async_signals();
            pcntl_async_signals(false);
            return (pcntl_wifexited($status) ? "exited" : "bad") . "|"
                . pcntl_wexitstatus($status) . "|"
                . (!$failed ? "false" : "bad") . "|"
                . pcntl_get_last_error() . "|"
                . ($nullable ? "nullable" : "bad");
        ');"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "exited|29|false|2|nullable");
    assert!(
        out.stderr.contains(
            "Warning: pcntl_exec(): Error has occurred: (errno 2) No such file or directory"
        ),
        "unexpected stderr: {}",
        out.stderr
    );
}

/// Forks and reaps a child inside runtime eval, proving by-reference status and usage writeback.
#[test]
fn test_pcntl_eval_fork_waitpid_writes_reference_outputs() {
    let out = compile_and_run(
        r#"<?php
        echo eval('
            $pid = pcntl_fork();
            if ($pid === 0) { exit(43); }
            $waited = pcntl_waitpid($pid, $status, 0, $usage);
            return ($waited === $pid ? "pid" : "bad") . "|"
                . pcntl_wexitstatus($status) . "|"
                . (is_int($usage["ru_utime.tv_sec"]) ? "usage" : "bad");
        ');"#,
    );
    assert_eq!(out, "pid|43|usage");
}

/// Preserves failed-wait outputs through the Magician by-reference adapter.
#[test]
fn test_pcntl_eval_waitpid_failure_preserves_outputs() {
    let out = compile_and_run(
        r#"<?php
        echo eval('
            $status = 41;
            $usage = ["old" => 1];
            $pid = pcntl_waitpid(99999999, $status, WNOHANG, $usage);
            return $pid . "|" . $status . "|" . count($usage) . "|"
                . (array_key_exists("old", $usage) ? "old" : "noold") . "|"
                . (array_key_exists("ru_maxrss", $usage) ? "rss" : "norss");
        ');"#,
    );
    assert_eq!(out, "-1|41|0|noold|norss");
}

/// Registers and dispatches an eval closure through the signal-safe Magician queue.
#[test]
fn test_pcntl_eval_signal_handler_dispatch() {
    let out = compile_and_run(
        r#"<?php
        echo eval('
            $seen = 0;
            $handler = function($signal, $info) use (&$seen) {
                $seen = $signal === $info["signo"] ? $signal : -1;
            };
            pcntl_signal(SIGALRM, $handler);
            pcntl_alarm(1);
            sleep(2);
            $ok = pcntl_signal_dispatch();
            pcntl_signal(SIGALRM, SIG_DFL);
            return ($ok ? "dispatch" : "bad") . "|" . $seen;
        ');"#,
    );
    assert_eq!(out, "dispatch|14");
}

/// Keeps an eval handler callable alive after the generated function frame that registered it exits.
#[test]
fn test_pcntl_eval_handler_survives_registering_function_frame() {
    let out = compile_and_run(
        r#"<?php
        extern "System" {
            function getpid(): int;
            function kill(int $pid, int $signal): int;
        }
        function register_eval_handler(): void {
            $word = "handled";
            eval('pcntl_signal(SIGUSR1, function() use ($word): void { echo $word; });');
        }
        register_eval_handler();
        kill(getpid(), SIGUSR1);
        echo eval('
            pcntl_signal_dispatch();
            pcntl_signal(SIGUSR1, SIG_DFL);
            return "|done";
        ');"#,
    );
    assert_eq!(out, "handled|done");
}

/// Returns a detached eval handler as a callable from a later eval context.
#[test]
fn test_pcntl_eval_get_detached_handler_remains_callable() {
    let out = compile_and_run(
        r#"<?php
        function register_gettable_eval_handler(): void {
            $word = "callable";
            eval('pcntl_signal(SIGUSR1, function() use ($word): void { echo $word; });');
        }
        register_gettable_eval_handler();
        echo eval('
            $handler = pcntl_signal_get_handler(SIGUSR1);
            pcntl_signal(SIGUSR1, SIG_DFL);
            echo is_callable($handler) ? "yes|" : "no|";
            $handler();
            echo "|";
            $closure = Closure::fromCallable($handler);
            $closure();
            return "|done";
        ');"#,
    );
    assert_eq!(out, "yes|callable|callable|done");
}

/// Refuses a detached eval handler before its context-local descriptor can escape into AOT.
#[test]
fn test_pcntl_detached_eval_handler_cannot_escape_to_aot() {
    let out = compile_and_run_capture(
        r#"<?php
        function register_exported_eval_handler(): void {
            $word = "escaped";
            eval('pcntl_signal(SIGUSR1, function() use ($word): void { echo $word; });');
        }
        function export_eval_handler_to_aot(): mixed {
            return eval('
                $handler = pcntl_signal_get_handler(SIGUSR1);
                pcntl_signal(SIGUSR1, SIG_DFL);
                return $handler;
            ');
        }
        register_exported_eval_handler();
        echo "before|";
        $handler = export_eval_handler_to_aot();
        echo "after";"#,
    );
    assert!(!out.success, "foreign eval handler unexpectedly escaped");
    assert_eq!(out.stdout, "before|");
    assert!(
        out.stderr
            .contains("Fatal error: PCNTL handler/closure cannot escape its eval context"),
        "unexpected stderr: {}",
        out.stderr
    );
}

/// Refuses a detached eval handler nested inside an array returned to AOT.
#[test]
fn test_pcntl_detached_eval_handler_cannot_escape_in_returned_array() {
    let out = compile_and_run_capture(
        r#"<?php
        function register_array_export_handler(): void {
            eval('pcntl_signal(SIGUSR1, function(): void { echo "escaped"; });');
        }
        register_array_export_handler();
        echo "before|";
        $wrapped = eval('
            $handler = pcntl_signal_get_handler(SIGUSR1);
            pcntl_signal(SIGUSR1, SIG_DFL);
            return ["handler" => $handler];
        ');
        echo "after";"#,
    );
    assert!(!out.success, "array-wrapped eval handler unexpectedly escaped");
    assert_eq!(out.stdout, "before|");
    assert!(
        out.stderr
            .contains("Fatal error: PCNTL handler/closure cannot escape its eval context"),
        "unexpected stderr: {}",
        out.stderr
    );
}

/// Refuses a detached eval handler nested inside an object returned to AOT.
#[test]
fn test_pcntl_detached_eval_handler_cannot_escape_in_returned_object() {
    let out = compile_and_run_capture(
        r#"<?php
        function register_object_export_handler(): void {
            eval('pcntl_signal(SIGUSR1, function(): void { echo "escaped"; });');
        }
        register_object_export_handler();
        echo "before|";
        $wrapped = eval('
            class HandlerBox { public mixed $handler; }
            $handler = pcntl_signal_get_handler(SIGUSR1);
            pcntl_signal(SIGUSR1, SIG_DFL);
            $box = new HandlerBox();
            $box->handler = $handler;
            return $box;
        ');
        $wrapped->handler();"#,
    );
    assert!(!out.success, "object-wrapped eval handler unexpectedly escaped");
    assert_eq!(out.stdout, "before|");
    assert!(
        out.stderr
            .contains("Fatal error: PCNTL handler/closure cannot escape its eval context"),
        "unexpected stderr: {}",
        out.stderr
    );
}

/// Refuses a detached eval handler assigned through `$GLOBALS` before AOT can invoke it.
#[test]
fn test_pcntl_detached_eval_handler_cannot_escape_through_globals() {
    let out = compile_and_run_capture(
        r#"<?php
        function register_global_export_handler(): void {
            eval('pcntl_signal(SIGUSR1, function(): void { echo "escaped"; });');
        }
        $handler = null;
        register_global_export_handler();
        echo "before|";
        eval('
            $handler = pcntl_signal_get_handler(SIGUSR1);
            pcntl_signal(SIGUSR1, SIG_DFL);
            $GLOBALS["handler"] = $handler;
        ');
        call_user_func($handler);"#,
    );
    assert!(!out.success, "global eval handler unexpectedly escaped");
    assert_eq!(out.stdout, "before|");
    assert!(
        out.stderr
            .contains("Fatal error: PCNTL handler/closure cannot escape its eval context"),
        "unexpected stderr: {}",
        out.stderr
    );
}

/// Retains the owner of a detached named-function handler after it is unregistered.
#[test]
fn test_pcntl_eval_get_detached_named_handler_remains_callable() {
    let out = compile_and_run(
        r#"<?php
        function register_named_eval_handler(): void {
            eval('
                function detached_named_handler(): void { echo "named"; }
                pcntl_signal(SIGUSR1, "detached_named_handler");
            ');
        }
        register_named_eval_handler();
        echo eval('
            $handler = pcntl_signal_get_handler(SIGUSR1);
            pcntl_signal(SIGUSR1, SIG_DFL);
            echo is_callable($handler) ? "yes|" : "no|";
            $handler();
            return "|done";
        ');"#,
    );
    assert_eq!(out, "yes|named|done");
}

/// Propagates a Throwable from a detached handler owner into the eval context doing dispatch.
#[test]
fn test_pcntl_detached_eval_handler_exception_reaches_dispatch_catch() {
    let out = compile_and_run(
        r#"<?php
        extern "System" {
            function getpid(): int;
            function kill(int $pid, int $signal): int;
        }
        function register_throwing_eval_handler(): void {
            eval('pcntl_signal(SIGUSR1, function(): void { throw new RuntimeException("detached"); });');
        }
        register_throwing_eval_handler();
        kill(getpid(), SIGUSR1);
        echo eval('
            try { pcntl_signal_dispatch(); }
            catch (RuntimeException $error) { echo $error->getMessage(); }
            pcntl_signal(SIGUSR1, SIG_DFL);
            return "|done";
        ');"#,
    );
    assert_eq!(out, "detached|done");
}

/// Keeps a detached owner pinned while its running handler replaces its own registration.
#[test]
fn test_pcntl_detached_eval_handler_can_replace_itself_safely() {
    let out = compile_and_run(
        r#"<?php
        extern "System" {
            function getpid(): int;
            function kill(int $pid, int $signal): int;
        }
        function register_self_replacing_eval_handler(): void {
            eval('
                function detached_handler_result(): string { return "safe"; }
                pcntl_signal(SIGUSR1, function(): void {
                    pcntl_signal(SIGUSR1, SIG_DFL);
                    echo detached_handler_result();
                });
            ');
        }
        register_self_replacing_eval_handler();
        kill(getpid(), SIGUSR1);
        echo eval('pcntl_signal_dispatch(); return "|done";');"#,
    );
    assert_eq!(out, "safe|done");
}

/// Keeps AOT and eval handler records queued for the backend that registered each callable.
#[test]
fn test_pcntl_aot_and_eval_dispatch_do_not_consume_each_others_records() {
    let out = compile_and_run(
        r#"<?php
        extern "System" {
            function getpid(): int;
            function kill(int $pid, int $signal): int;
        }
        $seen = "missing";
        pcntl_signal(SIGUSR1, function() use (&$seen): void { $seen = "aot"; });
        kill(getpid(), SIGUSR1);
        echo eval('return pcntl_signal_dispatch() ? "eval-empty|" : "eval-failed|";');
        pcntl_signal_dispatch();
        echo $seen . "|";
        pcntl_signal(SIGUSR1, SIG_DFL);

        echo eval('
            pcntl_signal(SIGALRM, function(): void { echo "eval-handler|"; });
            pcntl_alarm(1);
            return "registered|";
        ');
        sleep(2);
        pcntl_signal_dispatch();
        echo "aot-empty|";
        echo eval('
            pcntl_signal_dispatch();
            pcntl_signal(SIGALRM, SIG_DFL);
            return "done";
        ');"#,
    );
    assert_eq!(
        out,
        "eval-empty|aot|registered|aot-empty|eval-handler|done"
    );
}

/// Verifies a later eval registration owns delivery after replacing an AOT handler.
#[test]
fn test_pcntl_eval_later_signal_installer_owns_delivery_for_same_signal() {
    let out = compile_and_run(
        r#"<?php
        extern "System" {
            function getpid(): int;
            function kill(int $pid, int $signal): int;
        }
        $seen = "";
        pcntl_signal(SIGUSR1, function() use (&$seen): void { $seen = "aot"; });
        echo eval('
            pcntl_signal(SIGUSR1, function(): void { echo "eval|"; });
            return "installed|";
        ');
        kill(getpid(), SIGUSR1);
        pcntl_signal_dispatch();
        echo ($seen === "" ? "aot-empty|" : "aot-ran|");
        echo eval('
            pcntl_signal_dispatch();
            pcntl_signal(SIGUSR1, SIG_DFL);
            return "done";
        ');"#,
    );
    assert_eq!(out, "installed|aot-empty|eval|done");
}

/// Rejects Fiber switches from an eval-owned signal handler through the shared runtime guard.
#[test]
fn test_pcntl_eval_handler_cannot_switch_fibers() {
    let out = compile_and_run(
        r#"<?php
        function start_fiber_from_eval_handler(): void {
            $fiber = new Fiber(function(): void {});
            try { $fiber->start(); }
            catch (FiberError $error) { echo $error->getMessage(); }
        }
        echo eval('
            pcntl_signal(SIGALRM, function(): void {
                start_fiber_from_eval_handler();
            });
            pcntl_alarm(1);
            sleep(2);
            pcntl_signal_dispatch();
            pcntl_signal(SIGALRM, SIG_DFL);
        ');"#,
    );
    assert_eq!(out, "Cannot switch fibers in current execution context");
}

/// Rejects Fiber switches from an eval handler through static `call_user_func` Fiber entry points.
#[test]
fn test_pcntl_eval_handler_cannot_switch_fibers_through_call_user_func_static() {
    let out = compile_and_run(
        r#"<?php
        echo eval('
            pcntl_signal(SIGALRM, function(): void {
                try { call_user_func(["Fiber", "suspend"]); }
                catch (FiberError $error) { echo $error->getMessage(); }
            });
            pcntl_alarm(1);
            sleep(2);
            pcntl_signal_dispatch();
            pcntl_signal(SIGALRM, SIG_DFL);
        ');"#,
    );
    assert_eq!(out, "Cannot switch fibers in current execution context");
}

/// Preserves the inherited environment for a named eval `pcntl_exec()` call with omitted env.
#[test]
fn test_pcntl_eval_named_exec_omitted_environment_is_inherited() {
    let out = compile_and_run(
        r#"<?php
        echo eval('
            $pid = pcntl_fork();
            if ($pid === 0) {
                pcntl_exec(
                    path: "/bin/sh",
                    args: ["-c", "env | grep -q ^CARGO_MANIFEST_DIR= && printf inherited || printf cleared"],
                );
                exit(99);
            }
            pcntl_waitpid($pid, $status);
            return "|" . pcntl_wexitstatus($status);
        ');"#,
    );
    assert_eq!(out, "inherited|0");
}

/// Raises the same embedded-NUL `ValueError` through Magician's exec adapter.
#[test]
fn test_pcntl_eval_exec_rejects_embedded_null_bytes() {
    let out = compile_and_run(
        r#"<?php echo eval('
            try { pcntl_exec("/bin/echo", ["bad" . chr(0) . "arg"]); }
            catch (ValueError $error) { return $error->getMessage(); }
            return "bad";
        ');"#,
    );
    assert_eq!(
        out,
        "pcntl_exec(): Argument #2 ($args) individual argument must not contain null bytes"
    );
}

/// Uses eval-declared `__toString()` methods for both exec arguments and environment values.
#[test]
fn test_pcntl_eval_exec_coerces_stringable_object_values() {
    let out = compile_and_run(
        r#"<?php echo eval('
            class EvalExecStringable {
                public function __toString(): string { return "object"; }
            }
            $pid = pcntl_fork();
            if ($pid === 0) {
                pcntl_exec("/bin/echo", ["value", new EvalExecStringable()]);
                exit(99);
            }
            pcntl_waitpid($pid, $status);
            $pid = pcntl_fork();
            if ($pid === 0) {
                pcntl_exec("/usr/bin/env", [], ["VALUE" => new EvalExecStringable()]);
                exit(99);
            }
            pcntl_waitpid($pid, $status);
            return "status=" . pcntl_wexitstatus($status);
        ');"#,
    );
    assert_eq!(out, "value object\nVALUE=object\nstatus=0");
}

/// Applies PHP array and resource stringification to eval exec values.
#[test]
fn test_pcntl_eval_exec_stringifies_array_and_resource_values() {
    let out = compile_and_run_capture(
        r#"<?php echo eval('
            $resource = fopen("php://memory", "r");
            $pid = pcntl_fork();
            if ($pid === 0) {
                pcntl_exec("/bin/echo", [[], $resource]);
                exit(99);
            }
            pcntl_waitpid($pid, $status);
            return "status=" . pcntl_wexitstatus($status);
        ');"#,
    );
    assert!(
        out.stdout.starts_with("Array Resource id #"),
        "{}",
        out.stdout
    );
    assert!(out.stdout.ends_with("\nstatus=0"), "{}", out.stdout);
    assert_eq!(
        out.stderr.matches("Warning: Array to string conversion").count(),
        1,
        "{}",
        out.stderr
    );
}

/// Throws PHP's catchable `Error` when an eval exec value has no `__toString()` method.
#[test]
fn test_pcntl_eval_exec_non_stringable_object_throws_error() {
    let out = compile_and_run(
        r#"<?php echo eval('
            class EvalExecPlainObject {}
            try { pcntl_exec("/bin/echo", [new EvalExecPlainObject()]); }
            catch (Error $error) { return get_class($error) . ":" . $error->getMessage(); }
            catch (TypeError $error) { return "wrong"; }
            return "bad";
        ');"#,
    );
    assert_eq!(
        out,
        "Error:Object of class EvalExecPlainObject could not be converted to string"
    );
}

/// Exposes PHP 8.5 waitid resource usage through the Magician by-reference adapter.
#[cfg(target_os = "linux")]
#[test]
fn test_pcntl_eval_waitid_populates_php_85_resource_usage() {
    let out = compile_and_run(
        r#"<?php echo eval('
            $pid = pcntl_fork();
            if ($pid === 0) { exit(0); }
            $ok = pcntl_waitid(P_PID, $pid, $info, WEXITED, $usage);
            return ($ok ? "ok" : "bad") . "|" . count($usage);
        ');"#,
    );
    assert_eq!(out, "ok|17");
}

/// Restores Magician dispatch state after a handler exception and invokes the next alarm.
#[test]
fn test_pcntl_eval_handler_exception_restores_dispatch_state() {
    let out = compile_and_run(
        r#"<?php
        echo eval('
            $handler = function(): void { throw new RuntimeException("alarm"); };
            pcntl_signal(SIGALRM, $handler);
            pcntl_alarm(1);
            sleep(2);
            try { pcntl_signal_dispatch(); }
            catch (RuntimeException $error) { echo "caught|"; }
            pcntl_alarm(1);
            sleep(2);
            try { pcntl_signal_dispatch(); }
            catch (RuntimeException $error) { echo "caught2"; }
            pcntl_signal(SIGALRM, SIG_DFL);
        ');"#,
    );
    assert_eq!(out, "caught|caught2");
}

/// Exercises Linux-only eval adapters and confirms Darwin QoS metadata remains absent.
#[cfg(target_os = "linux")]
#[test]
fn test_pcntl_eval_linux_target_surface_and_affinity() {
    let out = compile_and_run(
        r#"<?php
        echo eval('
            $cpu = pcntl_getcpu();
            $mask = pcntl_getcpuaffinity();
            return (function_exists("pcntl_getcpu") ? "visible" : "missing") . "|"
                . (function_exists("pcntl_getqos_class") ? "qos" : "no-qos") . "|"
                . ($cpu >= 0 && count($mask) > 0 ? "affinity" : "bad");
        ');"#,
    );
    assert_eq!(out, "visible|no-qos|affinity");
}

/// Preserves explicit PID zero and raises PHP's `ValueError` through eval `pcntl_setns()`.
#[cfg(target_os = "linux")]
#[test]
fn test_pcntl_eval_setns_rejects_explicit_zero_pid() {
    let out = compile_and_run(
        r#"<?php
        echo eval('
            try { pcntl_setns(0, CLONE_NEWNET); }
            catch (ValueError $error) { return $error->getMessage(); }
            return "bad";
        ');"#,
    );
    assert_eq!(
        out,
        "pcntl_setns(): Argument #1 ($process_id) is not a valid process (0)"
    );
}

/// Exercises explicit Darwin QoS selection through eval and keeps Linux metadata absent.
#[cfg(target_os = "macos")]
#[test]
fn test_pcntl_eval_macos_qos_explicit_default_and_target_surface() {
    let out = compile_and_run(
        r#"<?php
        echo eval('
            $before = pcntl_getqos_class();
            pcntl_setqos_class($before);
            pcntl_setqos_class(Pcntl\\QosClass::Default);
            return (function_exists("pcntl_getqos_class") ? "visible" : "missing") . "|"
                . (function_exists("pcntl_getcpu") ? "linux" : "no-linux") . "|"
                . (pcntl_getqos_class() === Pcntl\\QosClass::Default ? "default" : "bad");
        ');"#,
    );
    assert_eq!(out, "visible|no-linux|default");
}
