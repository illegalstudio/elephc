//! Purpose:
//! Exercises store-free enum interface result slots during caught argument-count errors.
//!
//! Called from:
//! - The codegen types integration suite.
//!
//! Key details:
//! - String results need an initialized owner slot even if binding rejects every candidate.
//! - Repeated failures must preserve live caller strings and the existing enum singleton baseline.

use crate::support::*;

/// Extracts a retained-heap metric without accepting extra owners from rejected calls.
fn retained(stderr: &str, metric: &str) -> usize {
    stderr.lines().next().unwrap().split_whitespace()
        .find_map(|field| field.strip_prefix(metric)).unwrap().parse().unwrap()
}

/// Heap-return failures are caught without corrupting a live caller allocation or retaining values.
#[test]
fn test_enum_result_owner_review_rejected_string_result() {
    for iterations in [1, 24] {
        let run = |body: &str| compile_and_run_with_heap_debug(&format!(r#"<?php
interface I {{ public function f(int $a, int $b): int; }}
enum G implements I {{ case A; public function f(int $a = 1, int $b): int {{ return $a + $b; }} }}
interface J {{ public function f(int $a, int $b): string; }}
enum H implements J {{ case A; public function f(int $a = 1, int $b): string {{ return 'ok'; }} }}
function good(): int {{ return 3; }}
function missing_int(I $value): int {{ return $value->f(...[good()]); }}
function missing_string(J $value): string {{ return $value->f(...[good()]); }}
$live = str_repeat('owned', 16);
for ($i = 0; $i < {iterations}; $i++) {{ {body} }}
echo '|', $live;
unset($live);
"#));
        let baseline = run("G::A->f(1, 2); H::A->f(1, 2);");
        let rejected = run("try { missing_int(G::A); } catch (ArgumentCountError $error) { echo 'I'; unset($error); } try { missing_string(H::A); } catch (ArgumentCountError $error) { echo 'S'; unset($error); }");
        assert!(baseline.success && rejected.success, "{}\n{}", baseline.stderr, rejected.stderr);
        assert_eq!(baseline.stdout, format!("|{}", "owned".repeat(16)));
        assert_eq!(rejected.stdout, format!("{}|{}", "IS".repeat(iterations), "owned".repeat(16)));
        for metric in ["live_blocks=", "live_bytes="] {
            assert_eq!(retained(&rejected.stderr, metric), retained(&baseline.stderr, metric),
                "{metric}: baseline {}\nrejected {}", baseline.stderr, rejected.stderr);
        }
    }
}

/// Rejected string-result staging is also safe without EIR optimization.
#[test]
fn test_enum_result_owner_review_rejected_string_result_no_ir_opt() {
    without_ir_opt(test_enum_result_owner_review_rejected_string_result);
}
