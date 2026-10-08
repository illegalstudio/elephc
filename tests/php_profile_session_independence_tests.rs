//! Purpose:
//! Extends the profile-independence proof to the `--web` SESSION surface — the part of
//! `php_profile::sensitivity`'s table that `php_profile_independence_tests` cannot reach.
//!
//! Called from:
//! - `cargo test --test php_profile_session_independence_tests` through Rust's test harness.
//!
//! Key details:
//!
//! - WHY A SEPARATE FILE. `cli.rs` refuses `--web` together with `--emit-asm`, and a session
//!   surface has no output at all until a request is served, so the CLI differential's
//!   "compile, run, compare stdout" shape does not apply. Here the oracle is the HTTP
//!   RESPONSE BODY: compile with `--web` at each maintained profile, serve one request, and
//!   compare. Same claim, same bidirectional discipline, different way of observing it.
//!
//! - THE SAME TWO ARMS, FOR THE SAME REASON. The table's prediction is read from the shipping
//!   code (`sensitivity::scan(program, web = true)`), never from a hand-written per-case
//!   expectation, and both directions are asserted: a program the table calls dependent must
//!   produce different bodies somewhere, and one it calls independent must produce identical
//!   ones. Either arm alone passes vacuously.
//!
//! - COST. Every case compiles and serves at all four profiles, so this file spawns roughly
//!   `cases * 4` servers. It is deliberately small and every probe earns its place by making
//!   a VALUE observable — a returned array's shape, a function's return value — rather than a
//!   diagnostic, because `SensitivityKind` reasoning counts computed values only and warnings
//!   do not reach the response body.
//!
//! - Host-target only (macOS aarch64 locally), same harness style as `web_session_tests`.

use std::fs;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use elephc::php_profile::sensitivity;
use elephc::web_prelude::PhpVersion;

static TEST_ID: AtomicUsize = AtomicUsize::new(0);

/// One probe: a name, and a `--web` program whose RESPONSE BODY exposes the surface.
struct Case {
    /// Identifier used in assertion messages.
    name: &'static str,
    /// The PHP source, compiled once per maintained profile.
    source: &'static str,
}

/// The probes.
///
/// The independent half is not filler: `plain_response` and `session_id_is_not_versioned`
/// are the negative controls proving that merely being a `--web` build, or merely touching
/// the session surface at all, is not what the table reacts to.
const CORPUS: &[Case] = &[
    // ---- expected profile-INDEPENDENT ----
    Case {
        name: "plain_response",
        source: r#"<?php echo "hello from web";"#,
    },
    Case {
        name: "session_name_is_not_versioned",
        source: r#"<?php echo session_name();"#,
    },
    // ---- expected profile-DEPENDENT ----
    Case {
        name: "cookie_params_shape",
        source: r#"<?php
$p = session_get_cookie_params();
ksort($p);
echo implode(",", array_keys($p));
"#,
    },
    Case {
        name: "cookie_params_partitioned_roundtrip",
        source: r#"<?php
session_set_cookie_params(["partitioned" => true, "path" => "/x"]);
$p = session_get_cookie_params();
echo isset($p["partitioned"]) ? "has:" . var_export($p["partitioned"], true) : "absent";
"#,
    },
    Case {
        name: "create_id_long_prefix",
        source: r#"<?php
$long = str_repeat("a", 300);
$r = @session_create_id($long);
echo $r === false ? "rejected" : "accepted";
"#,
    },
];

/// Creates an isolated temp dir unique across parallel test threads/processes.
fn make_test_dir(prefix: &str) -> PathBuf {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("{}_{}_{:?}_{}", prefix, pid, tid, id));
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Resolves the elephc CLI binary path (cargo env var, fallback next to the test binary).
fn elephc_bin() -> String {
    std::env::var("CARGO_BIN_EXE_elephc").unwrap_or_else(|_| {
        let mut path = std::env::current_exe().expect("failed to resolve current test binary");
        path.pop();
        if path.ends_with("deps") {
            path.pop();
        }
        path.join("elephc").to_string_lossy().into_owned()
    })
}

/// Picks an ephemeral localhost port by binding :0 and releasing it.
fn free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

/// Blocks until `addr` accepts a TCP connection, or panics after 10s.
fn wait_until_ready(addr: &str) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if TcpStream::connect(addr).is_ok() {
            return;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    panic!("server did not start listening on {addr}");
}

/// Sends one GET and returns the raw response.
fn http_get(addr: &str) -> String {
    let mut stream = TcpStream::connect(addr).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let request =
        format!("GET / HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).unwrap();
    let mut raw = Vec::new();
    let _ = stream.read_to_end(&mut raw);
    String::from_utf8_lossy(&raw).into_owned()
}

/// Returns just the body of a raw HTTP response.
///
/// Headers carry a `Set-Cookie` session id that changes every run, so comparing whole
/// responses would report every case as different regardless of the profile.
fn body_of(response: &str) -> String {
    match response.split_once("\r\n\r\n") {
        Some((_, body)) => body.trim().to_string(),
        None => response.trim().to_string(),
    }
}

/// Compiles `source` with `--web` at `profile`, serves one request, returns the body.
fn serve_once(dir: &Path, source: &str, profile: &str, case: &str) -> String {
    let php = dir.join("prog.php");
    fs::write(&php, source).unwrap();

    let output = Command::new(elephc_bin()).arg("build")
        .env("XDG_CACHE_HOME", dir.join("cache-root"))
        .current_dir(dir)
        .args(["--web", "--php-version", profile])
        .arg(&php)
        .output()
        .expect("failed to spawn elephc");
    assert!(
        output.status.success(),
        "case `{case}` failed to compile --web at {profile}:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let bin = dir.join("prog");
    let addr = format!("127.0.0.1:{}", free_port());
    let mut server = Command::new(&bin)
        .args(["--listen", &addr, "--workers", "1"])
        .spawn()
        .expect("failed to spawn web server");
    wait_until_ready(&addr);
    let response = http_get(&addr);
    shutdown(&mut server, &bin);
    body_of(&response)
}

/// Stops a spawned server AND the worker it forked.
///
/// `Child::kill` sends SIGKILL, which the parent cannot survive long enough to reap anything:
/// the prefork worker is orphaned and keeps holding its port. Left unfixed this leaks one
/// process per probe — twenty per run of this file — which is how a test file quietly becomes
/// the reason a machine runs out of ports.
///
/// So the parent is asked to terminate (SIGTERM, which it can handle), and any process still
/// running this binary is then swept. The sweep is matched on the probe's own temp-dir path,
/// which is unique per case and per process, so it can never reach another test's servers.
fn shutdown(server: &mut std::process::Child, bin: &Path) {
    let _ = Command::new("kill")
        .arg(server.id().to_string())
        .status();
    std::thread::sleep(Duration::from_millis(150));
    let _ = Command::new("pkill")
        .arg("-f")
        .arg(bin.to_string_lossy().as_ref())
        .status();
    let _ = server.kill();
    let _ = server.wait();
}

/// Returns what the shipping table predicts for `source` under `--web`.
fn table_predicts_independent(source: &str) -> bool {
    let tokens = elephc::lexer::tokenize(source).expect("probe must tokenize");
    let program = elephc::parser::parse(&tokens).expect("probe must parse");
    sensitivity::scan(&program, true).is_empty()
}

/// THE check: for one probe, the table's `--web` prediction must match what the compiler
/// actually serves across every maintained profile.
///
/// One test PER PROBE rather than one loop, for the same reason as the CLI half: a probe is
/// an independent claim, and four compile-serve cycles is already most of a per-test timeout
/// budget. Summing five of them into one test is how a suite that does real work gets
/// terminated for looking hung.
fn check_probe(name: &str) {
    let case = CORPUS
        .iter()
        .find(|candidate| candidate.name == name)
        .unwrap_or_else(|| panic!("no session probe named `{name}`"));
    {
        let dir = make_test_dir(&format!("elephc_sess_{}", case.name));
        let predicted_independent = table_predicts_independent(case.source);

        let bodies: Vec<(String, String)> = PhpVersion::MAINTAINED
            .iter()
            .map(|profile| {
                let spelling = profile.spelling().to_string();
                let body = serve_once(&dir, case.source, &spelling, case.name);
                (spelling, body)
            })
            .collect();

        let baseline = &bodies[0].1;
        let observed_independent = bodies.iter().all(|(_, body)| body == baseline);

        assert_eq!(
            predicted_independent,
            observed_independent,
            "case `{}`: the sensitivity table says {}, the served behavior says {}.\nBodies: {:?}\n{}",
            case.name,
            if predicted_independent { "INDEPENDENT" } else { "DEPENDENT" },
            if observed_independent { "INDEPENDENT" } else { "DEPENDENT" },
            bodies,
            if predicted_independent {
                "The table is MISSING a session symbol: the served response changed for a \
                 program the table promised was profile-independent."
            } else {
                "The table is OVER-BROAD for this session symbol: it would report a \
                 dependence the served behavior does not actually have."
            },
        );

        let _ = fs::remove_dir_all(&dir);
    }
}

/// Generates one `#[test]` per session probe, and records which names are covered.
macro_rules! probe_tests {
    ($($name:ident,)*) => {
        /// Every probe name carrying a generated test, cross-checked against [`CORPUS`] by
        /// `session_corpus_and_tests_agree`.
        const COVERED: &[&str] = &[$(stringify!($name),)*];
        $(
            #[test]
            fn $name() {
                check_probe(stringify!($name));
            }
        )*
    };
}

probe_tests!(
    plain_response,
    session_name_is_not_versioned,
    cookie_params_shape,
    cookie_params_partitioned_roundtrip,
    create_id_long_prefix,
);

/// The probe corpus and the generated tests name exactly the same probes.
///
/// Per-probe tests trade a loop for a second list, and a second list can fall behind. A probe
/// added to [`CORPUS`] but not to `probe_tests!` would simply never run while the file stayed
/// green — the failure mode this file exists to prevent, reappearing one level up.
#[test]
fn session_corpus_and_tests_agree() {
    for case in CORPUS {
        assert!(
            COVERED.contains(&case.name),
            "session probe `{}` has no generated test — add it to `probe_tests!`",
            case.name
        );
    }
    for name in COVERED {
        assert!(
            CORPUS.iter().any(|case| case.name == *name),
            "generated test `{name}` has no session probe"
        );
    }
}

/// Guards this file against becoming vacuous, exactly as its CLI counterpart does.
#[test]
fn session_corpus_exercises_both_arms() {
    let independent = CORPUS
        .iter()
        .filter(|case| table_predicts_independent(case.source))
        .count();
    let dependent = CORPUS.len() - independent;
    assert!(
        independent >= 2,
        "corpus must keep at least 2 profile-independent cases, has {independent}"
    );
    assert!(
        dependent >= 2,
        "corpus must keep at least 2 profile-dependent cases, has {dependent}"
    );
}
