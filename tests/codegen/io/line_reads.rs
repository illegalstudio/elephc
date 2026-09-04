//! Purpose:
//! Integration tests for LINE reads on a buffered stream: `fgets()` and `stream_get_line()` take
//! their bytes from the stream's own holding area, and the answers around them stay php's.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - A line reader cannot ask a descriptor for the bytes it wants, because it does not know how
//!   many there are until it finds the newline. elephc therefore asked for ONE, and paid a
//!   `read(2)` for it: MEASURED at 538 ms over a 900 KB file of 100 000 lines where php takes
//!   4 ms. Filling the holding area a CHUNK at a time is what php does, and it is what lets the
//!   newline scan run over memory instead of over syscalls — the same file now measures 20 ms.
//! - Filling it moves other answers, which is what these tests pin: a line may SPAN chunks,
//!   `ftell()` must report the CONSUMED position and not where the fill stopped, a seek must
//!   discard what is held, and `unread_bytes` must report the remainder.
//! - `stream_get_line()` has to see the held bytes through the same door as descriptor bytes,
//!   because its delimiter scan lives behind that door. A bulk drain straight into the result
//!   window skipped it, and the delimiter can also STRADDLE the boundary between the two sources.
//! - Every expectation was measured on `php -n` 8.5.6.

use crate::support::*;

/// Verifies that `fgets()` walks a file whose lines outrun the fill, and stops where php stops.
///
/// The 20 000-byte line is the point: it spans several chunks, so a reader that treats one fill
/// as one line — or that loses the bytes it already held when it refills — answers short here and
/// nowhere else.
#[test]
fn test_fgets_walks_lines_that_span_the_fill() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
$h = fopen("many.txt", "w");
for ($i = 0; $i < 3000; $i++) { fwrite($h, str_pad("l$i", 9, "-") . "\n"); }
fwrite($h, str_repeat("x", 20000) . "\n");
fwrite($h, "tail\n");
fclose($h);

$h = fopen("many.txt", "rb");
$n = 0;
$bytes = 0;
$longest = 0;
while (($l = fgets($h)) !== false) {
    $n++;
    $bytes += strlen($l);
    if (strlen($l) > $longest) { $longest = strlen($l); }
    if ($n === 1 || $n === 3000) { echo "line$n ", rtrim($l, "\n"), "\n"; }
}
echo "count $n bytes $bytes longest $longest\n";
echo "eof ", var_export(feof($h), true), " tell ", ftell($h), "\n";
fclose($h);
unlink("many.txt");
"#,
    );
    assert_eq!(
        out,
        "line1 l0-------\n\
         line3000 l2999----\n\
         count 3002 bytes 50006 longest 20001\n\
         eof true tell 50006\n"
    );
    let _ = std::fs::remove_dir_all(dir);
}

/// Verifies the positions, EOF and buffer report that a filled holding area must not change.
///
/// One program covers them together on purpose: the fill is a single mechanism, and a change that
/// gets the line right while moving `ftell()` past the bytes the caller has actually consumed
/// would pass two narrower tests.
#[test]
fn test_a_line_read_leaves_phps_position_and_buffer() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("mix.txt", "alpha\nbravo\ncharlie\ndelta\n");
$h = fopen("mix.txt", "rb");
echo "one ", rtrim(fgets($h), "\n"), " tell ", ftell($h), " eof ", var_export(feof($h), true), "\n";
echo "held ", stream_get_meta_data($h)["unread_bytes"], "\n";
echo "read5 ", fread($h, 5), " tell ", ftell($h), "\n";
echo "rest ", rtrim(fgets($h), "\n"), " tell ", ftell($h), "\n";
fseek($h, 0);
echo "seek0 ", rtrim(fgets($h), "\n"), " tell ", ftell($h), "\n";
echo "bounded ", fgets($h, 4), " tell ", ftell($h), "\n";
echo "line ", rtrim(fgets($h), "\n"), " tell ", ftell($h), "\n";
echo "last ", rtrim(fgets($h), "\n"), " eof ", var_export(feof($h), true), "\n";
echo "after ", var_export(fgets($h), true), " eof ", var_export(feof($h), true), "\n";
fclose($h);
unlink("mix.txt");
"#,
    );
    assert_eq!(
        out,
        "one alpha tell 6 eof false\n\
         held 20\n\
         read5 bravo tell 11\n\
         rest  tell 12\n\
         seek0 alpha tell 6\n\
         bounded bra tell 9\n\
         line vo tell 12\n\
         last charlie eof false\n\
         after 'delta\n' eof false\n"
    );
    let _ = std::fs::remove_dir_all(dir);
}

/// Verifies that `stream_get_line()` applies its delimiter to bytes another reader left held.
///
/// `fgets($h, 4)` on `abcdef` hands back `abc` and leaves `def` on the stream. php then answers
/// `d` for a `stream_get_line()` ending on `ef`; elephc answered the whole `def`, because the
/// drained bytes went straight into the result window and never met the delimiter scan. The plain
/// file below adds the case the scan alone cannot see: a delimiter reached AFTER the held bytes
/// run out, so the match straddles the holding area and the descriptor.
#[test]
fn test_stream_get_line_delimits_the_bytes_fgets_left_held() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
$h = fopen('data://text/plain,abcdef', 'r');
echo fgets($h, 4), "\n";
echo stream_get_line($h, 1024, 'ef'), "\n";
fclose($h);

file_put_contents("d.txt", "one|two|three\nfour|five\n");
$h = fopen("d.txt", "rb");
echo fgets($h, 5), "\n";
echo stream_get_line($h, 1024, "|"), "\n";
echo stream_get_line($h, 1024, "\n"), "\n";
echo var_export(stream_get_line($h, 1024, "|"), true), "\n";
echo "eof ", var_export(feof($h), true), "\n";
fclose($h);
unlink("d.txt");
"#,
    );
    assert_eq!(
        out,
        "abc\n\
         d\n\
         one|\n\
         two\n\
         three\n\
         'four'\n\
         eof false\n"
    );
    let _ = std::fs::remove_dir_all(dir);
}
