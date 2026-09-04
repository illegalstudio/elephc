//! Purpose:
//! Integration tests for php's `file://` URL scheme: the wrapper `stream_get_wrappers()` has
//! always advertised, and which nothing honoured.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - MEASURED before the fix: twelve operations in a row — `file_get_contents`, `fopen`,
//!   `file_exists`, `filesize`, `is_file`, `file`, `copy`, `file_put_contents`, `is_dir`,
//!   `mkdir`, `unlink`, `rename` — every one answered `false` for a URL php reads without
//!   complaint, while `stream_get_wrappers()` listed `file` among the wrappers on offer.
//! - THE RULE: `file://` matches case-insensitively and needs exactly `://`. What follows is an
//!   authority then a path: the authority must be EMPTY or exactly `localhost`, also
//!   case-insensitively. `file://u.txt` is therefore a URL with host `u.txt` and no path, which
//!   php refuses — the case a naive `strip the first seven bytes` gets wrong while looking right
//!   everywhere else.
//! - NOT EVERY PATH BUILTIN takes it. php routes most through its plain-files WRAPPER, but
//!   `realpath()`, `readlink()`, `symlink()`'s link argument, `glob()`, `disk_free_space()` and
//!   `chdir()` call libc directly and never see the URL. Those were measured one by one, and
//!   pinning them here is what stops a later "make it uniform" from breaking parity.
//! - Every expectation was measured on `php -n` 8.5.6.

use crate::support::*;

/// Verifies that the ordinary path builtins read, write and stat through a `file://` URL.
///
/// One program covers the family on purpose: the URL is stripped in ONE place, and a test per
/// builtin would say the same thing a dozen times while missing the one that was never wired.
#[test]
fn the_file_url_scheme_reaches_every_wrapper_backed_builtin() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("u.txt", "content\n");
$abs = realpath("u.txt");
$dir = dirname($abs);

echo "get ", var_export(file_get_contents("file://$abs"), true), "\n";
$h = fopen("file://$abs", "rb");
echo "open ", var_export(is_resource($h), true), " read ", var_export(fread($h, 100), true), "\n";
$m = stream_get_meta_data($h);
echo "meta ", $m["wrapper_type"], " ", var_export($m["seekable"], true), "\n";
fclose($h);
echo "exists ", var_export(file_exists("file://$abs"), true);
echo " size ", var_export(filesize("file://$abs"), true);
echo " is_file ", var_export(is_file("file://$abs"), true);
echo " is_dir ", var_export(is_dir("file://$dir"), true), "\n";
echo "lines ", count(file("file://$abs")), "\n";
echo "copy ", var_export(copy("file://$abs", "file://$dir/b.txt"), true);
echo " kept ", var_export(file_get_contents("b.txt"), true), "\n";
echo "put ", var_export(file_put_contents("file://$dir/w.txt", "written"), true);
echo " back ", var_export(file_get_contents("w.txt"), true), "\n";
echo "mkdir ", var_export(mkdir("file://$dir/made"), true);
echo " rmdir ", var_export(rmdir("file://$dir/made"), true), "\n";
echo "rename ", var_export(rename("file://$dir/b.txt", "file://$dir/c.txt"), true);
echo " unlink ", var_export(unlink("file://$dir/c.txt"), true), "\n";
echo "scandir ", implode(",", array_slice(scandir("file://$dir"), 0, 2)), "\n";
echo "touch ", var_export(touch("file://$dir/t.txt"), true);
echo " chmod ", var_export(chmod("file://$dir/t.txt", 0644), true), "\n";
"#,
    );
    assert_eq!(
        out,
        "get 'content\n'\n\
         open true read 'content\n'\n\
         meta plainfile true\n\
         exists true size 8 is_file true is_dir true\n\
         lines 1\n\
         copy true kept 'content\n'\n\
         put 7 back 'written'\n\
         mkdir true rmdir true\n\
         rename true unlink true\n\
         scandir .,..\n\
         touch true chmod true\n"
    );
    let _ = std::fs::remove_dir_all(dir);
}

/// Verifies which URLs php accepts, and which it leaves as an odd filename.
///
/// `file://u.txt` is the case that matters: its authority is `u.txt` and its path is empty, so
/// php refuses it even though a relative file of that name sits right there. Dropping seven bytes
/// unconditionally would open it and look correct in every other test here.
#[test]
fn only_an_empty_or_localhost_authority_names_a_file() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("u.txt", "content\n");
$abs = realpath("u.txt");

function t(string $label, string $url): void {
    $r = @file_get_contents($url);
    echo $label, " ", $r === false ? "false" : "ok(" . strlen($r) . ")", "\n";
}

t("empty-host", "file://$abs");
t("localhost", "file://localhost$abs");
t("upper-scheme", "FILE://$abs");
t("mixed-scheme", "FiLe://$abs");
t("upper-host", "file://LOCALHOST$abs");
t("other-host", "file://example.com$abs");
t("relative", "file://u.txt");
t("no-path", "file://localhost");
t("single-slash", "file:/$abs");
t("no-slash", "file:$abs");
t("extra-slash", "file:///" . ltrim($abs, "/"));
t("many-slash", "file:////" . ltrim($abs, "/"));
"#,
    );
    assert_eq!(
        out,
        "empty-host ok(8)\n\
         localhost ok(8)\n\
         upper-scheme ok(8)\n\
         mixed-scheme ok(8)\n\
         upper-host ok(8)\n\
         other-host false\n\
         relative false\n\
         no-path false\n\
         single-slash false\n\
         no-slash false\n\
         extra-slash ok(8)\n\
         many-slash ok(8)\n"
    );
    let _ = std::fs::remove_dir_all(dir);
}

/// Verifies the builtins php does NOT route through the wrapper still see the URL as a filename.
///
/// These answer `false` in php, and each was measured rather than reasoned about: the split does
/// not follow "is it a path" but "does php call its plain-files wrapper or libc". Making them
/// uniform with the family above would look like a tidy-up and would break parity.
#[test]
fn the_libc_backed_builtins_do_not_read_the_url() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("u.txt", "content\n");
mkdir("sub");
$abs = realpath("u.txt");
$dir = dirname($abs);

echo "realpath ", var_export(@realpath("file://$abs"), true), "\n";
echo "glob ", var_export(@glob("file://$dir/*.txt"), true), "\n";
echo "disk_free ", var_export(@disk_free_space("file://$dir") > 0, true), "\n";
echo "chdir ", var_export(@chdir("file://$dir/sub"), true), "\n";
"#,
    );
    assert_eq!(
        out,
        "realpath false\n\
         glob array (\n)\n\
         disk_free false\n\
         chdir false\n"
    );
    let _ = std::fs::remove_dir_all(dir);
}
