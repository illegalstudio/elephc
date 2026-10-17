//! Purpose:
//! Regression tests for deferred JSON decode error locations.
//!
//! Called from:
//! - `cargo test --test codegen_tests` through the JSON codegen test module.
//!
//! Key details:
//! - Valid decodes must leave `_json_error_scan_count` and `_json_error_scan_bytes` at zero.
//! - One recorded decode error performs one source scan, bounded by the original source length.
//! - PHP 8.6 oracles compare full coordinates; older versions compare base messages.
//! - ELEPHC_JSON_PHP_BINARY can select a PHP CLI wrapper for the opt-in oracle.
//! - Native expectations always run; the live PHP oracle requires `ELEPHC_PHP_CHECK`.

use super::*;

/// Prints the process-wide location-scan counters before the test binary exits.
///
/// The counters are BSS symbols, not PHP builtins. The harness runs after user
/// code and writes `scan_count` and `scan_bytes` as decimal lines.
/// ARM64 write syscalls are selected explicitly for Darwin or Linux.
pub(super) fn scan_counter_harness() -> String {
    let asm = match target().arch {
        Arch::AArch64 => {
            r#"    adrp x0, _json_error_scan_count@PAGE
    add x0, x0, _json_error_scan_count@PAGEOFF
    ldr x0, [x0]
    bl __rt_itoa
    mov x0, #1
    mov x16, #4
    svc #0x80
    sub sp, sp, #16
    mov w9, #10
    strb w9, [sp]
    mov x1, sp
    mov x2, #1
    mov x0, #1
    mov x16, #4
    svc #0x80
    adrp x0, _json_error_scan_bytes@PAGE
    add x0, x0, _json_error_scan_bytes@PAGEOFF
    ldr x0, [x0]
    bl __rt_itoa
    mov x0, #1
    mov x16, #4
    svc #0x80
    mov w9, #10
    strb w9, [sp]
    mov x1, sp
    mov x2, #1
    mov x0, #1
    mov x16, #4
    svc #0x80
    add sp, sp, #16"#
        }
        Arch::X86_64 => {
            r#"    sub rsp, 8
    mov rax, QWORD PTR [rip + _json_error_scan_count]
    call __rt_itoa
    mov rsi, rax
    mov edi, 1
    mov eax, 1
    syscall
    push 10
    mov rsi, rsp
    mov edx, 1
    mov edi, 1
    mov eax, 1
    syscall
    pop rax
    mov rax, QWORD PTR [rip + _json_error_scan_bytes]
    call __rt_itoa
    mov rsi, rax
    mov edi, 1
    mov eax, 1
    syscall
    push 10
    mov rsi, rsp
    mov edx, 1
    mov edi, 1
    mov eax, 1
    syscall
    pop rax
    add rsp, 8"#
        }
    };
    if target().arch == Arch::AArch64 && target().platform == Platform::Linux {
        asm.replace("mov x16, #4", "mov x8, #64").replace("svc #0x80", "svc #0")
    } else {
        asm.to_string()
    }
}

/// Runs `source` on the default heap and returns user stdout plus the scan counters.
fn decode_with_scan_counts(source: &str) -> (String, u64, u64) {
    decode_with_scan_counts_heap(source, 8_388_608)
}

/// Runs `source` with an explicit heap and returns user stdout plus the scan counters.
fn decode_with_scan_counts_heap(source: &str, heap_size: usize) -> (String, u64, u64) {
    let out = compile_harness_and_run(source, heap_size, &scan_counter_harness());
    let mut lines: Vec<&str> = out.lines().collect();
    assert!(
        lines.len() >= 2,
        "scan harness did not append two counter lines: {out:?}"
    );
    let bytes: u64 = lines
        .pop()
        .unwrap()
        .parse()
        .unwrap_or_else(|_| panic!("scan byte counter was not an integer: {out:?}"));
    let scans: u64 = lines
        .pop()
        .unwrap()
        .parse()
        .unwrap_or_else(|_| panic!("scan count was not an integer: {out:?}"));
    (lines.join("\n"), scans, bytes)
}

/// Checks scan counters with the EIR optimizer on and off.
fn assert_scan_counts(source: &str, scans: u64, max_bytes: u64) {
    for ir_opt_off in [false, true] {
        let (body, got_scans, got_bytes) = if ir_opt_off {
            without_ir_opt(|| decode_with_scan_counts(source))
        } else {
            decode_with_scan_counts(source)
        };
        assert_eq!(
            got_scans, scans,
            "ir-opt off={ir_opt_off} body={body:?} scans={got_scans}"
        );
        assert!(
            got_bytes <= max_bytes,
            "ir-opt off={ir_opt_off} scanned {got_bytes} bytes, bound {max_bytes}, body={body:?}"
        );
        if scans == 0 {
            assert_eq!(got_bytes, 0, "valid decode scanned {got_bytes} bytes");
        }
    }
}

/// Executes the explicitly requested PHP oracle with `php -n -r` and returns stdout.
pub(super) fn php_cli(body: &str) -> String {
    let output = std::process::Command::new(std::env::var_os("ELEPHC_JSON_PHP_BINARY").unwrap_or_else(|| "php".into()))
        .arg("-n")
        .arg("-r")
        .arg(body)
        .output()
        .expect("php CLI");
    assert!(
        output.status.success(),
        "php failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("php stdout is utf-8")
}

/// Reads the selected PHP oracle version once per test process.
pub(super) fn php_json_version() -> u32 {
    static VERSION: std::sync::OnceLock<u32> = std::sync::OnceLock::new();
    *VERSION.get_or_init(|| {
        php_cli("echo PHP_VERSION_ID;").trim().parse::<u32>().expect("PHP version id")
    })
}

/// Reports whether the selected PHP oracle exposes JSON coordinate suffixes.
pub(super) fn php_json_error_has_locations() -> bool { php_json_version() >= 80600 }

/// Removes the location suffix only when comparing base messages to older PHP versions.
pub(super) fn strip_location_suffix(message: &str) -> String {
    match message.find(" near location ") {
        Some(index) => message[..index].to_string(),
        None => message.to_string(),
    }
}

/// Valid scalars, empty containers, nesting, and whitespace perform no diagnostic scan.
#[test]
fn test_json_decode_valid_inputs_do_not_scan_source() {
    let source = r#"<?php
$values = ["0", "42", "true", "false", "null", "\"hi\"", "1.5", "[]", "{}", "[[]]", "{\"a\":[]}", " \n\t42\r\n", "[\r\n]", "\"a\\nb\""];
foreach ($values as $json) {
    json_decode($json);
    if (json_last_error() != 0) { echo "fail:$json\n"; }
    json_validate($json);
    if (json_last_error() != 0) { echo "validate-fail:$json\n"; }
}
$rows = [];
for ($i = 0; $i < 8; $i++) { $rows[] = range(0, 7); }
json_decode(json_encode($rows));
echo json_last_error(), "\n";
"#;
    assert_scan_counts(source, 0, 0);
}

/// One syntax, depth, UTF-8, UTF-16, or control-character failure scans the source once.
#[test]
fn test_json_decode_one_error_scans_source_once() {
    let cases: &[(&str, &str, u64)] = &[
        ("x", "Syntax error near location 1:1", 0),
        ("[", "Syntax error near location 1:2", 1),
        ("[{", "Syntax error near location 1:3", 2),
        ("[1,]", "Syntax error near location 1:4", 3),
        ("[1,\n 2,]", "Syntax error near location 2:4", 7),
        ("[1,\r\n2,]", "Syntax error near location 2:3", 7),
        ("\"a\\uD83D\"", "Single unpaired UTF-16 surrogate in unicode escape near location 1:1", 0),
    ];
    for (json, message, max_bytes) in cases {
        let source = format!(
            "<?php json_decode({json:?}); echo json_last_error_msg(), \"\\n\";"
        );
        let (body, scans, bytes) = decode_with_scan_counts(&source);
        assert_eq!(body, *message, "json={json:?}");
        assert_eq!(scans, 1, "json={json:?}");
        assert!(bytes <= *max_bytes, "json={json:?} bytes={bytes}");
        let (body_off, scans_off, bytes_off) = without_ir_opt(|| decode_with_scan_counts(&source));
        assert_eq!(body_off, *message, "ir-opt off json={json:?}");
        assert_eq!(scans_off, 1, "ir-opt off json={json:?}");
        assert!(bytes_off <= *max_bytes, "ir-opt off json={json:?} bytes={bytes_off}");
    }
}

/// Empty decode and validate failures do not scan or retain previous coordinates.
#[test]
fn test_json_empty_sources_do_not_scan() {
    assert_scan_counts(r#"<?php json_decode(""); json_validate("");"#, 0, 0);
}

/// A document with many containers and one trailing syntax error still scans once.
#[test]
fn test_json_decode_late_error_does_not_rescan_each_container() {
    let source = r#"<?php
$rows = [];
for ($i = 0; $i < 30; $i++) { $rows[] = range(0, 9); }
$json = json_encode($rows);
$bad = substr($json, 0, -1);
json_decode($bad);
echo strlen($bad), "\n", json_last_error(), "\n";
"#;
    let (body, scans, bytes) = decode_with_scan_counts(source);
    let mut lines = body.lines();
    let len: u64 = lines.next().unwrap().parse().unwrap();
    assert_eq!(lines.next(), Some("4"));
    assert_eq!(scans, 1);
    assert!(bytes <= len, "scanned {bytes} of {len}");
    assert!(bytes > 0);
    let (_, scans_off, bytes_off) = without_ir_opt(|| decode_with_scan_counts(source));
    assert_eq!(scans_off, 1);
    assert!(bytes_off <= len);
}

/// Writes the compact 800 by 200 benchmark document used by the scan-counter test.
fn write_benchmark_document(path: &str, corrupt_tail: bool) {
    let row = format!("[{}]", (0..200).map(|n| n.to_string()).collect::<Vec<_>>().join(","));
    let mut json = String::from("[");
    for index in 0..800 {
        if index > 0 {
            json.push(',');
        }
        json.push_str(&row);
    }
    json.push(']');
    if corrupt_tail {
        json.push('x');
    }
    std::fs::write(path, json).unwrap();
}

/// Owns the unique benchmark fixture directory for one test invocation.
struct JsonLocationFixtures(std::path::PathBuf);

impl Drop for JsonLocationFixtures {
    /// Removes fixture files on success and when a test assertion unwinds.
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The 800 by 200 document from the location benchmark performs no diagnostic scan.
///
/// The document is loaded from a file. `json_encode()` of this shape is not used:
/// on this tree it rewrites a comma near byte 70704 once the output passes about
/// 70 KiB, which is outside this change.
#[test]
fn test_json_decode_benchmark_document_scans_only_on_error() {
    let fixtures = JsonLocationFixtures(make_cli_test_dir("elephc_json_location"));
    let valid_path = fixtures.0.join("valid.json").to_string_lossy().into_owned();
    let invalid_path = fixtures.0.join("invalid.json").to_string_lossy().into_owned();
    write_benchmark_document(&valid_path, false);
    write_benchmark_document(&invalid_path, true);
    let valid = format!(r#"<?php
$json = file_get_contents({valid_path:?});
$decoded = json_decode($json, true);
echo strlen($json), "\n", json_last_error(), "\n", count($decoded), "\n";
"#);
    let (body, scans, bytes) = decode_with_scan_counts_heap(&valid, 67_108_864);
    let mut lines = body.lines();
    let len: u64 = lines.next().unwrap().parse().unwrap();
    assert_eq!(len, 553_601, "compact 800x200 document size");
    assert_eq!(lines.next(), Some("0"));
    assert_eq!(lines.next(), Some("800"));
    assert_eq!(scans, 0);
    assert_eq!(bytes, 0);
    let (_, scans_off, bytes_off) = without_ir_opt(|| decode_with_scan_counts_heap(&valid, 67_108_864));
    assert_eq!(scans_off, 0);
    assert_eq!(bytes_off, 0);

    let invalid = format!(r#"<?php
$json = file_get_contents({invalid_path:?});
json_decode($json, true);
echo strlen($json), "\n", json_last_error(), "\n", json_last_error_msg(), "\n";
"#);
    let (body, scans, bytes) = decode_with_scan_counts_heap(&invalid, 67_108_864);
    let mut lines = body.lines();
    let len: u64 = lines.next().unwrap().parse().unwrap();
    assert_eq!(len, 553_602);
    assert_eq!(lines.next(), Some("4"));
    assert_eq!(
        lines.next(),
        Some("Syntax error near location 1:553602")
    );
    assert_eq!(scans, 1);
    assert_eq!(bytes, 553_601);
    let (_, scans_off, bytes_off) =
        without_ir_opt(|| decode_with_scan_counts_heap(&invalid, 67_108_864));
    assert_eq!(scans_off, 1);
    assert_eq!(bytes_off, 553_601);
}

/// An error, a success, and another error scan only the two failures.
#[test]
fn test_json_decode_error_success_error_scans_twice() {
    let source = r#"<?php
json_decode("[{");
json_decode("[1,2,3]");
json_decode("not json");
echo json_last_error_msg(), "\n";
"#;
    let (body, scans, bytes) = decode_with_scan_counts(source);
    assert_eq!(body, "Syntax error near location 1:1");
    assert_eq!(scans, 2);
    assert_eq!(bytes, 2);
    let (body_off, scans_off, _) = without_ir_opt(|| decode_with_scan_counts(source));
    assert_eq!(body_off, body);
    assert_eq!(scans_off, 2);
}

/// Public json_validate scans its own nonempty source once on failure.
#[test]
fn test_json_validate_publishes_its_own_location_and_scan() {
    let source = r#"<?php
json_decode("[{");
json_validate("{");
echo json_last_error_msg(), "\n";
"#;
    let (body, scans, bytes) = decode_with_scan_counts(source);
    assert_eq!(body, "Syntax error near location 1:2");
    assert_eq!(scans, 2, "decode and validate each scan their own failure");
    assert!(bytes <= 3);
}

/// Replacing the source string after a failure leaves the numeric location readable.
#[test]
fn test_json_last_error_msg_after_source_is_replaced() {
    let source = r#"<?php
function bad() { return "[{"; }
json_decode(bad());
$replaced = str_repeat("Z", 64);
echo json_last_error_msg(), "\n", strlen($replaced);
"#;
    assert_eq!(
        compile_and_run(source),
        "Syntax error near location 1:3\n64"
    );
    assert_eq!(
        without_ir_opt(|| compile_and_run(source)),
        "Syntax error near location 1:3\n64"
    );
}

/// A `jsonSerialize()` decode of a temporary string does not leave a dangling source.
#[test]
fn test_json_encode_jsonserializable_inner_decode_keeps_location_text() {
    let source = r#"<?php
class Temp implements JsonSerializable {
    public function jsonSerialize(): mixed {
        $bad = "[{";
        json_decode($bad);
        $bad = str_repeat("Z", 64);
        return json_last_error_msg();
    }
}
echo json_encode(new Temp());
"#;
    let expected = "\"Syntax error near location 1:3\"";
    assert_eq!(compile_and_run(source), expected);
    assert_eq!(without_ir_opt(|| compile_and_run(source)), expected);
}

/// A thrown decode inside `jsonSerialize()` formats the location before the source is replaced.
#[test]
fn test_json_encode_jsonserializable_thrown_decode_keeps_location_text() {
    let source = r#"<?php
class Temp implements JsonSerializable {
    public function jsonSerialize(): mixed {
        $bad = "[{";
        try {
            json_decode($bad, null, 512, JSON_THROW_ON_ERROR);
            return "no-throw";
        } catch (JsonException $e) {
            $bad = str_repeat("Z", 64);
            return $e->getMessage();
        }
    }
}
echo json_encode(new Temp());
"#;
    let expected = "\"Syntax error near location 1:3\"";
    assert_eq!(compile_and_run(source), expected);
    assert_eq!(without_ir_opt(|| compile_and_run(source)), expected);
}

/// Depth at the limit reports the opening container; one step looser succeeds with no scan.
#[test]
fn test_json_decode_depth_limit_locations_and_scans() {
    let fail_empty = r#"<?php
json_decode("[]", true, 1);
echo json_last_error_msg(), "\n";
"#;
    let (body, scans, bytes) = decode_with_scan_counts(fail_empty);
    assert_eq!(body, "Maximum stack depth exceeded near location 1:1");
    assert_eq!(scans, 1);
    assert_eq!(bytes, 0);

    let fail_nested = r#"<?php
json_decode("[[]]", true, 2);
echo json_last_error_msg(), "\n";
"#;
    let (body, scans, bytes) = decode_with_scan_counts(fail_nested);
    assert_eq!(body, "Maximum stack depth exceeded near location 1:2");
    assert_eq!(scans, 1);
    assert!(bytes <= 4);

    assert_scan_counts(
        r#"<?php
json_decode("[]", true, 2);
json_decode("[[]]", true, 3);
json_decode("{}", false, 2);
echo json_last_error(), "\n";
"#,
        0,
        0,
    );
}

/// Control characters and UTF-8 failures use token starts and Unicode character columns.
#[test]
fn test_json_decode_unicode_columns_for_utf8_and_controls() {
    let source = r#"<?php
json_decode("\"a" . chr(0x0A) . "b\"");
echo json_last_error(), "|", json_last_error_msg(), "\n";
json_decode("\"a" . chr(0x80) . "b\"");
echo json_last_error(), "|", json_last_error_msg(), "\n";
json_decode("[\"é\",]");
echo json_last_error(), "|", json_last_error_msg(), "\n";
"#;
    let expected = "\
3|Control character error, possibly incorrectly encoded near location 1:1
5|Malformed UTF-8 characters, possibly incorrectly encoded near location 1:1
4|Syntax error near location 1:6
";
    assert_eq!(compile_and_run(source), expected);
    assert_eq!(without_ir_opt(|| compile_and_run(source)), expected);
    let (_, scans, bytes) = decode_with_scan_counts(source);
    assert_eq!(scans, 3);
    assert!(bytes <= 4 + 4 + 7);
}

/// GC counters still print after a valid decode and a syntax error.
///
/// Process exit does not free every live value, so allocs and frees are not
/// equal here. The benchmark compares these counters with the baseline compiler.
#[test]
fn test_json_decode_location_change_still_reports_gc_stats() {
    for source in [
        r#"<?php json_decode("[1,2,3]");"#,
        r#"<?php json_decode("[{");"#,
    ] {
        let output = compile_and_run_with_gc_stats(source);
        assert!(output.success, "{}", output.stderr);
        let (allocs, frees) = parse_gc_stats(&output.stderr);
        assert!(allocs >= frees, "{source}: allocs={allocs} frees={frees}");
    }
}

/// Checks PHP-equivalent errors in both optimizer modes, with an opt-in live PHP oracle.
#[test]
fn test_json_decode_error_base_messages_match_php() {
    let body = r#"
$cases = ["", "x", "[{", "[1,]", "[1,\n 2,]", "[1,\r\n2,]", "\"\\uD83D\"", "[]", "{}"];
foreach ($cases as $json) {
    json_decode($json, true, 1);
    echo json_last_error(), "|", json_last_error_msg(), "\n";
}
try {
    json_decode("[{", null, 512, JSON_THROW_ON_ERROR);
    echo "no-throw\n";
} catch (JsonException $e) {
    echo get_class($e), "|", $e->getCode(), "|", $e->getMessage(), "\n";
}
json_validate("{");
echo json_last_error(), "|", json_last_error_msg(), "\n";
"#;
    let expected = "\
4|Syntax error
4|Syntax error
1|Maximum stack depth exceeded
1|Maximum stack depth exceeded
1|Maximum stack depth exceeded
1|Maximum stack depth exceeded
10|Single unpaired UTF-16 surrogate in unicode escape
1|Maximum stack depth exceeded
1|Maximum stack depth exceeded
JsonException|4|Syntax error
4|Syntax error
";
    let php_out = std::env::var_os("ELEPHC_PHP_CHECK").map(|_| php_cli(body));
    if let Some(oracle) = &php_out {
        let base = oracle.lines().map(strip_location_suffix).collect::<Vec<_>>().join("\n");
        assert_eq!(base, expected.trim_end(), "PHP base error reference changed");
    }
    let expected_lines: Vec<&str> = expected.lines().collect();
    let source = format!("<?php\n{body}");
    for ir_opt_off in [false, true] {
        let elephc_out = if ir_opt_off {
            without_ir_opt(|| compile_and_run(&source))
        } else {
            compile_and_run(&source)
        };
        let elephc_base: Vec<String> = elephc_out.lines().map(strip_location_suffix).collect();
        assert_eq!(elephc_base, expected_lines, "ir-opt off={ir_opt_off}: {elephc_out}");
        if let Some(oracle) = &php_out {
            let php_lines: Vec<&str> = oracle.lines().collect();
            let native = if php_json_error_has_locations() {
                elephc_out.lines().map(str::to_owned).collect::<Vec<_>>()
            } else {
                elephc_base.clone()
            };
            assert_eq!(native, php_lines, "PHP mismatch with ir-opt off={ir_opt_off}");
        }
    }
}
