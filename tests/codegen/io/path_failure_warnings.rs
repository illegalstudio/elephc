//! Purpose:
//! Integration tests for the line php prints when a filesystem PATH OPERATION fails.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - MEASURED before the fix: a program of sixteen failing path calls drew SIXTEEN warnings from
//!   `php -n` 8.5.6 and NINE from elephc — and two of those nine named the wrong function.
//!   Seven builtins failed in complete silence: `unlink`, `rmdir`, `rename`, `mkdir`, `opendir`,
//!   `touch` and `chmod`. A script that checked the return value behaved the same either way; a
//!   script that read the log learned nothing at all.
//! - THE SHAPES ARE NOT ONE SHAPE, which is the point of testing them together:
//!   `unlink(path): reason`, `opendir(path): Failed to open directory: reason`,
//!   `rename(from,to): reason` — comma, no space — and `mkdir(): reason`, `chmod(): reason`,
//!   `touch(): Unable to create file PATH because reason`, whose parentheses stay EMPTY even
//!   though a path was passed.
//! - `readfile()` and `file()` read through `file_get_contents`, and left to itself that helper
//!   names ITSELF: a missing file was reported as `file_get_contents(x.txt)` under both names.
//! - The errno varies on purpose — `ENOENT`, `EEXIST`, `ENOTDIR` — because the reason text comes
//!   from the system and only the frame around it is elephc's to get right.
//! - Every expectation was measured on `php -n` 8.5.6.

use crate::support::*;

/// Verifies that each failing path builtin says WHY, in php's wording for that builtin.
#[test]
fn a_failing_path_call_says_why_the_way_php_does() {
    let out = compile_and_run_capture(
        r#"<?php
mkdir("exists");
file_put_contents("f.txt", "x");
mkdir("exists");
mkdir("/no/such/dir/deep");
rmdir("f.txt");
rmdir("exists/nope");
unlink("nope.txt");
rename("nope.txt", "other.txt");
touch("/no/such/dir/x.txt");
chmod("nope.txt", 0644);
opendir("f.txt");
opendir("nope");
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.diagnostics,
        "Warning: mkdir(): File exists\n\
         Warning: mkdir(): No such file or directory\n\
         Warning: rmdir(f.txt): Not a directory\n\
         Warning: rmdir(exists/nope): No such file or directory\n\
         Warning: unlink(nope.txt): No such file or directory\n\
         Warning: rename(nope.txt,other.txt): No such file or directory\n\
         Warning: touch(): Unable to create file /no/such/dir/x.txt because No such file or directory\n\
         Warning: chmod(): No such file or directory\n\
         Warning: opendir(f.txt): Failed to open directory: Not a directory\n\
         Warning: opendir(nope): Failed to open directory: No such file or directory\n",
        "each builtin warns in ITS wording, and in the order the program calls them"
    );
}

/// Verifies that a delegating reader names ITSELF, not the helper it reads through.
///
/// `readfile()` and `file()` both go through `file_get_contents`, and both reported that name.
#[test]
fn a_delegating_reader_names_itself_in_its_warning() {
    let out = compile_and_run_capture(
        r#"<?php
readfile("/no/such/dir/x.txt");
file("/no/such/dir/x.txt");
file_get_contents("/no/such/dir/x.txt");
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.diagnostics,
        "Warning: readfile(/no/such/dir/x.txt): Failed to open stream: No such file or directory\n\
         Warning: file(/no/such/dir/x.txt): Failed to open stream: No such file or directory\n\
         Warning: file_get_contents(/no/such/dir/x.txt): Failed to open stream: No such file or directory\n",
        "the function the USER called is the one php names"
    );
}

/// Verifies that `@` still silences every one of these lines.
///
/// A warning that ignores the suppression operator is worse than no warning: it appears in
/// output a program deliberately kept clean.
#[test]
fn the_suppression_operator_silences_them_all() {
    let out = compile_and_run_capture(
        r#"<?php
@mkdir("/no/such/dir/deep");
@rmdir("nope");
@unlink("nope.txt");
@rename("nope.txt", "other.txt");
@touch("/no/such/dir/x.txt");
@chmod("nope.txt", 0644);
@opendir("nope");
@readfile("nope.txt");
echo "quiet\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.diagnostics, "", "every one of these honours @");
    assert_eq!(out.stdout, "quiet\n");
}

/// Verifies each ownership builtin names ITSELF, and tells the two failures apart.
///
/// `chown()` and `chgrp()` are one syscall with the other principal set to `-1`, and `lchown()`
/// and `lchgrp()` likewise — but php names the CALLER, so a shared entry point would report
/// `chown()` for all four. A principal NAME that does not resolve is a second, differently worded
/// failure: no `errno` is involved and php quotes the name. elephc printed none of these lines,
/// and answered `false` in silence.
///
/// Every expectation measured on `php -n` 8.5.6.
#[test]
fn a_failing_ownership_call_names_itself_and_its_reason() {
    let out = compile_and_run_capture(
        r#"<?php
file_put_contents("f.txt", "x");
chown("nope.txt", 0);
chgrp("nope.txt", 0);
lchown("nope.txt", 0);
lchgrp("nope.txt", 0);
chown("f.txt", "nosuchuser");
chgrp("f.txt", "nosuchgroup");
lchown("f.txt", "nosuchuser");
lchgrp("f.txt", "nosuchgroup");
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.diagnostics,
        "Warning: chown(): No such file or directory\n\
         Warning: chgrp(): No such file or directory\n\
         Warning: lchown(): No such file or directory\n\
         Warning: lchgrp(): No such file or directory\n\
         Warning: chown(): Unable to find uid for nosuchuser\n\
         Warning: chgrp(): Unable to find gid for nosuchgroup\n\
         Warning: lchown(): Unable to find uid for nosuchuser\n\
         Warning: lchgrp(): Unable to find gid for nosuchgroup\n",
        "each ownership builtin warns in ITS name, and an unresolvable principal is its own line"
    );
}

/// Verifies a null principal draws php's deprecation BEFORE the call it still makes.
///
/// php 8.1 deprecated passing null to a non-nullable internal parameter rather than removing the
/// coercion, so both lines come out: the notice, then the ordinary warning for uid/gid 0. The
/// path is one that cannot exist, which keeps the second line the same on every machine.
#[test]
fn a_null_ownership_principal_deprecates_and_still_runs() {
    let out = compile_and_run_capture(
        r#"<?php
chown("nope.txt", null);
chgrp("nope.txt", null);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.diagnostics,
        "Deprecated: chown(): Passing null to parameter #2 ($user) of type string|int is \
         deprecated\n\
         Warning: chown(): No such file or directory\n\
         Deprecated: chgrp(): Passing null to parameter #2 ($group) of type string|int is \
         deprecated\n\
         Warning: chgrp(): No such file or directory\n",
        "the notice precedes the call php still makes"
    );
}
