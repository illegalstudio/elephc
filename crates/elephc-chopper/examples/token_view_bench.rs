//! Purpose:
//! Measures `TOKEN_PARSE` validation cost for the default token view and the text view.
//!
//! Called from:
//! - `scripts/bench/token_view.py`, or directly through `cargo run --release`.
//!
//! Key details:
//! - `ELEPHC_TOKEN_VIEW=text` selects the legacy text view for comparison.
//! - Each run prints nanoseconds per validation on stdout for the Python harness.

use std::hint::black_box;
use std::time::Instant;

use elephc_chopper::syntax::php_tokenizer::{tokenize_with_warnings, TOKEN_PARSE};

/// Builds one synthetic source fixture.
fn source(shape: &str) -> String {
    let mut source = String::from("<?php\n");
    match shape {
        "plain" => {
            for index in 0..10_000 {
                source.push_str(&format!("$x{index} = $a + $b * 2;\n"));
            }
        }
        "hooks" => {
            for index in 0..5_000 {
                source.push_str(&format!(
                    "#[Attr]\nclass Example{index} {{\n    public int $value = 0 {{\n        get => $this->value;\n        set => $value;\n    }}\n}}\n"
                ));
            }
        }
        "strings" => {
            for index in 0..10_000 {
                source.push_str(&format!("$s{index} = \"hello $x world\";\n"));
            }
        }
        "namespaced" => {
            for index in 0..10_000 {
                source.push_str(&format!(
                    "\\Vendor\\Package\\Service{index}::run(\\Vendor\\Package\\Value{index}::create());\n"
                ));
            }
        }
        "fallback" => {
            for index in 0..10_000 {
                source.push_str(&format!("$x{index} = $a + $b * 2;\n"));
            }
            source.push_str("__halt_compiler();\n");
        }
        _ => panic!("unknown shape: {shape}"),
    }
    source
}

/// Validates one fixture repeatedly and prints nanoseconds per call.
fn main() {
    let shape = std::env::args().nth(1).expect("shape");
    let rounds: usize = std::env::args()
        .nth(2)
        .and_then(|value| value.parse().ok())
        .unwrap_or(200);
    let source = source(&shape);
    for _ in 0..5 {
        let output = tokenize_with_warnings(source.as_bytes(), TOKEN_PARSE).expect("valid fixture");
        black_box(output.tokens.len());
    }
    let started = Instant::now();
    for _ in 0..rounds {
        let output = tokenize_with_warnings(source.as_bytes(), TOKEN_PARSE).expect("valid fixture");
        black_box(output.tokens.len());
    }
    let elapsed = started.elapsed();
    println!(
        "shape={shape} bytes={} rounds={rounds} ns_per_call={}",
        source.len(),
        elapsed.as_nanos() / rounds as u128
    );
}
