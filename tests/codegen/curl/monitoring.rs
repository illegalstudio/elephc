//! Purpose:
//! The end-to-end regression for #880: PHP callback CPU must not be billed as outgoing
//! network wait. Compiles one program under `--with-curl --with-monitoring`, runs it twice
//! under `elephc monitor` against a real loopback transfer — once burning measurable CPU
//! INSIDE a `CURLOPT_WRITEFUNCTION`, once burning the same CPU immediately AFTER
//! `curl_exec()` as a control — and compares what the two saved captures report as network
//! wait.
//!
//! Called from:
//! - `cargo test --test codegen_tests curl::monitoring` through Rust's test harness.
//!
//! Key details:
//! - THIS IS THE ONLY FIXTURE THAT REACHES THE ACCOUNTING END TO END (issue #890).
//!   `note_wait_excluding` has unit coverage in `crates/elephc-monitoring-contract` and the
//!   nested-callback timer has stack-level coverage in the bridge, but nothing re-ran the
//!   proven PHP program through the compiler and the monitor and looked at the number a
//!   user actually reads. Measured on the pre-fix bridge, the in-callback run reports
//!   **133.9 ms** of network wait where the control reports **1.2 ms**; with the fix both
//!   report ~1.1 ms. Every piece can be individually correct and that number still be the
//!   burn.
//! - THE WAIT LANDS ON `curl_exec`, NOT ON THE USER'S FUNCTION. `curl_exec()` is itself an
//!   elephc-PHP prelude function (`src/curl_prelude.rs`), so it is the frame on the stack
//!   when the bridge reports the boundary. That is also why this runs the binary TWICE
//!   instead of calling both shapes in one run: one run with both would fold the two
//!   transfers into a single `curl_exec` node (`calls 2`) and there would be nothing left
//!   to compare.
//! - IT COMPILES THROUGH THE CLI, NOT THE HARNESS LINKER, and is the only curl fixture that
//!   does. `--with-monitoring` is a compiler feature (probe emission, instrumented frames),
//!   not a link-plan one, so `compile_and_run`'s bare `ld` invocation cannot produce the
//!   binary this needs. The managed native packages therefore resolve through the
//!   PRODUCTION resolver, which keys artifacts on the toolchain fingerprint — and that
//!   fingerprint covers `PATH` and `TMPDIR`, not just the compiler
//!   (`native_deps::toolchain::fingerprinted_environment`). `skip_without_curl_native`'s
//!   structural discovery ignores all of it, so a cache that satisfies the gate can be
//!   unusable here after nothing more than a shell change, and the local cure is a
//!   multi-minute from-source rebuild of the whole curl closure. This fixture therefore
//!   skips on that one diagnostic — REPORTING IT THROUGH `SKIP_GATE_MARKER`, so
//!   `scripts/ci/run_curl_codegen_shard.sh` turns it into a hard CI failure exactly as it
//!   does for a missing cache. CI materializes the cache with `native install --locked` in
//!   the same job and therefore the same environment, so reaching that branch there means
//!   the shard really did lose curl coverage. Every other compile failure still panics.
//! - THE CONTROL IS WHAT MAKES THE ASSERTION MEAN ANYTHING. An absolute "network wait is
//!   small" bound would also pass on a build that stopped recording wait altogether. The
//!   two runs do identical work — one transfer, one burn — and differ ONLY in which side of
//!   `curl_exec()` the burn sits on, so the two captures must agree.

use super::http_fixture::LocalHttpServer;
use crate::support::*;

/// Loop iterations for one burn. Calibrated to ~130 ms on an M-series laptop — two orders
/// of magnitude above a loopback transfer (~1 ms) so no plausible scheduling noise can
/// close the gap, and small enough that two monitored runs stay well under a second.
const BURN_ROUNDS: u64 = 30_000_000;

/// The slack allowed between the two runs' reported network wait. Both are sub-millisecond
/// loopback transfers whose jitter is not proportional to anything, so the bound is a flat
/// few milliseconds rather than a ratio — and it still sits well below the 133.9 ms the
/// pre-fix bridge reported.
///
/// 5 ms held on a quiet laptop and did not survive a shared CI runner. Measured there across
/// four unrelated pull requests in one afternoon, the control stayed at ~0.42 ms every time
/// while the instrumented run came back at 5.56 ms, 7.80 ms and 9.11 ms — loopback scheduling
/// jitter, not billed callback CPU, since the burn is ~130 ms and would show as that.
///
/// 20 ms covers those with better than 2x to spare and still leaves ~6.6x below the 132.7 ms
/// gap the defect produced, so the test keeps failing outright on the regression it guards
/// while no longer failing on the machine it runs on.
const WAIT_SLACK_NS: u64 = 20_000_000;

/// A `CURLOPT_WRITEFUNCTION` that burns CPU must not have that CPU counted as network wait.
///
/// Issue #880 subtracted nested PHP callback time from the easy-transfer boundary; issue
/// #890 is this fixture. The pre-fix behaviour: `elephc_curl_easy_perform` timed the whole
/// `curl_easy_perform()` call and reported all of it as wait, so a transfer whose write
/// callback burned 130 ms of CPU was reported as 130 ms *blocked on the network* — pointing
/// an optimization effort at a network that was never slow.
#[test]
fn curl_monitor_excludes_write_callback_cpu_from_network_wait() {
    if skip_without_curl_native("curl_monitor_excludes_write_callback_cpu_from_network_wait") {
        return;
    }
    ensure_cli_bridge_staticlibs(&["elephc_curl"]);

    let dir = make_cli_test_dir("elephc_curl_monitor_network_wait");
    // The canonical managed-curl manifest/lock pair, so this fixture pins exactly the
    // versions CI materializes with `native install --locked --manifest-path
    // examples/curl-get/elephc.toml` rather than a second copy that could drift from it.
    fs::write(
        dir.join("elephc.toml"),
        include_str!("../../../examples/curl-get/elephc.toml"),
    )
    .expect("curl monitor fixture: write elephc.toml");
    fs::write(
        dir.join("elephc.lock"),
        include_str!("../../../examples/curl-get/elephc.lock"),
    )
    .expect("curl monitor fixture: write elephc.lock");

    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/hello");
    fs::write(
        dir.join("main.php"),
        format!(
            r#"<?php
function burn(int $rounds): int {{
    $n = 0;
    for ($i = 0; $i < $rounds; $i++) {{ $n = ($n + $i) % 100003; }}
    return $n;
}}

// The defect's shape: the CPU burn happens INSIDE the write callback, which runs nested
// inside `curl_easy_perform()`. Its time is part of the transfer's wall clock and must not
// be part of the transfer's network wait.
//
// Both functions RETURN the burn's result and report the bytes they saw, so neither the
// work nor the transfer can be eliminated as dead, and the two runs are comparable only if
// they report the same thing.
//
// THE CALLBACK BURNS ON ITS FIRST INVOCATION ONLY. libcurl does not promise to deliver a
// ten-byte body in one callback, and burning per chunk would make the amount of work depend
// on how it happened to split — the control burns exactly once, so the two sides would stop
// being comparable for a reason that has nothing to do with wait accounting. `$chunks` is
// reported as well, so a split is visible rather than merely survived.
function burn_inside_callback(string $url): int {{
    $seen = 0;
    $chunks = 0;
    $burned = 0;
    $ch = curl_init($url);
    curl_setopt($ch, CURLOPT_WRITEFUNCTION, function (CurlHandle $handle, string $data) use (&$seen, &$chunks, &$burned): int {{
        $seen += strlen($data);
        $chunks++;
        if ($chunks === 1) {{
            $burned += burn({BURN_ROUNDS});
        }}
        return strlen($data);
    }});
    curl_exec($ch);
    echo "<<", $seen, ";", $chunks, ";";
    return $burned;
}}

// The control: identical work, identical transfer, the same single burn — moved to AFTER
// the transfer, where it was never in any danger of being counted as wait.
function burn_after_transfer(string $url): int {{
    $seen = 0;
    $chunks = 0;
    $burned = 0;
    $ch = curl_init($url);
    curl_setopt($ch, CURLOPT_WRITEFUNCTION, function (CurlHandle $handle, string $data) use (&$seen, &$chunks): int {{
        $seen += strlen($data);
        $chunks++;
        return strlen($data);
    }});
    curl_exec($ch);
    $burned += burn({BURN_ROUNDS});
    echo "<<", $seen, ";", $chunks, ";";
    return $burned;
}}

$url = "{url}";
// One binary, two shapes: a single `curl_exec` node per run is what makes the two captures
// comparable, and a program that ran both would merge them into one.
//
// The `<<`/`>>` delimiters are what the harness reads between. The program's stdout is
// interleaved with the monitor's own report on the same stream, and "the first line" only
// worked because the report happens to open with a newline today.
echo getenv("ELEPHC_FIXTURE_BURN") === "inside"
    ? burn_inside_callback($url)
    : burn_after_transfer($url);
echo ">>";
"#
        ),
    )
    .expect("curl monitor fixture: write main.php");

    let cache = managed_native_cache_root()
        .expect("skip_without_curl_native passed, so the managed native cache exists");
    let compile = elephc_cli_command(&dir)
        .args(["--with-curl", "--with-monitoring", "main.php"])
        .env("ELEPHC_NATIVE_CACHE", &cache)
        .output()
        .expect("curl monitor fixture: run the compiler");
    if !compile.status.success() {
        let stderr = String::from_utf8_lossy(&compile.stderr);
        // The PRODUCTION resolver keys artifacts on the toolchain fingerprint — which
        // includes `PATH` and `TMPDIR` (`native_deps::toolchain::fingerprinted_environment`)
        // — while `skip_without_curl_native`'s structural discovery does not. So a cache
        // that satisfies the gate above can still be unusable here, and on a developer
        // machine the cure is a multi-minute from-source rebuild of the whole curl closure.
        //
        // That is a skip, not a failure — but it is reported through the SAME marker the
        // gate above uses, so `scripts/ci/run_curl_codegen_shard.sh` turns it into a hard
        // CI failure exactly as it does for a missing cache. CI materializes the cache with
        // `native install --locked` in the same job and therefore the same environment, so
        // reaching this branch there means the shard really did lose curl coverage.
        if stderr.contains("native integrity error")
            || stderr.contains("requires managed native package")
        {
            eprintln!(
                "{SKIP_GATE_MARKER} skipping \
                 curl_monitor_excludes_write_callback_cpu_from_network_wait: the managed \
                 native curl archives in this cache were not built by the current \
                 toolchain, so the production resolver cannot use them \
                 (run: elephc native install --locked --target {} --manifest-path \
                 examples/curl-get/elephc.toml)\n{stderr}",
                target().as_str()
            );
            let _ = fs::remove_dir_all(&dir);
            return;
        }
        panic!(
            "--with-curl --with-monitoring compile failed\nstdout: {}\nstderr: {stderr}",
            String::from_utf8_lossy(&compile.stdout)
        );
    }

    let run = |shape: &str, capture: &str| -> (String, serde_json::Value) {
        let monitored = elephc_cli_command(&dir)
            .args(["monitor", "./main", "--save", capture])
            .env("ELEPHC_NATIVE_CACHE", &cache)
            .env("ELEPHC_FIXTURE_BURN", shape)
            .output()
            .expect("curl monitor fixture: run elephc monitor");
        let report = format!(
            "{}{}",
            String::from_utf8_lossy(&monitored.stdout),
            String::from_utf8_lossy(&monitored.stderr)
        );
        assert!(
            monitored.status.success(),
            "elephc monitor failed for the `{shape}` run:\n{report}"
        );
        let saved = fs::read_to_string(dir.join(capture))
            .unwrap_or_else(|_| panic!("the `{shape}` run saved no capture:\n{report}"));
        // The fixture brackets its own output with `<<`/`>>` and the harness reads between
        // them. The program and the monitor's report share one stdout, so anything that
        // depends on where the report happens to put its newlines — "the first line", say —
        // is one formatting change away from comparing report text instead.
        let start = report
            .find("<<")
            .unwrap_or_else(|| panic!("the `{shape}` run printed no fixture output:\n{report}"));
        let end = report[start..]
            .find(">>")
            .map(|offset| start + offset)
            .unwrap_or_else(|| panic!("the `{shape}` run's output was truncated:\n{report}"));
        let program_output = report[start + 2..end].to_string();
        (
            program_output,
            serde_json::from_str(&saved).expect("curl monitor fixture: capture must be JSON"),
        )
    };

    let (inside_output, inside) = run("inside", "inside.json");
    let (after_output, after) = run("after", "after.json");

    // The two runs are only comparable if they did the same work. Each prints
    // `<bytes>;<chunks>;<burn result>`, and BOTH halves have to be checked on BOTH runs:
    // the burn result alone cannot distinguish a control that transferred nothing, because
    // `burn` is pure and runs once either way — a short or empty control response would
    // then compare network waits from transfers that are not equivalent.
    //
    // The chunk count is reported rather than asserted equal: the burn is pinned to the
    // first invocation, so a split body no longer changes the work, but seeing the split in
    // the failure message is worth more than hiding it.
    for (shape, output) in [("in-callback", &inside_output), ("control", &after_output)] {
        assert!(
            output.starts_with("10;"),
            "the {shape} run did not receive the fixture body: {output:?}"
        );
    }
    let burn_result = |output: &str| -> String {
        output
            .rsplit_once(';')
            .map(|(_, burned)| burned.to_string())
            .unwrap_or_default()
    };
    assert_eq!(
        burn_result(&inside_output),
        burn_result(&after_output),
        "the two runs must burn the same amount; inside={inside_output:?} after={after_output:?}"
    );

    // `curl_exec()` is the prelude function on the stack when the bridge reports the
    // boundary, so it is the frame that carries the transfer's operation count and wait.
    let curl_exec_wait = |capture: &serde_json::Value, shape: &str| -> u64 {
        let node = capture["nodes"]
            .as_array()
            .expect("capture nodes")
            .iter()
            .find(|node| node["name"] == "curl_exec")
            .unwrap_or_else(|| panic!("the `{shape}` capture has no `curl_exec` frame:\n{capture}"));
        assert_eq!(
            node["network_exclusive"].as_u64(),
            Some(1),
            "the `{shape}` run did not count its transfer as one network operation:\n{capture}"
        );
        node["network_wait_exclusive"]
            .as_u64()
            .unwrap_or_else(|| panic!("`curl_exec` has no numeric network wait:\n{capture}"))
    };
    let inside_wait = curl_exec_wait(&inside, "inside");
    let after_wait = curl_exec_wait(&after, "after");

    // The burn is real and dominates both runs, so every bound below is a statement about
    // ~130 ms of PHP CPU and not about a fixture that finished too fast to measure.
    for (shape, capture) in [("inside", &inside), ("after", &after)] {
        let total = capture["total"].as_u64().expect("capture total");
        assert!(
            total > 20_000_000,
            "the `{shape}` run took {total} ns — too short for this fixture to distinguish \
             anything; raise BURN_ROUNDS:\n{capture}"
        );
    }

    // The control must record SOME wait: a build that stopped recording network wait
    // altogether would otherwise satisfy the comparison below trivially.
    assert!(
        after_wait > 0,
        "the control recorded no network wait at all, so the comparison below proves \
         nothing:\n{after:?}"
    );

    // The regression itself. Moving the burn across the `curl_exec()` boundary must not
    // change what the profile reports as network wait. Pre-fix this side reported the whole
    // burn — 133.9 ms against the control's 1.2 ms.
    assert!(
        inside_wait < after_wait + WAIT_SLACK_NS,
        "moving the burn into the write callback raised reported network wait from \
         {after_wait} ns to {inside_wait} ns for the same transfer — write-callback CPU is \
         being billed as time blocked on the network"
    );

    let _ = fs::remove_dir_all(&dir);
}
