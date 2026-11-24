//! Purpose:
//! End-to-end tests for transaction monitoring: a `--web --with-monitoring`
//! service started with `--apm` counts its requests per route, and
//! `elephc monitor <endpoint> --apm` reads them back.
//!
//! Called from:
//! - `tests/web_tests.rs` through Rust's integration test harness.
//!
//! Key details:
//! - The probe endpoint is a TCP address picked like the HTTP one; the build
//!   key sits next to the binary as `<binary>.key`.
//! - Counts are asserted exactly: every request the test sends is one the
//!   service must have recorded, under the route shape it belongs to.

use super::*;

/// A small app with one route per status class worth counting.
const APP: &str = r#"<?php
$path = parse_url($_SERVER['REQUEST_URI'], PHP_URL_PATH);
if ($path === '/boom') {
    http_response_code(500);
    echo "boom";
} elseif (str_starts_with($path, '/users/')) {
    echo "user";
} else {
    http_response_code(404);
    echo "nope";
}
"#;

/// Waits until the probe endpoint accepts connections.
fn wait_for_probe(addr: &str) {
    wait_until_ready(addr);
}

/// Starts `binary` with `--apm` and its probe endpoint on a fresh port.
fn spawn_apm_server(binary: &Path, http: &str, probe: &str, apm: bool) -> ServerGuard {
    let mut command = Command::new(binary);
    command
        .env("ELEPHC_PROBE_ADDR", probe)
        .arg("--listen")
        .arg(http)
        .arg("--workers")
        .arg("2");
    if apm {
        command.arg("--apm");
    }
    let child = command.spawn_group().expect("failed to spawn web server");
    wait_until_ready(http);
    wait_for_probe(probe);
    child
}

/// Runs `elephc monitor <probe> --apm` and returns its output.
fn read_transactions(binary: &Path, probe: &str) -> std::process::Output {
    let key = format!("{}.key", binary.display());
    let mut command = Command::new(elephc_bin());
    command.args(["monitor", probe, "--apm", "--key", &key]);
    output_with_timeout(command, Duration::from_secs(30))
}

/// The row a route got in the transactions table, split into its columns.
fn row<'a>(table: &'a str, route: &str) -> Vec<&'a str> {
    let line = table
        .lines()
        .find(|line| line.starts_with(route))
        .unwrap_or_else(|| panic!("no row for {route}:\n{table}"));
    line[route.len()..].split_whitespace().collect()
}

/// Sends the fixed traffic both models are checked against.
fn send_traffic(http: &str) {
    for id in [1, 22, 333] {
        assert!(http_get(http, &format!("/users/{id}")).starts_with("HTTP/1.1 200"));
    }
    for _ in 0..2 {
        assert!(http_get(http, "/missing").starts_with("HTTP/1.1 404"));
    }
    assert!(http_get(http, "/boom").starts_with("HTTP/1.1 500"));
}

/// Asserts the table holds exactly the traffic `send_traffic` produced.
fn assert_counted(output: &std::process::Output) {
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "monitor --apm failed:\n{stdout}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    // Identifiers collapse into one shape, so three users are one route.
    let users = row(&stdout, "GET /users/{id}");
    assert_eq!(users[0], "3", "{stdout}");
    assert_eq!(&users[users.len() - 3..users.len() - 1], ["0.0%", "0.0%"], "{stdout}");
    // A 404 names no route: it is counted under one row and spends no slot of
    // the route table, so a scanner cannot fill it.
    let missing = row(&stdout, "<not found>");
    assert_eq!(missing[0], "2", "{stdout}");
    assert_eq!(missing[missing.len() - 3], "100.0%", "all 404s: {stdout}");
    assert!(!stdout.contains("GET /missing"), "{stdout}");
    let boom = row(&stdout, "GET /boom");
    assert_eq!(boom[0], "1", "{stdout}");
    assert_eq!(boom[boom.len() - 2], "100.0%", "the 500 is a server error: {stdout}");
    assert!(stdout.contains("6 requests over"), "{stdout}");
    // The service measures its whole process tree: the master and its two
    // workers at least (request isolation adds a handler while one runs).
    let service = stdout
        .lines()
        .find(|line| line.starts_with("service: CPU "))
        .unwrap_or_else(|| panic!("no service resources line: {stdout}"));
    let processes: u32 = service
        .rsplit(" · ")
        .next()
        .and_then(|tail| tail.split_whitespace().next())
        .and_then(|n| n.parse().ok())
        .unwrap_or_else(|| panic!("unreadable process count: {service}"));
    assert!(processes >= 3, "master and two workers: {service}");
    // CPU is a share of every core the service may use, not of one.
    assert!(service.contains("% of ") && service.contains(" cores ("), "{service}");
    assert!(service.contains(" MB") || service.contains(" GB"), "{service}");
}

/// A worker-model service counts every request per route from the first one,
/// with nobody connected while the traffic ran.
#[test]
fn web_apm_counts_transactions_per_route() {
    let fixture = make_test_dir("web_apm");
    let binary = compile_web_with_flags(&fixture, APP, "app", &["--with-monitoring"]);
    let http = format!("127.0.0.1:{}", free_port());
    let probe = format!("127.0.0.1:{}", free_port());
    let server = spawn_apm_server(&binary, &http, &probe, true);
    send_traffic(&http);
    assert_counted(&read_transactions(&binary, &probe));
    drop(server);
}

/// Request isolation runs PHP in a forked handler child; the outcome is still
/// recorded, by the web worker that answered the client.
#[test]
fn web_apm_counts_transactions_under_request_isolation() {
    let fixture = make_test_dir("web_apm_isolated");
    let binary = compile_web_with_flags(
        &fixture,
        APP,
        "app",
        &["--with-monitoring", "--web-isolation=request"],
    );
    let http = format!("127.0.0.1:{}", free_port());
    let probe = format!("127.0.0.1:{}", free_port());
    let server = spawn_apm_server(&binary, &http, &probe, true);
    send_traffic(&http);
    assert_counted(&read_transactions(&binary, &probe));
    drop(server);
}

/// A monitored service started without `--apm` counts nothing, and the
/// client says which flag is missing instead of showing an idle service.
#[test]
fn web_apm_off_says_how_to_turn_it_on() {
    let fixture = make_test_dir("web_apm_off");
    let binary = compile_web_with_flags(&fixture, APP, "app", &["--with-monitoring"]);
    let http = format!("127.0.0.1:{}", free_port());
    let probe = format!("127.0.0.1:{}", free_port());
    let server = spawn_apm_server(&binary, &http, &probe, false);
    send_traffic(&http);
    let output = read_transactions(&binary, &probe);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("start the service with --apm"), "{stderr}");
    drop(server);
}

/// A binary that cannot count refuses `--apm` at startup rather than serving
/// as if monitored while recording nothing.
#[test]
fn web_apm_is_refused_without_monitoring() {
    let fixture = make_test_dir("web_apm_refused");
    let binary = compile_web(&fixture, APP, "app");
    let http = format!("127.0.0.1:{}", free_port());
    let mut command = Command::new(&binary);
    command.args(["--listen", &http, "--workers", "1", "--apm"]);
    let output = output_with_timeout(command, Duration::from_secs(10));
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("--with-monitoring"), "{stderr}");
}

/// A route's database work is counted per request under `--apm` in the
/// combined `--with-monitoring` build — whose exact profiler owns the database
/// slots and forwards each event — with nobody profiling. Under
/// `--web-isolation=request` the PHP runs in a handler child and its web worker
/// records the request: a new route's first queries, held until its answer,
/// still land on its row.
#[test]
fn web_apm_counts_database_queries_per_route() {
    for isolation in [None, Some("--web-isolation=request")] {
        database_queries_are_counted_per_route(isolation);
    }
}

/// Runs the per-route database count against a service built with `isolation`.
fn database_queries_are_counted_per_route(isolation: Option<&str>) {
    let fixture = make_test_dir("web_apm_pdo");
    let source = r#"<?php
$pdo = new PDO('sqlite::memory:');
$pdo->exec('CREATE TABLE t (n INTEGER)');
$path = parse_url($_SERVER['REQUEST_URI'], PHP_URL_PATH);
if ($path === '/three') {
    for ($i = 0; $i < 3; $i++) {
        $pdo->query('SELECT ' . $i)->fetchAll();
    }
}
echo "ok";
"#;
    let mut flags = vec!["--with-monitoring"];
    flags.extend(isolation);
    let binary = compile_web_with_flags(&fixture, source, "app", &flags);
    let http = format!("127.0.0.1:{}", free_port());
    let probe = format!("127.0.0.1:{}", free_port());
    let server = spawn_apm_server(&binary, &http, &probe, true);
    for _ in 0..2 {
        assert!(http_get(&http, "/three").starts_with("HTTP/1.1 200"));
        assert!(http_get(&http, "/none").starts_with("HTTP/1.1 200"));
    }
    let output = read_transactions(&binary, &probe);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "{stdout}\n{}", String::from_utf8_lossy(&output.stderr));
    assert!(stdout.contains("q/req"), "database columns appear once a route ran queries: {stdout}");
    // Each request runs CREATE TABLE, then three SELECTs on /three.
    let three = row(&stdout, "GET /three");
    assert_eq!(three[three.len() - 2], "4.0", "four statements per /three request ({isolation:?}): {stdout}");
    let none = row(&stdout, "GET /none");
    assert_eq!(none[none.len() - 2], "1.0", "one statement per /none request ({isolation:?}): {stdout}");
    drop(server);
}

/// `--capture-every` keeps exact profiles of the service's requests: a
/// failing request is kept with its route and status, and opens on the
/// monitor page as its own exact call graph.
#[test]
fn web_apm_captures_failed_requests_with_their_profile() {
    let fixture = make_test_dir("web_apm_capture");
    let source = r#"<?php
function fail_hard(): void { http_response_code(500); echo "boom"; }
fail_hard();
"#;
    let binary = compile_web_with_flags(&fixture, source, "app", &["--with-monitoring"]);
    let http = format!("127.0.0.1:{}", free_port());
    let probe = format!("127.0.0.1:{}", free_port());
    let page = format!("127.0.0.1:{}", free_port());
    let server = spawn_apm_server(&binary, &http, &probe, true);
    let key = format!("{}.key", binary.display());
    let collector = Command::new(elephc_bin())
        .args(["monitor", &probe, "--apm", "--key", &key, "--serve", &page, "--capture-every", "1"])
        .spawn_group()
        .expect("failed to start the collector");
    wait_until_ready(&page);
    // Keep requests coming so each capture has one to take, until one is kept.
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut listing = String::new();
    while Instant::now() < deadline {
        assert!(http_get(&http, "/boom").starts_with("HTTP/1.1 500"));
        listing = http_get(&page, "/api/traces");
        if listing.contains("\"route\":\"GET /boom\"") {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(listing.contains("\"route\":\"GET /boom\""), "no request was captured: {listing}");
    assert!(listing.contains("\"status\":500"), "the capture carries its status: {listing}");
    assert!(listing.contains("\"failed\":true"), "{listing}");
    let id_start = listing.find("\"id\":").expect("an id") + 5;
    let id: String = listing[id_start..].chars().take_while(|c| c.is_ascii_digit()).collect();
    let profile = http_get(&page, &format!("/?trace={id}"));
    assert!(profile.contains("{main}"), "the page shows the request's exact call graph");
    assert!(profile.contains("\"exact\":true"), "measured, not sampled");
    assert!(profile.contains("\"trace\":{"), "and says which request it is");
    drop(collector);
    drop(server);
}

/// A request whose PHP takes its worker down — an uncaught exception, `exit`
/// — never reaches the request's end, and used to vanish from the counts:
/// the error rate read healthy while requests died. The master counts it as a
/// server error when it reaps the worker.
#[test]
fn web_apm_counts_requests_that_kill_their_worker() {
    let fixture = make_test_dir("web_apm_crash");
    let source = r#"<?php
$path = parse_url($_SERVER['REQUEST_URI'], PHP_URL_PATH);
if ($path === '/throw') {
    throw new RuntimeException("boom");
}
if ($path === '/exit') {
    exit(3);
}
echo "ok";
"#;
    let binary = compile_web_with_flags(&fixture, source, "app", &["--with-monitoring"]);
    let http = format!("127.0.0.1:{}", free_port());
    let probe = format!("127.0.0.1:{}", free_port());
    let server = spawn_apm_server(&binary, &http, &probe, true);
    assert!(http_get(&http, "/ok").starts_with("HTTP/1.1 200"));
    for path in ["/throw", "/exit"] {
        // The connection dies with the worker; the response does not matter.
        let _ = http_get_with_timeout(&http, path, Duration::from_secs(5));
    }
    // The master reaps and respawns; give it a moment, then read.
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut stdout = String::new();
    while Instant::now() < deadline {
        let output = read_transactions(&binary, &probe);
        stdout = String::from_utf8_lossy(&output.stdout).to_string();
        if stdout.contains("GET /throw") && stdout.contains("GET /exit") {
            break;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    for route in ["GET /throw", "GET /exit"] {
        let cells = row(&stdout, route);
        assert_eq!(cells[0], "1", "{route} counted once: {stdout}");
        assert_eq!(cells[cells.len() - 2], "100.0%", "{route} is a server error: {stdout}");
    }
    drop(server);
}

/// Under `--web-isolation=request` PHP runs in handler children, so a web
/// worker never dies of PHP — but it can be killed (the out-of-memory killer).
/// The request it was waiting on is counted as a server error when the master
/// reaps it, like a prefork worker's (review: it was not counted at all).
#[test]
fn web_apm_counts_requests_of_a_killed_isolated_web_worker() {
    let fixture = make_test_dir("web_apm_isolated_kill");
    // The handler says when the slow request runs: by then its web worker has
    // marked it in flight. A fixed wait could kill an idle worker first on a
    // loaded machine, and the request then succeeded elsewhere (review).
    let started = fixture.join("slow-started");
    let source = format!(
        r#"<?php
$path = parse_url($_SERVER['REQUEST_URI'], PHP_URL_PATH);
if ($path === '/slow') {{
    file_put_contents('{}', '1');
    sleep(4);
}}
echo "ok";
"#,
        started.display()
    );
    let binary = compile_web_with_flags(&fixture, &source, "app", &["--with-monitoring", "--web-isolation=request"]);
    let http = format!("127.0.0.1:{}", free_port());
    let probe = format!("127.0.0.1:{}", free_port());
    let server = spawn_apm_server(&binary, &http, &probe, true);
    assert!(http_get(&http, "/ok").starts_with("HTTP/1.1 200"));
    let slow_http = http.clone();
    let slow = std::thread::spawn(move || http_get_with_timeout(&slow_http, "/slow", Duration::from_secs(10)));
    let running = Instant::now() + Duration::from_secs(10);
    while !started.exists() && Instant::now() < running {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(started.exists(), "the slow request reached its handler");
    // The master's direct children are its web workers; their handler
    // children are a level below. Kill the web workers, as the kernel would.
    // The suite's own process-tree reader and a direct signal: `pgrep -P` is
    // not on every host the suite runs on (review).
    let workers = direct_child_process_ids(server.child.id());
    assert!(!workers.is_empty(), "the master has web workers");
    for pid in workers {
        unsafe { libc::kill(pid as libc::pid_t, libc::SIGKILL) };
    }
    let _ = slow.join();
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut stdout = String::new();
    while Instant::now() < deadline {
        let output = read_transactions(&binary, &probe);
        stdout = String::from_utf8_lossy(&output.stdout).to_string();
        if stdout.contains("GET /slow") {
            break;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    let cells = row(&stdout, "GET /slow");
    assert_eq!(cells[0], "1", "the killed worker's request is counted once: {stdout}");
    assert_eq!(cells[cells.len() - 2], "100.0%", "as a server error: {stdout}");
    drop(server);
}

/// A request whose client goes away before the answer is counted, as a 499,
/// in an isolated mode too: hyper drops the request's future when the client
/// closes, and the request vanished from the counts while the prefork worker
/// counts the same request (review).
#[test]
fn web_apm_counts_a_request_its_client_abandoned() {
    let fixture = make_test_dir("web_apm_isolated_abandon");
    // The handler says when the slow request runs, so the client goes away
    // while it is in flight however loaded the machine is.
    let started = fixture.join("slow-started");
    let source = format!(
        r#"<?php
$path = parse_url($_SERVER['REQUEST_URI'], PHP_URL_PATH);
if ($path === '/slow') {{
    file_put_contents('{}', '1');
    sleep(2);
}}
echo "ok";
"#,
        started.display()
    );
    let binary = compile_web_with_flags(&fixture, &source, "app", &["--with-apm", "--web-isolation=pool"]);
    let http = format!("127.0.0.1:{}", free_port());
    let probe = format!("127.0.0.1:{}", free_port());
    let server = spawn_apm_server(&binary, &http, &probe, true);
    assert!(http_get(&http, "/ok").starts_with("HTTP/1.1 200"));
    {
        let mut client = TcpStream::connect(&http).expect("connect");
        client.write_all(b"GET /slow HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n").expect("send");
        let running = Instant::now() + Duration::from_secs(10);
        while !started.exists() && Instant::now() < running {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(started.exists(), "the slow request reached its handler");
    }
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut stdout = String::new();
    while Instant::now() < deadline {
        let output = read_transactions(&binary, &probe);
        stdout = String::from_utf8_lossy(&output.stdout).to_string();
        if stdout.contains("GET /slow") {
            break;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    let cells = row(&stdout, "GET /slow");
    assert_eq!(cells[0], "1", "the abandoned request is counted once: {stdout}");
    assert_eq!(cells[cells.len() - 3], "100.0%", "as a client error: {stdout}");
    drop(server);
}

/// The project's `elephc.toml` turns transaction monitoring on for the build, and its
/// `[apm] exclude` keeps a health check out of the table: the route is served, and never
/// counted, while the others are.
#[test]
fn web_apm_project_file_excludes_a_route() {
    let fixture = make_test_dir("web_apm_project_exclude");
    std::fs::write(
        fixture.join("elephc.toml"),
        "[apm]\nenabled = true\nexclude = [\"GET /health\"]\n",
    )
    .expect("write elephc.toml");
    let source = r#"<?php
$path = parse_url($_SERVER['REQUEST_URI'], PHP_URL_PATH);
echo $path === '/health' ? "up" : "page";
"#;
    // No --with-apm on the command line: the project file asks for it.
    let binary = compile_web_with_flags(&fixture, source, "app", &[]);
    let http = format!("127.0.0.1:{}", free_port());
    let probe = format!("127.0.0.1:{}", free_port());
    let server = spawn_apm_server(&binary, &http, &probe, true);
    for _ in 0..3 {
        assert!(http_get(&http, "/health").contains("up"), "the excluded route is still served");
        assert!(http_get(&http, "/page").contains("page"));
    }
    let output = read_transactions(&binary, &probe);
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    assert_eq!(row(&stdout, "GET /page")[0], "3", "{stdout}");
    assert!(!stdout.contains("GET /health"), "the excluded route has no row: {stdout}");
    drop(server);
}

/// `--reset-routes` empties a running service's route table: the counters
/// start a new run, and the routes come back from the next requests only.
#[test]
fn web_apm_reset_routes_empties_the_table() {
    let fixture = make_test_dir("web_apm_reset");
    let binary = compile_web_with_flags(&fixture, APP, "app", &["--with-monitoring"]);
    let http = format!("127.0.0.1:{}", free_port());
    let probe = format!("127.0.0.1:{}", free_port());
    let server = spawn_apm_server(&binary, &http, &probe, true);
    send_traffic(&http);
    let before = String::from_utf8_lossy(&read_transactions(&binary, &probe).stdout).to_string();
    assert!(before.contains("GET /users/{id}"), "{before}");
    let key = format!("{}.key", binary.display());
    let mut command = Command::new(elephc_bin());
    command.args(["monitor", &probe, "--apm", "--reset-routes", "--key", &key]);
    let reset = output_with_timeout(command, Duration::from_secs(30));
    let said = String::from_utf8_lossy(&reset.stdout).to_string();
    assert!(reset.status.success() && said.contains("route table reset:") && said.contains("routes cleared"), "{said}");
    let empty = String::from_utf8_lossy(&read_transactions(&binary, &probe).stdout).to_string();
    assert!(!empty.contains("GET /users/{id}"), "the old routes are gone: {empty}");
    assert!(http_get(&http, "/boom").starts_with("HTTP/1.1 500"));
    let after = String::from_utf8_lossy(&read_transactions(&binary, &probe).stdout).to_string();
    let cells = row(&after, "GET /boom");
    assert_eq!(cells[0], "1", "the next request fills it again: {after}");
    assert!(!after.contains("GET /users/{id}"), "{after}");
    drop(server);
}

/// The collector end to end: a slow request whose statements span several
/// polls reaches the store with every statement, and browsing the store reads
/// them back. One-shot reads never exercised the poll loop.
#[test]
fn web_apm_collector_stores_a_slow_routes_work_across_polls() {
    let fixture = make_test_dir("web_apm_collect");
    let source = r#"<?php
$pdo = new PDO('sqlite::memory:');
$pdo->exec('CREATE TABLE t (n INTEGER)');
if (!isset($_GET['fast'])) {
    for ($i = 0; $i < 10; $i++) { $pdo->query('SELECT ' . $i)->fetchAll(); }
    usleep(2500000);
    for ($i = 0; $i < 10; $i++) { $pdo->query('SELECT ' . $i)->fetchAll(); }
}
echo "done";
"#;
    let binary = compile_web_with_flags(&fixture, source, "app", &["--with-monitoring"]);
    let http = format!("127.0.0.1:{}", free_port());
    let probe = format!("127.0.0.1:{}", free_port());
    let server = spawn_apm_server(&binary, &http, &probe, true);
    let key = format!("{}.key", binary.display());
    let store = fixture.join("store");
    let collector = Command::new(elephc_bin())
        .args(["monitor", &probe, "--apm", "--key", &key, "--resolution", "1", "--store"])
        .arg(&store)
        .spawn_group()
        .expect("failed to start the collector");
    std::thread::sleep(Duration::from_millis(1500));
    // A first, fast request puts the route in the table; the slow one then runs
    // across several polls. Its statements stay in its worker until it answers
    // and are published with it, so every one of them must reach the store
    // however the polls fall.
    assert!(http_get(&http, "/slow?fast=1").starts_with("HTTP/1.1 200"));
    std::thread::sleep(Duration::from_millis(1500));
    assert!(http_get(&http, "/slow").starts_with("HTTP/1.1 200"));
    std::thread::sleep(Duration::from_millis(2500));
    // SIGTERM: the collector writes what it holds before it exits.
    drop(collector);
    let mut command = Command::new(elephc_bin());
    command.args(["monitor", "--apm", "--store"]).arg(&store);
    let output = output_with_timeout(command, Duration::from_secs(30));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "{stdout}\n{}", String::from_utf8_lossy(&output.stderr));
    let slow = row(&stdout, "GET /slow");
    assert_eq!(slow[0], "2", "{stdout}");
    // One CREATE TABLE, then one CREATE TABLE and twenty SELECTs counted over
    // several polls: 22 statements over 2 requests.
    assert_eq!(slow[slow.len() - 2], "11.0", "every statement reached the store: {stdout}");
    drop(server);
}

/// A request that fails with an uncaught exception has its error counted, grouped by class
/// and the place it was thrown from: once per failed request, and never for a request
/// that did not fail.
#[test]
fn web_apm_groups_the_errors_of_failed_requests() {
    errors_are_grouped(&[]);
}

/// The same under request isolation: the handler child that ran the request counts its
/// error, though its web worker records the request.
#[test]
fn web_apm_groups_the_errors_of_isolated_requests() {
    errors_are_grouped(&["--web-isolation=request"]);
}

/// Serves failing and succeeding requests built with `flags` and checks their errors.
fn errors_are_grouped(flags: &[&str]) {
    let fixture = make_test_dir("web_apm_errors");
    let source = r#"<?php
$path = parse_url($_SERVER['REQUEST_URI'], PHP_URL_PATH);
if ($path === '/pay') {
    throw new RuntimeException('card declined');
}
if ($path === '/caught') {
    try { throw new LogicException('handled'); } catch (LogicException $e) {}
}
echo "ok";
"#;
    let mut all = vec!["--with-monitoring"];
    all.extend_from_slice(flags);
    let binary = compile_web_with_flags(&fixture, source, "app", &all);
    let http = format!("127.0.0.1:{}", free_port());
    let probe = format!("127.0.0.1:{}", free_port());
    let server = spawn_apm_server(&binary, &http, &probe, true);
    for _ in 0..3 {
        assert!(http_get(&http, "/pay").starts_with("HTTP/1.1 500"));
        assert!(http_get(&http, "/caught").starts_with("HTTP/1.1 200"));
    }
    let output = read_transactions(&binary, &probe);
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    assert!(stdout.contains("errors of failed transactions — 3 in 1 group"), "{stdout}");
    let line = stdout.lines().find(|line| line.contains("RuntimeException")).unwrap_or_else(|| panic!("{stdout}"));
    assert!(line.trim_start().starts_with("3 ") && line.contains("app.php:4") && line.ends_with("(GET /pay)"), "{line}");
    assert!(!stdout.contains("LogicException"), "a caught exception fails nothing: {stdout}");
    drop(server);
}

/// A collector started with `--otlp` posts what it measured to an OTLP/HTTP endpoint as
/// metrics: here a listener of the test's own, which keeps every request it is sent.
#[test]
fn web_apm_collector_exports_otlp_metrics() {
    let fixture = make_test_dir("web_apm_otlp");
    let binary = compile_web_with_flags(&fixture, APP, "app", &["--with-monitoring"]);
    let http = format!("127.0.0.1:{}", free_port());
    let probe = format!("127.0.0.1:{}", free_port());
    let server = spawn_apm_server(&binary, &http, &probe, true);
    let receiver = std::net::TcpListener::bind("127.0.0.1:0").expect("bind the OTLP receiver");
    let endpoint = format!("http://{}", receiver.local_addr().unwrap());
    let received = std::sync::Arc::new(std::sync::Mutex::new(Vec::<Vec<u8>>::new()));
    let kept = received.clone();
    std::thread::spawn(move || {
        for stream in receiver.incoming().flatten() {
            let mut stream = stream;
            let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
            let mut request = Vec::new();
            let mut chunk = [0u8; 65536];
            // Read the head, then the body its Content-Length announces.
            loop {
                match std::io::Read::read(&mut stream, &mut chunk) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => request.extend_from_slice(&chunk[..n]),
                }
                let text = String::from_utf8_lossy(&request).to_string();
                if let Some(end) = text.find("\r\n\r\n") {
                    let length = text[..end]
                        .lines()
                        .find_map(|line| line.to_ascii_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse::<usize>().unwrap_or(0)))
                        .unwrap_or(0);
                    if request.len() >= end + 4 + length {
                        break;
                    }
                }
            }
            let _ = std::io::Write::write_all(&mut stream, b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
            kept.lock().unwrap().push(request);
        }
    });
    let key = format!("{}.key", binary.display());
    let collector = Command::new(elephc_bin())
        .args(["monitor", &probe, "--apm", "--key", &key, "--resolution", "1", "--otlp", &endpoint])
        .spawn_group()
        .expect("failed to start the collector");
    std::thread::sleep(Duration::from_millis(1200));
    send_traffic(&http);
    std::thread::sleep(Duration::from_millis(2500));
    // SIGTERM: the collector posts what it holds before it exits.
    drop(collector);
    std::thread::sleep(Duration::from_millis(500));
    let requests = received.lock().unwrap().clone();
    let all: Vec<String> = requests.iter().map(|r| String::from_utf8_lossy(r).to_string()).collect();
    assert!(all.iter().any(|r| r.starts_with("POST /v1/metrics ")), "{} posts: {all:?}", all.len());
    let joined = all.join("\n");
    for expected in ["application/x-protobuf", "elephc.transactions", "elephc.transaction.duration", "GET /users/{id}", "service.name"] {
        assert!(joined.contains(expected), "{expected} was exported: {joined:?}");
    }
    drop(server);
}

/// A `--with-apm` service continues the trace a request's `traceparent` names, with a
/// span of its own, and publishes the context the bridges send on (curl reads it
/// through the same contract); a request without one starts a trace. The request's error
/// keeps the trace id, so the collector's store points at the trace.
#[test]
fn web_apm_propagates_the_trace_context_and_keeps_it_with_errors() {
    let fixture = make_test_dir("web_apm_trace");
    let source = r#"<?php
$context = (string) getenv('ELEPHC_TRACEPARENT');
if (parse_url($_SERVER['REQUEST_URI'], PHP_URL_PATH) === '/pay') {
    throw new RuntimeException('declined under ' . $context);
}
echo $context;
"#;
    let binary = compile_web_with_flags(&fixture, source, "app", &["--with-apm"]);
    let http = format!("127.0.0.1:{}", free_port());
    let probe = format!("127.0.0.1:{}", free_port());
    let server = spawn_apm_server(&binary, &http, &probe, true);
    let key = format!("{}.key", binary.display());
    let store = fixture.join("store");
    let collector = Command::new(elephc_bin())
        .args(["monitor", &probe, "--apm", "--key", &key, "--resolution", "1", "--store"])
        .arg(&store)
        .spawn_group()
        .expect("failed to start the collector");
    std::thread::sleep(Duration::from_millis(1200));
    let trace = "4bf92f3577b34da6a3ce929d0e0e4736";
    let inbound = format!("00-{trace}-00f067aa0ba902b7-01");
    let continued = raw_response_body(&http_request(&http, "GET", "/", &[("traceparent", &inbound)], "")).to_string();
    assert!(continued.starts_with(&format!("00-{trace}-")) && continued.ends_with("-01"), "{continued}");
    assert!(!continued.contains("00f067aa0ba902b7"), "a span of the service's own: {continued}");
    let started = raw_response_body(&http_request(&http, "GET", "/", &[], "")).to_string();
    assert!(started.len() == 55 && !started.contains(trace), "a new trace: {started}");
    let answer = http_request(&http, "GET", "/pay", &[("traceparent", &inbound)], "");
    assert!(answer.starts_with("HTTP/1.1 500"), "{answer}");
    std::thread::sleep(Duration::from_millis(2000));
    drop(collector);
    let stored: String = std::fs::read_dir(&store)
        .unwrap()
        .flatten()
        .filter(|entry| entry.file_name().to_string_lossy().starts_with("apm-"))
        .filter_map(|entry| std::fs::read_to_string(entry.path()).ok())
        .collect();
    assert!(stored.contains(&format!("\"trace\":\"{trace}\"")), "the error keeps the trace id: {stored}");
    drop(server);
}

/// Each request's peak memory is counted on its route: a route that builds a few
/// megabytes reads in megabytes, a route that builds nothing reads in kilobytes.
#[test]
fn web_apm_counts_the_memory_of_each_request() {
    memory_is_counted(&[]);
}

/// The same under request isolation: the handler child that ran the request measures it.
#[test]
fn web_apm_counts_the_memory_of_isolated_requests() {
    memory_is_counted(&["--web-isolation=request"]);
}

/// Serves a heavy and a light route built with `flags` and checks their memory column.
fn memory_is_counted(flags: &[&str]) {
    let fixture = make_test_dir("web_apm_memory");
    let source = r#"<?php
$path = parse_url($_SERVER['REQUEST_URI'], PHP_URL_PATH);
if ($path === '/heavy') {
    $rows = [];
    for ($i = 0; $i < 20000; $i++) { $rows[] = str_repeat('x', 200) . $i; }
    echo count($rows);
} else {
    echo "light";
}
"#;
    let mut all = vec!["--with-monitoring"];
    all.extend_from_slice(flags);
    let binary = compile_web_with_flags(&fixture, source, "app", &all);
    let http = format!("127.0.0.1:{}", free_port());
    let probe = format!("127.0.0.1:{}", free_port());
    let server = spawn_apm_server(&binary, &http, &probe, true);
    for _ in 0..3 {
        assert!(http_get(&http, "/heavy").contains("20000"));
        assert!(http_get(&http, "/light").contains("light"));
    }
    let output = read_transactions(&binary, &probe);
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    assert!(stdout.contains("mem/req"), "{stdout}");
    let heavy = row(&stdout, "GET /heavy").join(" ");
    let light = row(&stdout, "GET /light").join(" ");
    assert!(heavy.contains(" MB "), "a few megabytes per heavy request: {stdout}");
    assert!(light.contains(" KB "), "kilobytes per light request: {stdout}");
    drop(server);
}

/// Symfony answers a request's exception itself, through `HttpKernel::handleThrowable`
/// (reduced here to what the hook relies on): the error of a request answered 500 is
/// counted, and a "not found" exception answered 404 is not.
#[test]
fn web_apm_counts_the_errors_symfony_answers_with_a_5xx() {
    let fixture = make_test_dir("web_apm_symfony_errors");
    let source = r#"<?php
namespace Symfony\Component\HttpKernel {
    class NotFound extends \RuntimeException {}
    class HttpKernel {
        public function handle(string $path): string {
            try {
                if ($path === '/missing') { throw new NotFound('no route'); }
                if ($path === '/fail') { throw new \DomainException('stock out'); }
                return "ok";
            } catch (\Throwable $e) {
                return $this->handleThrowable($e);
            }
        }
        private function handleThrowable(\Throwable $e): string {
            http_response_code($e instanceof NotFound ? 404 : 500);
            return "error page";
        }
    }
}
namespace {
    echo (new Symfony\Component\HttpKernel\HttpKernel())->handle((string) parse_url($_SERVER['REQUEST_URI'], PHP_URL_PATH));
}
"#;
    let binary = compile_web_with_flags(&fixture, source, "app", &["--with-monitoring"]);
    let http = format!("127.0.0.1:{}", free_port());
    let probe = format!("127.0.0.1:{}", free_port());
    let server = spawn_apm_server(&binary, &http, &probe, true);
    for _ in 0..2 {
        assert!(http_get(&http, "/fail").starts_with("HTTP/1.1 500"));
    }
    assert!(http_get(&http, "/missing").starts_with("HTTP/1.1 404"));
    let output = read_transactions(&binary, &probe);
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    assert!(stdout.contains("errors of failed transactions — 2 in 1 group"), "{stdout}");
    let line = stdout.lines().find(|line| line.contains("DomainException")).unwrap_or_else(|| panic!("{stdout}"));
    assert!(line.contains("app.php:8") && line.ends_with("(GET /fail)"), "{line}");
    assert!(!stdout.contains("NotFound "), "a 404 is no error of the service: {stdout}");
    drop(server);
}

/// A probe build carries its identity in its snapshots, and a collector with a store notes
/// the first time the service serves it as a deploy — once: a collector started again on
/// the same build marks nothing new.
#[test]
fn web_apm_collector_marks_a_new_build_once() {
    let fixture = make_test_dir("web_apm_deploy");
    let binary = compile_web_with_flags(&fixture, APP, "app", &["--with-monitoring"]);
    let http = format!("127.0.0.1:{}", free_port());
    let probe = format!("127.0.0.1:{}", free_port());
    let server = spawn_apm_server(&binary, &http, &probe, true);
    let key = format!("{}.key", binary.display());
    let store = fixture.join("store");
    let deploys = || std::fs::read_to_string(store.join("deploys.jsonl")).unwrap_or_default();
    for _ in 0..2 {
        let collector = Command::new(elephc_bin())
            .args(["monitor", &probe, "--apm", "--key", &key, "--resolution", "1", "--store"])
            .arg(&store)
            .spawn_group()
            .expect("failed to start the collector");
        std::thread::sleep(Duration::from_millis(1200));
        assert!(http_get(&http, "/users/7").starts_with("HTTP/1.1 200"));
        std::thread::sleep(Duration::from_millis(1500));
        drop(collector);
    }
    let text = deploys();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 1, "one deploy, however many collectors saw the build: {text}");
    let deploy: serde_json::Value = serde_json::from_str(lines[0]).expect("a deploy line");
    assert_eq!(deploy["service"], probe.as_str(), "{text}");
    let build = deploy["build"].as_str().unwrap_or_default();
    let seconds: u64 = build.split(' ').next().and_then(|s| s.parse().ok()).unwrap_or(0);
    assert!(seconds > 1_700_000_000, "the build's time leads its identity: {text}");
    drop(server);
}

/// A consumer outside HTTP: Messenger's worker, as the framework writes it
/// (reduced to what the hook relies on), and a job function named with
/// `--apm-transaction`. Nothing in it knows it is monitored.
const CONSUMER: &str = r#"<?php
namespace Symfony\Component\Messenger\Event {
    class WorkerMessageFailedEvent {
        public function __construct(public object $envelope, public string $receiverName, public \Throwable $error) {}
    }
}
namespace Symfony\Component\Messenger {
    class Envelope {
        public function __construct(private object $message) {}
        public function getMessage(): object { return $this->message; }
    }
    class Worker {
        public function run(array $envelopes): void {
            foreach ($envelopes as $envelope) {
                $this->handleMessage($envelope, 'async');
            }
        }
        private function handleMessage(Envelope $envelope, string $transportName): void {
            try {
                $envelope->getMessage()->handle();
            } catch (\Throwable $e) {
                new Event\WorkerMessageFailedEvent($envelope, $transportName, $e);
            }
        }
    }
}
namespace App\Message {
    class SendEmail {
        public function handle(): void {
            $pdo = new \PDO('sqlite::memory:');
            $pdo->query('SELECT 1')->fetchAll();
        }
    }
    class Charge {
        public function handle(): void { throw new \RuntimeException('declined'); }
    }
}
namespace {
    function import_batch(int $n): int {
        if ($n === 2) {
            throw new RuntimeException('bad batch');
        }
        return $n * 2;
    }
    use Symfony\Component\Messenger\{Envelope, Worker};
    use App\Message\{SendEmail, Charge};
    (new Worker())->run([new Envelope(new SendEmail()), new Envelope(new Charge()), new Envelope(new SendEmail())]);
    for ($i = 0; $i < 3; $i++) {
        try {
            import_batch($i);
        } catch (RuntimeException $e) {
        }
    }
    echo "done\n";
}
"#;

/// A short program built with `--apm-collector` sends what it counted as it
/// exits: each message is a transaction named after its class, failed when
/// the worker reported it, with its queries; each call of a named boundary
/// is one too, failed when an exception left it.
#[test]
fn apm_messages_and_boundaries_reach_a_listening_collector() {
    let fixture = make_test_dir("apm_consumer");
    let source = fixture.join("consumer.php");
    fs::write(&source, CONSUMER).unwrap();
    let address = format!("127.0.0.1:{}", free_port());
    let mut build = Command::new(elephc_bin());
    build
        .env("XDG_CACHE_HOME", fixture.join("cache-root"))
        .current_dir(&fixture)
        .args(["build", "--with-apm", "--apm-collector", &address, "--apm-transaction", "import_batch"])
        .arg(&source);
    let built = output_with_timeout(build, Duration::from_secs(600));
    assert!(built.status.success(), "{}", String::from_utf8_lossy(&built.stderr));
    let binary = fixture.join("consumer");
    let key = fixture.join("consumer.key");

    let store = fixture.join("store");
    let collector = Command::new(elephc_bin())
        .args(["monitor", "--apm", "--listen", &address, "--resolution", "1", "--key"])
        .arg(&key)
        .arg("--store")
        .arg(&store)
        .spawn_group()
        .expect("failed to start the collector");
    wait_until_ready(&address);

    let run = output_with_timeout(Command::new(&binary), Duration::from_secs(60));
    assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));
    assert_eq!(String::from_utf8_lossy(&run.stdout), "done\n");
    // The program's last counters arrive as it exits, and the collector stores them a
    // resolution later: wait for them rather than for a fixed time, which a loaded machine
    // outran (the collector was stopped before the push was in).
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    let stored = || {
        std::fs::read_dir(&store)
            .map(|entries| {
                entries.flatten().any(|entry| {
                    entry.file_name().to_string_lossy().starts_with("apm-")
                        // The run is counted at exit: its row comes with the last push.
                        && std::fs::read_to_string(entry.path()).is_ok_and(|text| text.contains("CLI consumer"))
                })
            })
            .unwrap_or(false)
    };
    while !stored() && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(200));
    }
    // SIGTERM: the collector writes what it holds before it exits.
    drop(collector);

    let mut command = Command::new(elephc_bin());
    command.args(["monitor", "--apm", "--store"]).arg(&store);
    let output = output_with_timeout(command, Duration::from_secs(30));
    let table = String::from_utf8_lossy(&output.stdout).to_string();
    assert!(output.status.success(), "{table}\n{}", String::from_utf8_lossy(&output.stderr));
    let email = row(&table, "MSG App\\Message\\SendEmail");
    assert_eq!(email[0], "2", "{table}");
    assert_eq!(email[email.len() - 2], "1.0", "one query per message: {table}");
    let charge = row(&table, "MSG App\\Message\\Charge");
    assert_eq!(charge[0], "1", "{table}");
    // The 5xx column, fourth from the end with the I/O columns: not any
    // cell that happens to read the same, such as the time share (review).
    assert_eq!(charge[charge.len() - 4], "100.0%", "the worker reported the failure: {table}");
    // The script's whole run is counted too, beside what it ran.
    let run = row(&table, "CLI consumer");
    assert_eq!(run[0], "1", "the run itself: {table}");
    let batches = row(&table, "TXN import_batch");
    assert_eq!(batches[0], "3", "{table}");
    assert_eq!(batches[batches.len() - 4], "33.3%", "one batch in three threw: {table}");
    // Each failure's error, from the store: the handler's exception the worker reported,
    // and the one that left the boundary.
    assert!(table.contains("errors of failed transactions — 2 in 2 groups"), "{table}");
    let error = |transaction: &str| {
        table
            .lines()
            .find(|line| line.ends_with(&format!("({transaction})")))
            .unwrap_or_else(|| panic!("no error of {transaction}: {table}"))
            .to_string()
    };
    assert!(error("MSG App\\Message\\Charge").contains("RuntimeException  at ") && error("MSG App\\Message\\Charge").contains("consumer.php:35"), "{table}");
    assert!(error("TXN import_batch").contains("consumer.php:41"), "{table}");
}
