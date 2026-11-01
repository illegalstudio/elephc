//! Purpose:
//! End-to-end AOT tests for PHP's password hashing API (`password_hash`, `password_verify`,
//! `password_needs_rehash`, `password_get_info`) and its constants.
//!
//! Called from:
//! - `cargo test --test codegen_tests password` through the Rust test harness.
//!
//! Key details:
//! - The stored hash was produced by php (`password_hash('kitten', PASSWORD_BCRYPT,
//!   ['cost' => 13])`, the Symfony demo's fixtures), so verifying it proves a compiled program
//!   accepts what php wrote. The password derives from `$argc` so nothing folds.
//! - Expected output is php 8.5's, including the two `ValueError` messages.

use super::*;

/// bcrypt hashes php produced verify, fresh hashes round-trip in php's `$2y$` spelling, and the
/// info/rehash/validation surface answers as php does. Before this the functions did not exist
/// and the Symfony demo's login could never succeed.
#[test]
fn test_password_hash_and_verify_match_php() {
    let out = compile_and_run(
        r#"<?php
$pw = $argc > 0 ? "kitten" : "";
$stored = '$2y$13$3EJxNVreeSXjCJooqMEDsu8jjTtCWVQWKs5QaCRZ2VcrXK1ATq0E.';
var_dump(password_verify($pw, $stored), password_verify($pw . "!", $stored), password_verify($pw, "nope"));
$h = password_hash($pw, PASSWORD_BCRYPT, ["cost" => 4]);
var_dump(strlen($h), substr($h, 0, 7), password_verify($pw, $h), password_get_info($h)["options"]["cost"], password_get_info("x")["algoName"]);
var_dump(password_needs_rehash($stored, PASSWORD_DEFAULT, ["cost" => 13]), password_needs_rehash($stored, PASSWORD_BCRYPT), PASSWORD_DEFAULT, PASSWORD_BCRYPT_DEFAULT_COST);
try { password_hash($pw, PASSWORD_BCRYPT, ["cost" => 3]); } catch (\ValueError $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
try { password_hash($pw, "zz"); } catch (\ValueError $e) { echo $e->getMessage(), "\n"; }
"#,
    );
    assert_eq!(
        out,
        "bool(true)\nbool(false)\nbool(false)\nint(60)\nstring(7) \"$2y$04$\"\nbool(true)\nint(4)\n\
         string(7) \"unknown\"\nbool(false)\nbool(true)\nstring(2) \"2y\"\nint(12)\n\
         ValueError: Invalid bcrypt cost parameter specified: 3\n\
         password_hash(): Argument #2 ($algo) must be a valid password hashing algorithm\n"
    );
}
