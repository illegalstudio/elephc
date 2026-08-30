//! Purpose:
//! Exercises the PCNTL bridge ABI against real process, wait, signal, and platform operations.
//!
//! Called from:
//! - `cargo test -p elephc-pcntl` through Rust's test harness.
//!
//! Key details:
//! - Process-global signal and QoS tests serialize through one mutex.
//! - Linux-only cases validate affinity and synchronous signal APIs.

use super::*;

/// Stages argv and envp entries, then exposes the OS error from a missing executable.
#[test]
fn exec_builder_reports_failed_replacement() {
    let path = b"/definitely/missing/elephc-pcntl";
    let argument = b"argument";
    let key = b"PCNTL_TEST";
    let value = b"present";
    let builder = unsafe { elephc_pcntl_exec_new(path.as_ptr(), path.len(), 1) };
    assert!(!builder.is_null());
    assert_eq!(
        unsafe { elephc_pcntl_exec_add_arg(builder, argument.as_ptr(), argument.len()) },
        1
    );
    assert_eq!(
        unsafe {
            elephc_pcntl_exec_add_env(
                builder,
                key.as_ptr() as u64,
                key.len() as i64,
                value.as_ptr(),
                value.len(),
            )
        },
        1
    );
    assert_eq!(unsafe { elephc_pcntl_exec_run(builder) }, 0);
    assert_eq!(elephc_pcntl_get_last_error(), libc::ENOENT);
}

/// Formats the latest exec failure with PHP's warning prefix, errno, and native message.
#[test]
fn exec_failure_warning_contains_errno_and_native_message() {
    LAST_ERROR.store(libc::ENOENT, Ordering::Relaxed);
    let warning = pcntl_last_error_warning(PCNTL_WARNING_EXEC);
    assert!(warning.starts_with("Warning: pcntl_exec(): Error has occurred: (errno "));
    assert!(warning.contains(&libc::ENOENT.to_string()));
    assert!(warning.ends_with('\n'));
}

/// Reads the current Linux CPU and affinity mask through the stable bridge ABI.
#[cfg(target_os = "linux")]
#[test]
fn linux_cpu_queries_return_consistent_identifiers() {
    let cpu = elephc_pcntl_getcpu();
    assert!(cpu >= 0, "sched_getcpu failed with errno {}", elephc_pcntl_get_last_error());
    let mut cpus = [0i64; libc::CPU_SETSIZE as usize];
    let count = unsafe { elephc_pcntl_getcpuaffinity(0, cpus.as_mut_ptr(), cpus.len()) };
    assert!(count > 0, "get affinity failed with errno {}", elephc_pcntl_get_last_error());
    assert!(cpus[..count as usize].contains(&cpu));
}

/// Reapplies the current Linux affinity mask without changing process placement policy.
#[cfg(target_os = "linux")]
#[test]
fn linux_cpu_affinity_round_trips_current_mask() {
    let mut cpus = [0i64; libc::CPU_SETSIZE as usize];
    let count = unsafe { elephc_pcntl_getcpuaffinity(0, cpus.as_mut_ptr(), cpus.len()) };
    assert!(count > 0, "get affinity failed with errno {}", elephc_pcntl_get_last_error());
    let success = unsafe { elephc_pcntl_setcpuaffinity(0, cpus.as_ptr(), count as usize) };
    assert_eq!(success, 1, "set affinity failed with errno {}", elephc_pcntl_get_last_error());
}

/// Rejects an empty Linux affinity mask and exposes the expected `EINVAL` status.
#[cfg(target_os = "linux")]
#[test]
fn linux_empty_cpu_affinity_is_rejected() {
    let success = unsafe { elephc_pcntl_setcpuaffinity(0, std::ptr::null(), 0) };
    assert_eq!(success, -1);
    assert_eq!(elephc_pcntl_get_last_error(), libc::EINVAL);
    assert_eq!(
        pcntl_cpu_affinity_value_error(success, 0),
        "pcntl_setcpuaffinity(): Argument #2 ($cpu_ids) must not be empty"
    );
}

/// Classifies an out-of-range Linux CPU identifier and retains it for PHP's message.
#[cfg(target_os = "linux")]
#[test]
fn linux_out_of_range_cpu_affinity_is_rejected() {
    let invalid = i64::MAX;
    let success = unsafe { elephc_pcntl_setcpuaffinity(0, &invalid, 1) };
    assert_eq!(success, -2);
    let message = pcntl_cpu_affinity_value_error(success, 0);
    assert!(message.contains("cpu id must be between 0 and"));
    assert!(message.ends_with(&format!("({invalid})")));
}

/// Rejects explicit PID zero before `pidfd_open` can reinterpret it through the host policy.
#[cfg(target_os = "linux")]
#[test]
fn linux_setns_rejects_explicit_zero_process_id() {
    assert_eq!(elephc_pcntl_setns(0, libc::CLONE_NEWNET), -1);
    assert_eq!(elephc_pcntl_get_last_error(), libc::EINVAL);
}

/// Rejects bits outside PHP's exported namespace-clone mask before entering `unshare`.
#[cfg(target_os = "linux")]
#[test]
fn linux_unshare_rejects_unknown_flag_bits() {
    assert_eq!(elephc_pcntl_unshare(1), -1);
    assert_eq!(elephc_pcntl_get_last_error(), libc::EINVAL);
}

/// Forks a real child, reaps it, and verifies target-native wait status decoding.
#[test]
fn fork_waitpid_and_status_decoding_round_trip() {
    let _guard = PROCESS_TEST_LOCK.lock().expect("process test lock poisoned");
    let pid = elephc_pcntl_fork();
    assert!(pid >= 0, "fork failed with errno {}", elephc_pcntl_get_last_error());
    if pid == 0 {
        unsafe { libc::_exit(23) };
    }

    let mut status = 0;
    let waited = unsafe { elephc_pcntl_waitpid(pid, &mut status, 0) };
    assert_eq!(waited, pid);
    assert_eq!(elephc_pcntl_wifexited(status), 1);
    assert_eq!(elephc_pcntl_wifsignaled(status), 0);
    assert_eq!(elephc_pcntl_wifstopped(status), 0);
    assert_eq!(elephc_pcntl_wexitstatus(status), 23);
}

/// Reaps a real child through the any-child wait entry point.
#[test]
fn fork_wait_and_status_decoding_round_trip() {
    let _guard = PROCESS_TEST_LOCK.lock().expect("process test lock poisoned");
    let pid = elephc_pcntl_fork();
    assert!(pid >= 0, "fork failed with errno {}", elephc_pcntl_get_last_error());
    if pid == 0 {
        unsafe { libc::_exit(31) };
    }

    let mut status = 0;
    let waited = unsafe { elephc_pcntl_wait(&mut status, 0) };
    assert_eq!(waited, pid);
    assert_eq!(elephc_pcntl_wifexited(status), 1);
    assert_eq!(elephc_pcntl_wexitstatus(status), 31);
}

/// Reaps a real child through `wait4` and exposes its usage in the stable bridge layout.
#[test]
fn fork_wait4_populates_stable_resource_usage() {
    let _guard = PROCESS_TEST_LOCK.lock().expect("process test lock poisoned");
    let pid = elephc_pcntl_fork();
    assert!(pid >= 0, "fork failed with errno {}", elephc_pcntl_get_last_error());
    if pid == 0 {
        unsafe { libc::_exit(19) };
    }

    let mut status = 0;
    let mut usage = ElephcPcntlRUsage::default();
    let waited = unsafe { elephc_pcntl_wait4(pid, &mut status, 0, &mut usage) };
    assert_eq!(waited, pid);
    assert_eq!(elephc_pcntl_wexitstatus(status), 19);
    assert!(usage.ru_utime_tv_sec >= 0);
    assert!(usage.ru_stime_tv_sec >= 0);
}

/// Reaps a real child through `waitid` and copies its portable PHP information fields.
#[test]
fn fork_waitid_populates_stable_siginfo() {
    let _guard = PROCESS_TEST_LOCK.lock().expect("process test lock poisoned");
    let pid = elephc_pcntl_fork();
    assert!(pid >= 0, "fork failed with errno {}", elephc_pcntl_get_last_error());
    if pid == 0 {
        unsafe { libc::_exit(29) };
    }

    let mut info = ElephcPcntlSigInfo::default();
    let success = unsafe {
        elephc_pcntl_waitid(
            libc::P_PID as libc::c_int,
            pid,
            &mut info,
            libc::WEXITED,
            std::ptr::null_mut(),
        )
    };
    assert_eq!(success, 1);
    assert_eq!(info.pid, pid);
    assert_eq!(info.status, 29);
    assert_ne!(info.present & SIGINFO_STATUS, 0);
}

/// Reaps a real child through Linux raw `waitid` and copies PHP 8.5 resource usage.
#[cfg(target_os = "linux")]
#[test]
fn fork_waitid_populates_stable_resource_usage() {
    let _guard = PROCESS_TEST_LOCK.lock().expect("process test lock poisoned");
    let pid = elephc_pcntl_fork();
    assert!(pid >= 0, "fork failed with errno {}", elephc_pcntl_get_last_error());
    if pid == 0 {
        unsafe { libc::_exit(0) };
    }

    let mut info = ElephcPcntlSigInfo::default();
    let mut usage = ElephcPcntlRUsage::default();
    let success = unsafe {
        elephc_pcntl_waitid(
            libc::P_PID as libc::c_int,
            pid,
            &mut info,
            libc::WEXITED,
            &mut usage,
        )
    };
    assert_eq!(success, 1);
    assert_eq!(info.pid, pid);
    assert!(usage.ru_utime_tv_sec >= 0);
    assert!(usage.ru_stime_tv_sec >= 0);
}

/// Blocks one signal through the stable array ABI, returns the prior mask, and restores it.
#[test]
fn signal_mask_round_trips_old_members() {
    let _guard = PROCESS_TEST_LOCK.lock().expect("process test lock poisoned");
    let mut original = unsafe { std::mem::zeroed::<libc::sigset_t>() };
    let mut selected = unsafe { std::mem::zeroed::<libc::sigset_t>() };
    unsafe {
        libc::sigemptyset(&mut selected);
        libc::sigaddset(&mut selected, libc::SIGUSR1);
        assert_eq!(libc::sigprocmask(libc::SIG_UNBLOCK, &selected, &mut original), 0);
    }
    let signals = [i64::from(libc::SIGUSR1)];
    let mut old = [0i64; 128];
    let count = unsafe {
        elephc_pcntl_sigprocmask(
            libc::SIG_BLOCK,
            signals.as_ptr(),
            signals.len(),
            old.as_mut_ptr(),
            old.len(),
        )
    };
    assert!(count >= 0);
    assert!(!old[..count as usize].contains(&i64::from(libc::SIGUSR1)));
    unsafe {
        assert_eq!(libc::sigprocmask(libc::SIG_SETMASK, &original, std::ptr::null_mut()), 0);
    }
}

/// Classifies empty, out-of-range, and valid widened signal arrays without mutating the mask.
#[test]
fn signal_set_validation_matches_php_argument_rules() {
    assert_eq!(
        unsafe { elephc_pcntl_validate_signal_set(std::ptr::null(), 0, 0) },
        -1
    );
    assert_eq!(
        unsafe { elephc_pcntl_validate_signal_set(std::ptr::null(), 0, 1) },
        1
    );
    let invalid = 0i64;
    assert_eq!(
        unsafe { elephc_pcntl_validate_signal_set(&invalid, 1, 0) },
        -2
    );
    let valid = i64::from(libc::SIGUSR1);
    assert_eq!(
        unsafe { elephc_pcntl_validate_signal_set(&valid, 1, 0) },
        1
    );
}

/// Queues one asynchronously delivered signal and copies its stable siginfo outside the handler.
#[test]
fn signal_handler_queues_a_stable_record() {
    let _guard = PROCESS_TEST_LOCK.lock().expect("process test lock poisoned");
    let mut original = unsafe { std::mem::zeroed::<libc::sigset_t>() };
    let mut selected = unsafe { std::mem::zeroed::<libc::sigset_t>() };
    unsafe {
        libc::sigemptyset(&mut selected);
        libc::sigaddset(&mut selected, libc::SIGUSR1);
        assert_eq!(libc::sigprocmask(libc::SIG_UNBLOCK, &selected, &mut original), 0);
    }
    let mut discarded = ElephcPcntlSigInfo::default();
    while unsafe { elephc_pcntl_signal_next(&mut discarded, PCNTL_SIGNAL_OWNER_AOT) } == 1 {}
    assert_eq!(
        elephc_pcntl_signal(libc::SIGUSR1, 2, 1, PCNTL_SIGNAL_OWNER_AOT),
        1
    );
    let raise_result = unsafe { libc::raise(libc::SIGUSR1) };
    let mut info = ElephcPcntlSigInfo::default();
    let queued = unsafe { elephc_pcntl_signal_next(&mut info, PCNTL_SIGNAL_OWNER_AOT) };
    let restore_handler =
        elephc_pcntl_signal(libc::SIGUSR1, 0, 1, PCNTL_SIGNAL_OWNER_AOT);
    unsafe {
        assert_eq!(libc::sigprocmask(libc::SIG_SETMASK, &original, std::ptr::null_mut()), 0);
    }
    assert_eq!(raise_result, 0);
    assert_eq!(queued, 1);
    assert_eq!(restore_handler, 1);
    assert_eq!(info.signo, i64::from(libc::SIGUSR1));
    assert_ne!(info.present & SIGINFO_SIGNO, 0);
}

/// Pins the PHP/POSIX limit: standard signals coalesce before the queued handler runs.
#[test]
fn standard_non_realtime_signals_coalesce_before_php_style_handler_runs() {
    let _guard = PROCESS_TEST_LOCK.lock().expect("process test lock poisoned");
    let mut original = unsafe { std::mem::zeroed::<libc::sigset_t>() };
    let mut selected = unsafe { std::mem::zeroed::<libc::sigset_t>() };
    unsafe {
        libc::sigemptyset(&mut selected);
        libc::sigaddset(&mut selected, libc::SIGUSR1);
        assert_eq!(libc::sigprocmask(libc::SIG_BLOCK, &selected, &mut original), 0);
    }
    let mut info = ElephcPcntlSigInfo::default();
    while unsafe { elephc_pcntl_signal_next(&mut info, PCNTL_SIGNAL_OWNER_AOT) } == 1 {}
    assert_eq!(
        elephc_pcntl_signal(libc::SIGUSR1, 2, 1, PCNTL_SIGNAL_OWNER_AOT),
        1
    );
    assert_eq!(unsafe { libc::raise(libc::SIGUSR1) }, 0);
    assert_eq!(unsafe { libc::raise(libc::SIGUSR1) }, 0);
    unsafe {
        assert_eq!(
            libc::sigprocmask(libc::SIG_UNBLOCK, &selected, std::ptr::null_mut()),
            0
        );
    }

    let mut delivered = 0;
    while unsafe { elephc_pcntl_signal_next(&mut info, PCNTL_SIGNAL_OWNER_AOT) } == 1 {
        delivered += 1;
    }
    assert_eq!(
        elephc_pcntl_signal(libc::SIGUSR1, 0, 1, PCNTL_SIGNAL_OWNER_AOT),
        1
    );
    unsafe {
        assert_eq!(
            libc::sigprocmask(libc::SIG_SETMASK, &original, std::ptr::null_mut()),
            0
        );
    }
    assert_eq!(delivered, 1);
}

/// Keeps AOT and eval records isolated even when the other backend drains first.
#[test]
fn signal_queues_are_routed_to_the_registering_backend() {
    let _guard = PROCESS_TEST_LOCK.lock().expect("process test lock poisoned");
    let mut discarded = ElephcPcntlSigInfo::default();
    for owner in [PCNTL_SIGNAL_OWNER_AOT, PCNTL_SIGNAL_OWNER_EVAL] {
        while unsafe { elephc_pcntl_signal_next(&mut discarded, owner) } == 1 {}
    }
    assert_eq!(
        elephc_pcntl_signal(libc::SIGUSR1, 2, 1, PCNTL_SIGNAL_OWNER_AOT),
        1
    );
    assert_eq!(unsafe { libc::raise(libc::SIGUSR1) }, 0);
    assert_eq!(
        elephc_pcntl_signal(libc::SIGUSR1, 2, 1, PCNTL_SIGNAL_OWNER_EVAL),
        1
    );
    assert_eq!(unsafe { libc::raise(libc::SIGUSR1) }, 0);

    let mut aot = ElephcPcntlSigInfo::default();
    let mut eval = ElephcPcntlSigInfo::default();
    assert_eq!(
        unsafe { elephc_pcntl_signal_next(&mut eval, PCNTL_SIGNAL_OWNER_EVAL) },
        1
    );
    assert_eq!(eval.signo, i64::from(libc::SIGUSR1));
    assert_eq!(
        unsafe { elephc_pcntl_signal_next(&mut eval, PCNTL_SIGNAL_OWNER_EVAL) },
        0
    );
    assert_eq!(
        unsafe { elephc_pcntl_signal_next(&mut aot, PCNTL_SIGNAL_OWNER_AOT) },
        1
    );
    assert_eq!(aot.signo, i64::from(libc::SIGUSR1));

    assert_eq!(
        elephc_pcntl_signal(libc::SIGUSR1, 0, 1, PCNTL_SIGNAL_OWNER_EVAL),
        1
    );
}

/// Routes a new delivery only to the backend whose later registration owns `sigaction`.
#[test]
fn later_signal_installer_owns_delivery_for_same_signal() {
    let _guard = PROCESS_TEST_LOCK.lock().expect("process test lock poisoned");
    let mut discarded = ElephcPcntlSigInfo::default();
    for owner in [PCNTL_SIGNAL_OWNER_AOT, PCNTL_SIGNAL_OWNER_EVAL] {
        while unsafe { elephc_pcntl_signal_next(&mut discarded, owner) } == 1 {}
    }
    assert_eq!(
        elephc_pcntl_signal(libc::SIGUSR1, 2, 1, PCNTL_SIGNAL_OWNER_AOT),
        1
    );
    assert_eq!(
        elephc_pcntl_signal(libc::SIGUSR1, 2, 1, PCNTL_SIGNAL_OWNER_EVAL),
        1
    );
    assert_eq!(unsafe { libc::raise(libc::SIGUSR1) }, 0);

    let mut info = ElephcPcntlSigInfo::default();
    assert_eq!(
        unsafe { elephc_pcntl_signal_next(&mut info, PCNTL_SIGNAL_OWNER_EVAL) },
        1
    );
    assert_eq!(info.signo, i64::from(libc::SIGUSR1));
    assert_eq!(
        unsafe { elephc_pcntl_signal_next(&mut info, PCNTL_SIGNAL_OWNER_AOT) },
        0
    );
    assert_eq!(
        elephc_pcntl_signal(libc::SIGUSR1, 0, 1, PCNTL_SIGNAL_OWNER_EVAL),
        1
    );
}

/// Replays every retained record after a saturated pipe falls back to the spill queue.
#[test]
fn signal_queue_overflow_preserves_delivery_count() {
    let _guard = PROCESS_TEST_LOCK.lock().expect("process test lock poisoned");
    const DELIVERY_COUNT: usize = 4096;
    let mut original = unsafe { std::mem::zeroed::<libc::sigset_t>() };
    let mut selected = unsafe { std::mem::zeroed::<libc::sigset_t>() };
    unsafe {
        libc::sigemptyset(&mut selected);
        libc::sigaddset(&mut selected, libc::SIGUSR1);
        assert_eq!(libc::sigprocmask(libc::SIG_UNBLOCK, &selected, &mut original), 0);
    }
    let mut discarded = ElephcPcntlSigInfo::default();
    while unsafe { elephc_pcntl_signal_next(&mut discarded, PCNTL_SIGNAL_OWNER_AOT) } == 1 {}
    assert_eq!(
        elephc_pcntl_signal(libc::SIGUSR1, 2, 1, PCNTL_SIGNAL_OWNER_AOT),
        1
    );
    let mut raises_succeeded = true;
    for _ in 0..DELIVERY_COUNT {
        raises_succeeded &= unsafe { libc::raise(libc::SIGUSR1) } == 0;
    }

    let mut delivered = 0;
    let mut records_match = true;
    loop {
        let mut info = ElephcPcntlSigInfo::default();
        let status = unsafe { elephc_pcntl_signal_next(&mut info, PCNTL_SIGNAL_OWNER_AOT) };
        if status != 1 {
            records_match &= status == 0;
            break;
        }
        delivered += 1;
        records_match &= info.signo == i64::from(libc::SIGUSR1);
    }
    let restore_handler =
        elephc_pcntl_signal(libc::SIGUSR1, 0, 1, PCNTL_SIGNAL_OWNER_AOT);
    unsafe {
        assert_eq!(libc::sigprocmask(libc::SIG_SETMASK, &original, std::ptr::null_mut()), 0);
    }

    assert!(raises_succeeded);
    assert_eq!(restore_handler, 1);
    assert!(records_match);
    assert_eq!(delivered, DELIVERY_COUNT);
}

/// Verifies overflow retains each delivery's distinct siginfo snapshot in FIFO order.
#[test]
fn signal_queue_overflow_preserves_distinct_siginfo_snapshots() {
    let _guard = PROCESS_TEST_LOCK.lock().expect("process test lock poisoned");
    let mut info = ElephcPcntlSigInfo::default();
    while unsafe { elephc_pcntl_signal_next(&mut info, PCNTL_SIGNAL_OWNER_AOT) } == 1 {}

    let first = ElephcPcntlSigInfo {
        signo: i64::from(libc::SIGUSR1),
        status: 11,
        present: SIGINFO_SIGNO | SIGINFO_STATUS,
        ..ElephcPcntlSigInfo::default()
    };
    let second = ElephcPcntlSigInfo {
        status: 22,
        ..first
    };
    assert!(queue_signal_overflow_for_test(
        PCNTL_SIGNAL_OWNER_AOT,
        &first
    ));
    assert!(queue_signal_overflow_for_test(
        PCNTL_SIGNAL_OWNER_AOT,
        &second
    ));

    for expected_status in [11, 22] {
        assert_eq!(
            unsafe { elephc_pcntl_signal_next(&mut info, PCNTL_SIGNAL_OWNER_AOT) },
            1
        );
        assert_eq!(info.status, expected_status);
        assert_eq!(info.present, SIGINFO_SIGNO | SIGINFO_STATUS);
    }
    assert_eq!(
        unsafe { elephc_pcntl_signal_next(&mut info, PCNTL_SIGNAL_OWNER_AOT) },
        0
    );
}

/// Pins the PHP-style preallocated limit: overflow drops newer records after 4096 entries.
#[test]
fn signal_queue_overflow_drops_newer_records_after_4096_record_capacity() {
    let _guard = PROCESS_TEST_LOCK.lock().expect("process test lock poisoned");
    let mut info = ElephcPcntlSigInfo::default();
    while unsafe { elephc_pcntl_signal_next(&mut info, PCNTL_SIGNAL_OWNER_AOT) } == 1 {}
    assert_eq!(signal_overflow_capacity_for_test(), 4096);

    for status in 0..signal_overflow_capacity_for_test() {
        let record = ElephcPcntlSigInfo {
            signo: i64::from(libc::SIGUSR1),
            status: status as i64,
            present: SIGINFO_SIGNO | SIGINFO_STATUS,
            ..ElephcPcntlSigInfo::default()
        };
        assert!(queue_signal_overflow_for_test(
            PCNTL_SIGNAL_OWNER_AOT,
            &record
        ));
    }
    let dropped = ElephcPcntlSigInfo {
        signo: i64::from(libc::SIGUSR1),
        status: 4096,
        present: SIGINFO_SIGNO | SIGINFO_STATUS,
        ..ElephcPcntlSigInfo::default()
    };
    assert!(!queue_signal_overflow_for_test(
        PCNTL_SIGNAL_OWNER_AOT,
        &dropped
    ));

    for expected_status in 0..signal_overflow_capacity_for_test() {
        assert_eq!(
            unsafe { elephc_pcntl_signal_next(&mut info, PCNTL_SIGNAL_OWNER_AOT) },
            1
        );
        assert_eq!(info.status, expected_status as i64);
    }
    assert_eq!(
        unsafe { elephc_pcntl_signal_next(&mut info, PCNTL_SIGNAL_OWNER_AOT) },
        0
    );
}

/// Drops inherited overflow records when `fork()` gives the child private signal queues.
#[test]
fn signal_queue_overflow_is_not_inherited_by_forked_children() {
    let _guard = PROCESS_TEST_LOCK.lock().expect("process test lock poisoned");
    let mut discarded = ElephcPcntlSigInfo::default();
    while unsafe { elephc_pcntl_signal_next(&mut discarded, PCNTL_SIGNAL_OWNER_AOT) } == 1 {}
    let overflow = ElephcPcntlSigInfo {
        signo: i64::from(libc::SIGUSR1),
        present: SIGINFO_SIGNO,
        ..ElephcPcntlSigInfo::default()
    };
    assert!(queue_signal_overflow_for_test(
        PCNTL_SIGNAL_OWNER_AOT,
        &overflow
    ));

    let pid = elephc_pcntl_fork();
    assert!(
        pid >= 0,
        "fork failed with errno {}",
        elephc_pcntl_get_last_error()
    );
    if pid == 0 {
        let mut inherited = ElephcPcntlSigInfo::default();
        let status = unsafe {
            elephc_pcntl_signal_next(&mut inherited, PCNTL_SIGNAL_OWNER_AOT)
        };
        unsafe { libc::_exit(libc::c_int::from(status != 0)) };
    }

    let mut status = 0;
    assert_eq!(unsafe { elephc_pcntl_waitpid(pid, &mut status, 0) }, pid);
    assert_eq!(elephc_pcntl_wexitstatus(status), 0);
    assert_eq!(
        unsafe { elephc_pcntl_signal_next(&mut discarded, PCNTL_SIGNAL_OWNER_AOT) },
        1
    );
}

/// Receives a queued Linux signal synchronously and exposes sender identity fields.
#[cfg(target_os = "linux")]
#[test]
fn signal_wait_info_receives_a_blocked_signal() {
    let _guard = PROCESS_TEST_LOCK.lock().expect("process test lock poisoned");
    let mut original = unsafe { std::mem::zeroed::<libc::sigset_t>() };
    let mut selected = unsafe { std::mem::zeroed::<libc::sigset_t>() };
    unsafe {
        libc::sigemptyset(&mut selected);
        libc::sigaddset(&mut selected, libc::SIGUSR1);
        assert_eq!(libc::sigprocmask(libc::SIG_BLOCK, &selected, &mut original), 0);
        assert_eq!(libc::raise(libc::SIGUSR1), 0);
    }
    let signals = [i64::from(libc::SIGUSR1)];
    let mut info = ElephcPcntlSigInfo::default();
    let signal = unsafe { elephc_pcntl_sigwaitinfo(signals.as_ptr(), 1, &mut info) };
    unsafe {
        assert_eq!(libc::sigprocmask(libc::SIG_SETMASK, &original, std::ptr::null_mut()), 0);
    }
    assert_eq!(signal, i64::from(libc::SIGUSR1));
    assert_eq!(info.signo, i64::from(libc::SIGUSR1));
    assert_eq!(info.pid, i64::from(unsafe { libc::getpid() }));
    assert_ne!(info.present & SIGINFO_PID, 0);
    assert_ne!(info.present & SIGINFO_UID, 0);
}

/// Times out without replacing PCNTL's last error when no selected signal is pending.
#[cfg(target_os = "linux")]
#[test]
fn timed_signal_wait_preserves_last_error_on_timeout() {
    let _guard = PROCESS_TEST_LOCK.lock().expect("process test lock poisoned");
    LAST_ERROR.store(731, Ordering::Relaxed);
    let signals = [i64::from(libc::SIGUSR2)];
    let signal = unsafe {
        elephc_pcntl_sigtimedwait(signals.as_ptr(), signals.len(), std::ptr::null_mut(), 0, 1)
    };
    assert_eq!(signal, -1);
    assert_eq!(elephc_pcntl_get_last_error(), 731);
}

/// Reads the current process priority without confusing a valid `-1` with failure.
#[test]
fn getpriority_uses_a_separate_success_status() {
    let mut priority = 0;
    let success = unsafe { elephc_pcntl_getpriority(0, libc::PRIO_PROCESS as _, &mut priority) };
    assert_eq!(success, 1, "getpriority failed with errno {}", elephc_pcntl_get_last_error());
    assert!((-20..=20).contains(&priority));
}

/// Returns a non-empty C-library message for a known errno value.
#[test]
fn strerror_returns_borrowed_bytes_and_length() {
    let mut length = 0;
    let pointer = unsafe { elephc_pcntl_strerror(libc::EINVAL, &mut length) };
    assert!(!pointer.is_null());
    assert!(length > 0);
    let bytes = unsafe { std::slice::from_raw_parts(pointer, length) };
    assert!(!bytes.contains(&0));
}

/// Rejects a missing getpriority output pointer with `EFAULT` instead of dereferencing it.
#[test]
fn getpriority_rejects_a_null_output_pointer() {
    let success = unsafe {
        elephc_pcntl_getpriority(0, libc::PRIO_PROCESS as _, std::ptr::null_mut())
    };
    assert_eq!(success, 0);
    assert_eq!(elephc_pcntl_get_last_error(), libc::EFAULT);
}

/// Reads Darwin's current QoS class through the stable five-case ordinal ABI.
#[cfg(target_os = "macos")]
#[test]
fn macos_qos_getter_returns_a_known_case() {
    let qos_class = elephc_pcntl_getqos_class();
    assert!((0..=4).contains(&qos_class));
}

/// Rejects an unknown QoS enum case name without changing the current thread.
#[cfg(target_os = "macos")]
#[test]
fn macos_qos_setter_rejects_unknown_case() {
    let _guard = PROCESS_TEST_LOCK.lock().expect("process test lock poisoned");
    let success = unsafe { elephc_pcntl_setqos_class(b"Unknown".as_ptr(), 7) };
    assert_eq!(success, 0);
    assert_eq!(elephc_pcntl_get_last_error(), libc::EINVAL);
}

/// Changes the current test thread to Default QoS through the case-name bridge.
#[cfg(target_os = "macos")]
#[test]
fn macos_qos_default_setter_round_trips() {
    let _guard = PROCESS_TEST_LOCK.lock().expect("process test lock poisoned");
    let success = unsafe { elephc_pcntl_setqos_class(b"Default".as_ptr(), 7) };
    assert_eq!(
        success,
        1,
        "set Default failed with {}",
        elephc_pcntl_get_last_error()
    );
    assert_eq!(elephc_pcntl_getqos_class(), 2);
}

/// Reapplies the current Darwin QoS case through the case-name bridge.
#[cfg(target_os = "macos")]
#[test]
fn macos_qos_current_setter_round_trips() {
    let _guard = PROCESS_TEST_LOCK.lock().expect("process test lock poisoned");
    let ordinal = elephc_pcntl_getqos_class();
    let name = [
        &b"UserInteractive"[..],
        &b"UserInitiated"[..],
        &b"Default"[..],
        &b"Utility"[..],
        &b"Background"[..],
    ][ordinal as usize];
    let success = unsafe { elephc_pcntl_setqos_class(name.as_ptr(), name.len()) };
    assert_eq!(
        success,
        1,
        "set current failed with {}",
        elephc_pcntl_get_last_error()
    );
    assert_eq!(elephc_pcntl_getqos_class(), ordinal);
}
