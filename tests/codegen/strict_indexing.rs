//! Purpose:
//! Verifies public checked indexing through CLI, opaque eval and source entry points.
//!
//! Called from:
//! - Focused codegen integration tests.
//!
//! Key details:
//! - Public admission is validated together with source effects and heap ownership.

use crate::support::*;

const SOURCE: &str = include_str!("../fixtures/strict/indexed_reads.php");
const EXPECTED: &str = "PBH:NULL:11:4|c:a|SN|q:q:q:i:e|quiet|D|rkKfXF:2|WZ";

/// Public CLI source modes preserve checked reads, probes and independent PHP coercions.
#[test]
fn test_strict_indexing_public_cli_values_and_owners() {
    for optimization in ["--ir-opt=off", "--ir-opt=on"] {
        let output = StrictFixture::new(SOURCE).capture(&["--strict-indexing", "--php-only", optimization, "--heap-debug"]);
        let errors = String::from_utf8_lossy(&output.stderr);
        assert_eq!(String::from_utf8_lossy(&output.stdout), EXPECTED, "{errors}");
        assert!(errors.contains("HEAP DEBUG: leak summary: clean"), "{errors}");
        assert!(!errors.contains("Warning:"), "{errors}");
    }
    let output = StrictFixture::new("<?php $a=[1];try{$a[9];}catch(OutOfBoundsException $error){echo 'bad';}echo 'P';")
        .capture(&["--strict-indexing", "--no-strict-indexing", "--heap-debug"]);
    assert_eq!(String::from_utf8_lossy(&output.stdout), "P");
    let errors = String::from_utf8_lossy(&output.stderr);
    assert!(errors.contains("Undefined array key"), "{errors}");
    assert!(errors.contains("HEAP DEBUG: leak summary: clean"), "{errors}");
    let output = StrictFixture::new("<?php $a=[1];echo $a[0];").run(&["--strict",
        "--no-strict-mixed", "--no-strict-array-types"]);
    assert_eq!(output, "1");
}

/// Opaque runtime source uses the checked callback and returns errors through Rust's throwable channel.
#[test]
fn test_strict_indexing_public_opaque_eval_values_and_owners() {
    let fragment = format!("{}{}",
        include_str!("../fixtures/strict/indexed_eval_reads.php").strip_prefix("<?php").unwrap(),
        include_str!("../fixtures/strict/indexed_eval_edge_reads.php").strip_prefix("<?php declare(strict_types=0);").unwrap());
    let fragment = fragment.replace('\\', "\\\\").replace('\'', "\\'");
    let source = format!("<?php $fragment=$argc>0?'{fragment}':'';eval($fragment);");
    let output = StrictFixture::new(&source).capture(&["--strict-indexing", "--heap-debug"]);
    let errors = String::from_utf8_lossy(&output.stderr);
    assert_eq!(String::from_utf8_lossy(&output.stdout), format!("{EXPECTED}OutOfBoundsException|SR:1|7:8|9|CB"), "{errors}");
    assert!(errors.contains("HEAP DEBUG: leak summary: clean"), "{errors}");
    assert!(!errors.contains("Warning:"), "{errors}");
}

/// Literal missing reads survive folding while intentional creation, unset and coalescing remain valid.
#[test]
fn test_strict_indexing_public_literals_aliases_and_probes() {
    let source = include_str!("../fixtures/strict/indexed_edge_reads.php");
    for optimization in ["--ir-opt=off", "--ir-opt=on"] {
        let output = StrictFixture::new(source).capture(&["--strict-indexing", optimization, "--heap-debug"]);
        let errors = String::from_utf8_lossy(&output.stderr);
        assert_eq!(String::from_utf8_lossy(&output.stdout), "OutOfBoundsException|SR:1|7:8|9|CB", "{errors}");
        assert!(errors.contains("HEAP DEBUG: leak summary: clean"), "{errors}");
        assert!(!errors.contains("Warning:"), "{errors}");
    }
}

/// PHP-only callers retain checked reads inside tagless LFC includes and the runnable example.
#[test]
fn test_strict_indexing_public_lfc_and_example() {
    let fixture = StrictFixture::new("<?php require 'stock.lfc';");
    fixture.file("stock.lfc", "int $index=8;$stock=[1];try{echo $stock[$index];}catch(OutOfBoundsException $error){echo 'L';}");
    let output = fixture.capture(&["--strict-indexing", "--php-only", "--heap-debug"]);
    assert_eq!(String::from_utf8_lossy(&output.stdout), "L");
    assert!(String::from_utf8_lossy(&output.stderr).contains("HEAP DEBUG: leak summary: clean"));
    let output = StrictFixture::new(include_str!("../../examples/strict-indexing/main.php"))
        .capture(&["--strict-indexing", "--heap-debug"]);
    assert_eq!(String::from_utf8_lossy(&output.stdout),
        "pens: 12\npaper: not counted\nink: unknown item\nOptional item: not listed\n");
    assert!(String::from_utf8_lossy(&output.stderr).contains("HEAP DEBUG: leak summary: clean"), "{}", String::from_utf8_lossy(&output.stderr));
}

/// Object hooks stay authoritative and nullsafe probes remain quiet under the selected profile.
#[test]
fn test_strict_indexing_public_object_protocols_and_probes() {
    let direct = include_str!("../fixtures/strict/indexed_object_reads.php");
    let quoted = direct.strip_prefix("<?php").unwrap().replace('\\', "\\\\").replace('\'', "\\'");
    let opaque = format!("<?php $fragment=$argc>0?'{quoted}':'';eval($fragment);");
    for source in [direct, opaque.as_str()] {
        let output = StrictFixture::new(source).capture(&["--strict-indexing", "--heap-debug"]);
        let errors = String::from_utf8_lossy(&output.stderr);
        assert_eq!(String::from_utf8_lossy(&output.stdout), "provided:1:0|provided:2:0|qq", "{errors}");
        assert!(errors.contains("HEAP DEBUG: leak summary: clean"), "{errors}");
        assert!(!errors.contains("Warning:"), "{errors}");
    }
}
