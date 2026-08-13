//! Purpose:
//! Task 11 end-to-end fixtures: `CURLFile`/`CURLStringFile` uploads through
//! `CURLOPT_POSTFIELDS`, driven against the `/multipart` route of the loopback HTTP
//! fixture so a real `multipart/form-data` request is proven on the wire, not just a
//! `curl_setopt()` return value.
//!
//! Called from:
//! - `cargo test --test codegen_tests curl` through Rust's test harness.
//!
//! Key details:
//! - Every fixture writes its own real temporary file with `std::fs::write` before
//!   compiling the PHP source that references it, mirroring
//!   `tests/codegen/io/stat_ext.rs`'s pattern — the PHP program under test never has to
//!   create the file itself, which keeps each fixture's PHP source focused on the curl
//!   surface being tested.
//! - `/multipart`'s summary format (`content-type=`, `parts=N`,
//!   `part[N].name`/`.filename`/`.type`/`.size`/`.body`) is `http_fixture.rs`'s own; see
//!   that file for the parser.

use super::http_fixture::LocalHttpServer;
use crate::support::*;

/// Returns a fresh path under the OS temp directory, unique enough for parallel test
/// threads: process id, thread id, and a monotonic counter.
fn unique_temp_path(label: &str) -> std::path::PathBuf {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    std::env::temp_dir().join(format!(
        "elephc_curl_mime_{}_{:?}_{n}_{label}",
        std::process::id(),
        std::thread::current().id(),
    ))
}

/// STEP 1 (the brief's own fixture): a `CURLFile` with an explicit mime type and posted
/// filename, posted as `['f' => $cfile]` with `CURLOPT_RETURNTRANSFER`, produces a real
/// `multipart/form-data` request whose one part carries field name `f`, filename
/// `hello.txt`, content-type `text/plain`, and body bytes matching the file this test
/// wrote to disk.
#[test]
fn curlfile_uploads_as_a_real_multipart_part() {
    if skip_without_curl_native("curlfile_uploads_as_a_real_multipart_part") {
        return;
    }
    let path = unique_temp_path("step1.txt");
    std::fs::write(&path, b"hello from a real file").expect("write fixture file");
    let path_str = path.to_string_lossy().into_owned();

    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/multipart");
    let out = compile_and_run(&format!(
        r#"<?php
        $ch = curl_init("{url}");
        $cfile = new CURLFile("{path_str}", 'text/plain', 'hello.txt');
        curl_setopt($ch, CURLOPT_POST, true);
        curl_setopt($ch, CURLOPT_POSTFIELDS, ['f' => $cfile]);
        curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
        echo curl_exec($ch);
        "#
    ));

    let _ = std::fs::remove_file(&path);
    assert!(out.contains("content-type=multipart/form-data"), "{out}");
    assert!(out.contains("parts=1"), "{out}");
    assert!(out.contains("part[0].name=f"), "{out}");
    assert!(out.contains("part[0].filename=hello.txt"), "{out}");
    assert!(out.contains("part[0].type=text/plain"), "{out}");
    assert!(out.contains("part[0].body=hello from a real file"), "{out}");
}

/// CRITICAL, review-caught: `CURLFile` with NO explicit mime must send
/// `Content-Type: application/octet-stream`, php-src's own literal default — NOT whatever
/// libcurl sniffs from the posted filename's extension. Uses a `.png` name deliberately:
/// `.png` is one of the ~10 extensions the pinned libcurl 8.21.0's `Curl_mime_contenttype`
/// (`lib/mime.c`) sniffs to a REAL image type when no explicit `curl_mime_type()` call was
/// made, so this is the shape that would have silently sent `image/png` under the bug this
/// test pins shut (`__elephc_curl_build_multipart()` used to skip the `FIELD_TYPE` call
/// entirely when `$file->mime === ""`, trusting a false claim that libcurl's own default
/// was already `application/octet-stream`).
#[test]
fn curlfile_without_mime_sends_octet_stream_not_a_sniffed_type() {
    if skip_without_curl_native("curlfile_without_mime_sends_octet_stream_not_a_sniffed_type") {
        return;
    }
    let path = unique_temp_path("avatar.png");
    std::fs::write(&path, b"not a real png, just bytes").expect("write fixture file");
    let path_str = path.to_string_lossy().into_owned();

    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/multipart");
    let out = compile_and_run(&format!(
        r#"<?php
        $ch = curl_init("{url}");
        $cfile = new CURLFile("{path_str}");
        curl_setopt($ch, CURLOPT_POST, true);
        curl_setopt($ch, CURLOPT_POSTFIELDS, ['f' => $cfile]);
        curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
        echo curl_exec($ch);
        "#
    ));

    let _ = std::fs::remove_file(&path);
    assert!(out.contains("parts=1"), "{out}");
    assert!(
        out.contains("part[0].type=application/octet-stream"),
        "a mime-less CURLFile must send application/octet-stream, not a sniffed type: {out}"
    );
}

/// A `CURLStringFile` part is entirely in-memory: no disk file is involved, and `postname`
/// (required by the constructor) and `mime` (defaulted by the constructor) are ALWAYS both
/// present on the wire, unlike `CURLFile`'s conditional type.
#[test]
fn curlstringfile_uploads_from_memory() {
    if skip_without_curl_native("curlstringfile_uploads_from_memory") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/multipart");
    let out = compile_and_run(&format!(
        r#"<?php
        $ch = curl_init("{url}");
        $sfile = new CURLStringFile("inline payload", "inline.txt", "text/plain");
        curl_setopt($ch, CURLOPT_POST, true);
        curl_setopt($ch, CURLOPT_POSTFIELDS, ['f' => $sfile]);
        curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
        echo curl_exec($ch);
        "#
    ));

    assert!(out.contains("parts=1"), "{out}");
    assert!(out.contains("part[0].name=f"), "{out}");
    assert!(out.contains("part[0].filename=inline.txt"), "{out}");
    assert!(out.contains("part[0].type=text/plain"), "{out}");
    assert!(out.contains("part[0].body=inline payload"), "{out}");
}

/// `CURLStringFile`'s `$mime` default (`"application/octet-stream"`) is honored when the
/// constructor's third argument is omitted.
#[test]
fn curlstringfile_defaults_mime_to_octet_stream() {
    if skip_without_curl_native("curlstringfile_defaults_mime_to_octet_stream") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/multipart");
    let out = compile_and_run(&format!(
        r#"<?php
        $ch = curl_init("{url}");
        $sfile = new CURLStringFile("bytes", "data.bin");
        curl_setopt($ch, CURLOPT_POST, true);
        curl_setopt($ch, CURLOPT_POSTFIELDS, ['f' => $sfile]);
        curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
        echo curl_exec($ch);
        "#
    ));

    assert!(out.contains("part[0].filename=data.bin"), "{out}");
    assert!(out.contains("part[0].type=application/octet-stream"), "{out}");
}

/// BINARY SAFETY, review-caught: an embedded NUL byte must survive intact through BOTH the
/// scalar `FIELD_DATA` path AND the `CURLStringFile` `FIELD_DATA` path — the claim "`curl_
/// mime_data` is binary-safe, not NUL-terminated" was documented everywhere in this
/// feature and proven nowhere until this test. `"a\0b"` is 3 bytes; a NUL-truncating bug
/// anywhere in the `(handle, kind, ptr, len)` marshalling chain (the mime part-field
/// builtin's lowering, the bridge's `curl_mime_data` call) would silently ship 1 byte
/// (`"a"`) instead.
#[test]
fn embedded_nul_survives_through_scalar_and_curlstringfile_data() {
    if skip_without_curl_native("embedded_nul_survives_through_scalar_and_curlstringfile_data") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/multipart");
    let out = compile_and_run(&format!(
        r#"<?php
        $ch = curl_init("{url}");
        curl_setopt($ch, CURLOPT_POST, true);
        curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
        $sfile = new CURLStringFile("a\0b", "bin.dat", "application/octet-stream");
        curl_setopt($ch, CURLOPT_POSTFIELDS, ["scalar" => "a\0b", "f" => $sfile]);
        echo curl_exec($ch);
        "#
    ));

    assert!(out.contains("parts=2"), "{out}");
    assert!(out.contains("part[0].name=scalar"), "{out}");
    assert!(out.contains("part[0].size=3"), "a NUL byte must not truncate the scalar field: {out}");
    assert!(out.contains("part[0].body=a\u{0}b"), "{out:?}");
    assert!(out.contains("part[1].name=f"), "{out}");
    assert!(
        out.contains("part[1].size=3"),
        "a NUL byte must not truncate the CURLStringFile data: {out}"
    );
    assert!(out.contains("part[1].body=a\u{0}b"), "{out:?}");
}

/// A mixed array (an ordinary scalar field alongside a `CURLFile`) posts both parts, in
/// order, in the SAME request — the shape a real upload form takes (a caption plus a
/// file).
#[test]
fn mixed_scalar_and_file_array_posts_both_parts() {
    if skip_without_curl_native("mixed_scalar_and_file_array_posts_both_parts") {
        return;
    }
    let path = unique_temp_path("mixed.txt");
    std::fs::write(&path, b"file body").expect("write fixture file");
    let path_str = path.to_string_lossy().into_owned();

    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/multipart");
    let out = compile_and_run(&format!(
        r#"<?php
        $ch = curl_init("{url}");
        $cfile = new CURLFile("{path_str}", 'text/plain', 'upload.txt');
        curl_setopt($ch, CURLOPT_POST, true);
        curl_setopt($ch, CURLOPT_POSTFIELDS, ['note' => 'hello', 'f' => $cfile]);
        curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
        echo curl_exec($ch);
        "#
    ));

    let _ = std::fs::remove_file(&path);
    assert!(out.contains("parts=2"), "{out}");
    assert!(out.contains("part[0].name=note"), "{out}");
    assert!(out.contains("part[0].filename=\n"), "{out}");
    assert!(out.contains("part[0].body=hello"), "{out}");
    assert!(out.contains("part[1].name=f"), "{out}");
    assert!(out.contains("part[1].filename=upload.txt"), "{out}");
    assert!(out.contains("part[1].body=file body"), "{out}");
}

/// Mutating `CURLFile`'s properties through `setMimeType()`/`setPostFilename()` BEFORE
/// `curl_exec()` changes what actually goes out on the wire — the walker reads the
/// object's CURRENT property values at exec time, not values captured at construction.
#[test]
fn property_mutation_before_exec_is_honored() {
    if skip_without_curl_native("property_mutation_before_exec_is_honored") {
        return;
    }
    let path = unique_temp_path("mutate.txt");
    std::fs::write(&path, b"mutated upload").expect("write fixture file");
    let path_str = path.to_string_lossy().into_owned();

    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/multipart");
    let out = compile_and_run(&format!(
        r#"<?php
        $ch = curl_init("{url}");
        $cfile = new CURLFile("{path_str}");
        $cfile->setMimeType("application/json");
        $cfile->setPostFilename("renamed.json");
        curl_setopt($ch, CURLOPT_POST, true);
        curl_setopt($ch, CURLOPT_POSTFIELDS, ['f' => $cfile]);
        curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
        echo curl_exec($ch);
        "#
    ));

    let _ = std::fs::remove_file(&path);
    assert!(out.contains("part[0].filename=renamed.json"), "{out}");
    assert!(out.contains("part[0].type=application/json"), "{out}");
}

/// `curl_file_create()` is `CURLFile::__construct()`'s alias, and the object it returns
/// uploads exactly the same way an explicit `new CURLFile(...)` does.
#[test]
fn curl_file_create_is_an_alias_of_the_constructor() {
    if skip_without_curl_native("curl_file_create_is_an_alias_of_the_constructor") {
        return;
    }
    let path = unique_temp_path("alias.txt");
    std::fs::write(&path, b"created via alias").expect("write fixture file");
    let path_str = path.to_string_lossy().into_owned();

    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/multipart");
    let out = compile_and_run(&format!(
        r#"<?php
        $ch = curl_init("{url}");
        $cfile = curl_file_create("{path_str}", "text/plain", "aliased.txt");
        echo get_class($cfile), "\n";
        curl_setopt($ch, CURLOPT_POST, true);
        curl_setopt($ch, CURLOPT_POSTFIELDS, ['f' => $cfile]);
        curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
        echo curl_exec($ch);
        "#
    ));

    let _ = std::fs::remove_file(&path);
    assert!(out.starts_with("CURLFile\n"), "{out}");
    assert!(out.contains("part[0].filename=aliased.txt"), "{out}");
    assert!(out.contains("part[0].body=created via alias"), "{out}");
}

/// DIVERGENCE FROM PHP, documented in full at `__elephc_curl_build_multipart()`
/// (`src/curl_prelude.rs`): elephc validates a `CURLFile`'s local path EAGERLY, at
/// `curl_setopt()` time (`curl_mime_filedata`'s own eager stat, measured directly against
/// this pinned libcurl 8.21.0), and `curl_setopt()` answers `false` for a file that does
/// not exist rather than accepting the option and failing later at `curl_exec()` the way a
/// real `ext/curl` does (via a custom read callback this build does not have). Both are
/// honest, defensible answers to "the file does not exist"; this fixture pins elephc's own.
#[test]
fn missing_file_makes_curl_setopt_fail_eagerly() {
    if skip_without_curl_native("missing_file_makes_curl_setopt_fail_eagerly") {
        return;
    }
    let missing = unique_temp_path("does_not_exist.txt");
    let _ = std::fs::remove_file(&missing); // ensure it really is absent
    let missing_str = missing.to_string_lossy().into_owned();

    let out = compile_and_run(&format!(
        r#"<?php
        $ch = curl_init("http://127.0.0.1:1/");
        $cfile = new CURLFile("{missing_str}");
        $result = curl_setopt($ch, CURLOPT_POSTFIELDS, ['f' => $cfile]);
        echo $result ? "true" : "false", "\n";
        "#
    ));
    assert_eq!(out, "false\n");
}

/// A SECOND, successful `CURLOPT_POSTFIELDS` array call REPLACES the first multipart body
/// rather than appending to it or leaking the first `curl_mime` structure — the bridge's
/// "attach only after the replacement succeeds, then free the old one" discipline
/// (`crates/elephc-curl/src/mime.rs`), proven here through two real transfers on the SAME
/// handle.
#[test]
fn a_second_postfields_array_call_replaces_the_first_multipart_body() {
    if skip_without_curl_native("a_second_postfields_array_call_replaces_the_first_multipart_body") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/multipart");
    let out = compile_and_run(&format!(
        r#"<?php
        $ch = curl_init("{url}");
        curl_setopt($ch, CURLOPT_POST, true);
        curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
        curl_setopt($ch, CURLOPT_POSTFIELDS, ['a' => '1']);
        $first = curl_exec($ch);
        curl_setopt($ch, CURLOPT_POSTFIELDS, ['b' => '2']);
        $second = curl_exec($ch);
        echo str_contains($first, "part[0].name=a") ? "first-a\n" : "first-not-a\n";
        echo str_contains($second, "part[0].name=b") ? "second-b\n" : "second-not-b\n";
        echo str_contains($second, "part[0].name=a") ? "second-still-a\n" : "second-clean\n";
        "#
    ));
    assert_eq!(out, "first-a\nsecond-b\nsecond-clean\n");
}

/// `curl_copy_handle()` on a handle with an attached multipart body is safe to perform a
/// real transfer with: libcurl's `curl_easy_duphandle` re-duplicates the mime part itself
/// (`crates/elephc-curl/src/abi.rs`'s `elephc_curl_easy_duphandle` doc comment), so the
/// copy needs no bridge-side rebuild the way a copied `curl_slist` option does.
#[test]
fn copy_handle_with_an_attached_multipart_body_transfers_correctly() {
    if skip_without_curl_native("copy_handle_with_an_attached_multipart_body_transfers_correctly") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/multipart");
    let out = compile_and_run(&format!(
        r#"<?php
        $ch = curl_init("{url}");
        curl_setopt($ch, CURLOPT_POST, true);
        curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
        curl_setopt($ch, CURLOPT_POSTFIELDS, ['a' => 'original']);
        $copy = curl_copy_handle($ch);
        $out = curl_exec($copy);
        echo str_contains($out, "part[0].name=a") ? "name\n" : "no-name\n";
        echo str_contains($out, "part[0].body=original") ? "body\n" : "no-body\n";
        "#
    ));
    assert_eq!(out, "name\nbody\n");
}

/// A NESTED ARRAY VALUE FLATTENS ONE LEVEL, matching php-src's own
/// `build_mime_structure_from_hash` exactly (measured directly against a real PHP 8.4.20 +
/// `ext/curl`, not assumed): `['tags' => ['a', 'b']]` posts TWO separate parts, both named
/// `tags`, one per inner array element's value — the standard repeated-field-name idiom,
/// not the "array to string conversion" mangling an earlier version of this test (and of
/// `__elephc_curl_build_multipart()`) wrongly refused with a `\TypeError`.
#[test]
fn nested_array_value_flattens_one_level_into_repeated_parts() {
    if skip_without_curl_native("nested_array_value_flattens_one_level_into_repeated_parts") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/multipart");
    let out = compile_and_run(&format!(
        r#"<?php
        $ch = curl_init("{url}");
        curl_setopt($ch, CURLOPT_POST, true);
        curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
        curl_setopt($ch, CURLOPT_POSTFIELDS, ['tags' => ['a', 'b'], 'note' => 'x']);
        echo curl_exec($ch);
        "#
    ));
    assert!(out.contains("parts=3"), "{out}");
    assert!(out.contains("part[0].name=tags"), "{out}");
    assert!(out.contains("part[0].body=a"), "{out}");
    assert!(out.contains("part[1].name=tags"), "{out}");
    assert!(out.contains("part[1].body=b"), "{out}");
    assert!(out.contains("part[2].name=note"), "{out}");
    assert!(out.contains("part[2].body=x"), "{out}");
}

/// An EMPTY nested array value posts NO parts at all for that key — the inner `foreach`
/// simply runs zero times, matching real php-src's own measured behavior for `[]`.
#[test]
fn empty_nested_array_value_posts_no_parts() {
    if skip_without_curl_native("empty_nested_array_value_posts_no_parts") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/multipart");
    let out = compile_and_run(&format!(
        r#"<?php
        $ch = curl_init("{url}");
        curl_setopt($ch, CURLOPT_POST, true);
        curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
        curl_setopt($ch, CURLOPT_POSTFIELDS, ['empty' => [], 'note' => 'x']);
        echo curl_exec($ch);
        "#
    ));
    assert!(out.contains("parts=1"), "{out}");
    assert!(out.contains("part[0].name=note"), "{out}");
}

/// DIVERGENCE FROM PHP, documented in full at `__elephc_curl_build_multipart()`: an inner
/// element that is itself an array (double nesting) is refused with a catchable
/// `\TypeError` rather than reproducing php-src's own `Warning: Array to string
/// conversion` + literal `"Array"` value for that one element.
#[test]
fn doubly_nested_array_value_is_refused() {
    if skip_without_curl_native("doubly_nested_array_value_is_refused") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
        $ch = curl_init();
        try {
            curl_setopt($ch, CURLOPT_POSTFIELDS, ['a' => [['b' => 'c']]]);
            echo "accepted\n";
        } catch (\TypeError $e) {
            echo $e->getMessage(), "\n";
        }
        "#,
    );
    assert_eq!(
        out,
        "curl_setopt(): CURLOPT_POSTFIELDS nested array value must contain only scalars\n"
    );
}

/// `CURLFile`'s properties (`name`/`mime`/`postname`) and its six methods
/// (`__construct`/`getFilename`/`getMimeType`/`getPostFilename`/`setMimeType`/
/// `setPostFilename`) all behave exactly as php-src documents, independent of any curl
/// transfer.
#[test]
fn curlfile_properties_and_getters_match_php() {
    if skip_without_curl_native("curlfile_properties_and_getters_match_php") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
        $f = new CURLFile("/tmp/does-not-need-to-exist.txt", null, null);
        echo $f->getFilename(), "|";
        echo $f->getMimeType() === "" ? "empty" : $f->getMimeType(), "|";
        echo $f->getPostFilename() === "" ? "empty" : $f->getPostFilename(), "|";
        echo $f->name, "|", ($f->mime === "" ? "empty" : $f->mime), "|", ($f->postname === "" ? "empty" : $f->postname), "\n";
        $f->setMimeType("application/json");
        $f->setPostFilename("renamed.json");
        echo $f->getMimeType(), "|", $f->getPostFilename(), "|", $f->mime, "|", $f->postname, "\n";
        "#,
    );
    assert_eq!(
        out,
        "/tmp/does-not-need-to-exist.txt|empty|empty|/tmp/does-not-need-to-exist.txt|empty|empty\n\
         application/json|renamed.json|application/json|renamed.json\n"
    );
}

/// A loop that builds and posts a multipart body (one `CURLFile` part, one
/// `CURLStringFile` part, one scalar field) several times over the SAME handle must not
/// grow elephc's own heap: every `__elephc_curl_build_multipart()` array-walk allocation
/// (the `(string) $x . ""` locals, the array literal itself) has to be released each
/// iteration, exactly like the pre-existing "bind to a local"/leak-avoidance rules this
/// prelude already follows for every other wrapper. This is a PHP-heap balance check
/// (`--gc-stats`), not a native libcurl memory check — the mime builder's own native
/// allocations are exercised (and freed on replace) by
/// `a_second_postfields_array_call_replaces_the_first_multipart_body` above and by
/// `crates/elephc-curl/src/tests.rs::native_mime` directly.
#[test]
fn repeated_multipart_builds_do_not_leak_the_php_heap() {
    if skip_without_curl_native("repeated_multipart_builds_do_not_leak_the_php_heap") {
        return;
    }
    let path = unique_temp_path("leak.txt");
    std::fs::write(&path, b"leak-check-body").expect("write fixture file");
    let path_str = path.to_string_lossy().into_owned();

    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/multipart");
    let output = compile_and_run_with_gc_stats(&format!(
        r#"<?php
        function cycle(string $url): int {{
            $ch = curl_init($url);
            $cfile = new CURLFile("{path_str}", "text/plain", "hello.txt");
            $sfile = new CURLStringFile("inline", "inline.txt", "text/plain");
            curl_setopt($ch, CURLOPT_POST, true);
            curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
            curl_setopt($ch, CURLOPT_POSTFIELDS, ["note" => "hi", "f" => $cfile, "s" => $sfile]);
            $out = curl_exec($ch);
            unset($cfile);
            unset($sfile);
            unset($ch);
            return strlen($out);
        }}
        echo cycle("{url}"), ",", cycle("{url}"), ",", cycle("{url}"), "\n";
        "#
    ));

    let _ = std::fs::remove_file(&path);
    let (allocs, frees) = parse_gc_stats(&output.stderr);
    assert_eq!(
        allocs, frees,
        "repeated multipart array walks must not leak or double-free the PHP heap: {}",
        output.stderr
    );
}
