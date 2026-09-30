//! Purpose:
//! Integration tests for the edges of `SplFileObject`/`SplFileInfo`: the flag matrix that decides
//! the trailing element, what `seek()` does outside the file, and what a stat-backed getter does
//! when it cannot stat.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - THE TRAILING ELEMENT TAKES THREE SHAPES, and which one it takes depends on TWO flags. Two
//!   earlier readings of this each had half of it because neither varied `READ_AHEAD`. MEASURED
//!   over the whole 6 file shapes × 8 flags matrix on `php -n` 8.5.6.
//! - `SKIP_EMPTY` only skips blank LINES together with `DROP_NEW_LINE`, and then the keys are
//!   CONSECUTIVE. Under `READ_CSV` the opposite holds: php steps over the blank record and leaves
//!   the keys of the records that follow unchanged.
//! - `seek()` refuses a negative line with a `ValueError`, and past the end it clamps the key and
//!   leaves the object INVALID — php walks the stream to get there, so the walk consumes it.
//! - A stat-backed `SplFileInfo` getter THROWS for a path it cannot stat. elephc answered `0` or
//!   `false` in silence, so a program that trusted `getSize()` got a size of zero for a file that
//!   was not there.
//! - Every expectation was measured on `php -n` 8.5.6.

use crate::support::*;

/// Verifies the whole flag matrix that decides the trailing element and the blank-line skipping.
///
/// One program covers all eight flag values on six file shapes on purpose: the three rules
/// interact, and a test per rule is exactly how the first two readings each missed the third.
#[test]
fn the_trailing_element_depends_on_skip_empty_and_read_ahead() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
$cases = [
    "empty"        => "",
    "one-nl"       => "a\n",
    "blank-middle" => "a\n\nb\n",
];
foreach ($cases as $label => $body) {
    file_put_contents("f.txt", $body);
    for ($flags = 0; $flags < 8; $flags++) {
        $o = new SplFileObject("f.txt");
        $o->setFlags($flags);
        echo $label, "/", $flags, "=>";
        foreach ($o as $k => $line) { echo $k, ":", var_export($line, true), ";"; }
        echo "\n";
        unset($o);
    }
}
"#,
    );
    assert_eq!(
        out,
        "empty/0=>0:'';\n\
         empty/1=>0:'';\n\
         empty/2=>0:'';\n\
         empty/3=>0:'';\n\
         empty/4=>0:false;\n\
         empty/5=>0:false;\n\
         empty/6=>\n\
         empty/7=>\n\
         one-nl/0=>0:'a\n';1:'';\n\
         one-nl/1=>0:'a';1:'';\n\
         one-nl/2=>0:'a\n';1:'';\n\
         one-nl/3=>0:'a';1:'';\n\
         one-nl/4=>0:'a\n';1:false;\n\
         one-nl/5=>0:'a';1:false;\n\
         one-nl/6=>0:'a\n';\n\
         one-nl/7=>0:'a';\n\
         blank-middle/0=>0:'a\n';1:'\n';2:'b\n';3:'';\n\
         blank-middle/1=>0:'a';1:'';2:'b';3:'';\n\
         blank-middle/2=>0:'a\n';1:'\n';2:'b\n';3:'';\n\
         blank-middle/3=>0:'a';1:'';2:'b';3:'';\n\
         blank-middle/4=>0:'a\n';1:'\n';2:'b\n';3:false;\n\
         blank-middle/5=>0:'a';1:'b';2:false;\n\
         blank-middle/6=>0:'a\n';1:'\n';2:'b\n';\n\
         blank-middle/7=>0:'a';1:'b';\n"
    );
    let _ = std::fs::remove_dir_all(dir);
}

/// Verifies `seek()` at and beyond both ends.
///
/// The two out-of-range answers differ from each other, which is what a single clamp would have
/// got wrong: seeking TO the last element leaves the value readable and the object invalid, while
/// seeking PAST it leaves the value `false` as well.
#[test]
fn seek_refuses_a_negative_line_and_clamps_past_the_end() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("f.txt", "l0\nl1\nl2\nl3\n");
$o = new SplFileObject("f.txt");
$o->seek(99);
echo "over key ", $o->key(), " valid ", var_export($o->valid(), true), " cur ", var_export($o->current(), true), "\n";
$o->seek(0);
echo "zero key ", $o->key(), " cur ", var_export($o->current(), true), "\n";
$o->seek(4);
echo "last key ", $o->key(), " cur ", var_export($o->current(), true), " valid ", var_export($o->valid(), true), "\n";
try { $o->seek(-1); echo "neg no throw\n"; }
catch (Throwable $e) { echo "neg ", get_class($e), ": ", $e->getMessage(), "\n"; }
"#,
    );
    assert_eq!(
        out,
        "over key 4 valid false cur false\n\
         zero key 0 cur 'l0\n'\n\
         last key 4 cur '' valid false\n\
         neg ValueError: SplFileObject::seek(): Argument #1 ($line) must be greater than or equal to 0\n"
    );
    let _ = std::fs::remove_dir_all(dir);
}

/// Verifies every stat-backed `SplFileInfo` getter throws for a path it cannot stat.
///
/// All nine together, because they share one guard and php words `getType()`'s differently.
#[test]
fn a_stat_backed_getter_throws_for_a_path_it_cannot_stat() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("f.txt", "1234");
echo "size ", (new SplFileInfo("f.txt"))->getSize(), "\n";
$m = new SplFileInfo("nope.txt");
try { $m->getSize(); }  catch (Throwable $t) { echo get_class($t), ": ", $t->getMessage(), "\n"; }
try { $m->getOwner(); } catch (Throwable $t) { echo get_class($t), ": ", $t->getMessage(), "\n"; }
try { $m->getGroup(); } catch (Throwable $t) { echo get_class($t), ": ", $t->getMessage(), "\n"; }
try { $m->getATime(); } catch (Throwable $t) { echo get_class($t), ": ", $t->getMessage(), "\n"; }
try { $m->getMTime(); } catch (Throwable $t) { echo get_class($t), ": ", $t->getMessage(), "\n"; }
try { $m->getCTime(); } catch (Throwable $t) { echo get_class($t), ": ", $t->getMessage(), "\n"; }
try { $m->getPerms(); } catch (Throwable $t) { echo get_class($t), ": ", $t->getMessage(), "\n"; }
try { $m->getInode(); } catch (Throwable $t) { echo get_class($t), ": ", $t->getMessage(), "\n"; }
try { $m->getType(); }  catch (Throwable $t) { echo get_class($t), ": ", $t->getMessage(), "\n"; }
echo "isFile ", var_export($m->isFile(), true), "\n";
"#,
    );
    assert_eq!(
        out,
        "size 4\n\
         RuntimeException: SplFileInfo::getSize(): stat failed for nope.txt\n\
         RuntimeException: SplFileInfo::getOwner(): stat failed for nope.txt\n\
         RuntimeException: SplFileInfo::getGroup(): stat failed for nope.txt\n\
         RuntimeException: SplFileInfo::getATime(): stat failed for nope.txt\n\
         RuntimeException: SplFileInfo::getMTime(): stat failed for nope.txt\n\
         RuntimeException: SplFileInfo::getCTime(): stat failed for nope.txt\n\
         RuntimeException: SplFileInfo::getPerms(): stat failed for nope.txt\n\
         RuntimeException: SplFileInfo::getInode(): stat failed for nope.txt\n\
         RuntimeException: SplFileInfo::getType(): Lstat failed for nope.txt\n\
         isFile false\n"
    );
    let _ = std::fs::remove_dir_all(dir);
}

/// Verifies `file_put_contents()` DRAINS a stream argument instead of naming it.
///
/// php writes what the handle still holds; elephc converted the handle to a string and wrote the
/// fourteen bytes of `Resource id #5`.
#[test]
fn file_put_contents_drains_a_stream_argument() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
$src = fopen("php://memory", "r+");
fwrite($src, "from-stream");
rewind($src);
var_dump(file_put_contents("out.txt", $src));
echo file_get_contents("out.txt"), "\n";
fclose($src);
var_dump(file_put_contents("out.txt", 42));
echo file_get_contents("out.txt"), "\n";
var_dump(file_put_contents("out.txt", ["a", "b"]));
echo file_get_contents("out.txt"), "\n";
"#,
    );
    assert_eq!(
        out,
        "int(11)\n\
         from-stream\n\
         int(2)\n\
         42\n\
         int(2)\n\
         ab\n"
    );
    let _ = std::fs::remove_dir_all(dir);
}
