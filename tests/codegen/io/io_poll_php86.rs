//! Purpose:
//! Runs PHP 8.6 `Io\Poll` fixtures against the portable `Poll` backend.
//!
//! Called from:
//! - `cargo test --test codegen_tests io_poll_php86`.
//!
//! Key details:
//! - Sources are transcribed from `ext/standard/tests/poll/*.phpt` with the upstream
//!   `poll.inc` helper inlined, because elephc's checker rejects several untyped helper
//!   parameter shapes used by that file.
//! - The portable backend maps level-triggered readiness onto `stream_select()`; edge
//!   triggering, hang-up precision, and descriptors above 63 are documented limitations.
//! - The closed-handle fixture now runs unchanged, including PHP's `get_debug_type()`
//!   `"resource (closed)"` diagnostic.
//! - Tests compare stdout only. Warnings and fatal diagnostics are not part of these fixtures.

use elephc::php_version::PhpVersion;

use crate::support::*;

/// Compiles and runs one fixture with the PHP 8.6 profile.
fn run86(source: &str) -> String {
    compile_and_run_with_php_version(source, PhpVersion::Php86)
}

/// php-src `poll_backend_getAvailableBackends.phpt`, `poll_ctx_backend_unix.phpt`,
/// and `poll_backend` availability predicates.
#[test]
fn test_io_poll_backend_surface_php86() {
    let out = run86(
        r#"<?php
foreach (Io\Poll\Backend::getAvailableBackends() as $backend) {
    echo $backend->name, '|';
}
echo Io\Poll\Backend::Poll->isAvailable() ? 'yes' : 'no';
echo '|';
echo Io\Poll\Backend::WSAPoll->isAvailable() ? 'yes' : 'no';
$context = new Io\Poll\Context(Io\Poll\Backend::Poll);
echo '|', $context->getBackend()->name;
try {
    new Io\Poll\Context(Io\Poll\Backend::Kqueue);
} catch (Throwable $e) {
    echo '|', $e::class, ': ', $e->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "Poll|yes|no|Poll|Io\\Poll\\BackendUnavailableException: Backend Kqueue not available"
    );
}

/// php-src `poll_stream_sock_add_only.phpt`: strict properties are hidden from var_dump.
#[test]
fn test_io_poll_add_only_dump_php86() {
    let out = run86(
        r#"<?php
[$socket1, $socket2] = stream_socket_pair(STREAM_PF_UNIX, STREAM_SOCK_STREAM, 0);
$context = new Io\Poll\Context(Io\Poll\Backend::Poll);
$context->add(new StreamPollHandle($socket2), [Io\Poll\Event::Write], 'socket2_data');
var_dump($context);
"#,
    );
    assert!(out.starts_with("object(Io\\Poll\\Context)#"), "{out}");
    assert!(out.contains("(0) {"), "{out}");
}

/// php-src `poll_stream_sock_read.phpt` and `poll_stream_sock_write.phpt`.
#[test]
fn test_io_poll_socket_read_write_php86() {
    let out = run86(
        r#"<?php
[$r, $w] = stream_socket_pair(STREAM_PF_UNIX, STREAM_SOCK_STREAM, 0);
$context = new Io\Poll\Context(Io\Poll\Backend::Poll);
$reader = $context->add(new StreamPollHandle($r), [Io\Poll\Event::Read], 'socket1_data');
$writer = $context->add(new StreamPollHandle($w), [Io\Poll\Event::Write], 'socket2_data');
$events = $context->wait(Time\Duration::fromSeconds(0));
echo count($events), ':', $events[0]->getData(), ':', $events[0]->hasTriggered(Io\Poll\Event::Write) ? 'W' : '-', "\n";
fwrite($w, 'test data');
$events = $context->wait(Time\Duration::fromMicroseconds(100000));
echo count($events), "\n";
foreach ($events as $event) {
    echo $event->getData(), ':', $event->hasTriggered(Io\Poll\Event::Read) ? 'R' : '-', $event->hasTriggered(Io\Poll\Event::Write) ? 'W' : '-';
    if ($event->hasTriggered(Io\Poll\Event::Read)) {
        echo ':', fread($event->getHandle()->getStream(), 1024);
    }
    echo "\n";
}
"#,
    );
    assert_eq!(out, "1:socket2_data:W\n2\nsocket1_data:R-:test data\nsocket2_data:-W\n");
}

/// php-src `poll_stream_sock_modify_write.phpt` and `poll_stream_sock_remove_write.phpt`.
#[test]
fn test_io_poll_modify_and_remove_php86() {
    let out = run86(
        r#"<?php
[$r1, $w1] = stream_socket_pair(STREAM_PF_UNIX, STREAM_SOCK_STREAM, 0);
[$r2, $w2] = stream_socket_pair(STREAM_PF_UNIX, STREAM_SOCK_STREAM, 0);
$context = new Io\Poll\Context(Io\Poll\Backend::Poll);
$first = $context->add(new StreamPollHandle($w1), [Io\Poll\Event::Write], 'socket_data_1');
$second = $context->add(new StreamPollHandle($w2), [Io\Poll\Event::Write], 'socket_data_2');
$events = $context->wait(Time\Duration::fromSeconds(0));
echo count($events), ':', $events[0]->getData(), ':', $events[1]->getData(), "\n";
$first->modify([Io\Poll\Event::Write], 'modified_data');
$events = $context->wait(Time\Duration::fromSeconds(0));
echo count($events), ':', $events[0]->getData(), ':', $events[1]->getData(), "\n";
$first->remove();
$events = $context->wait(Time\Duration::fromSeconds(0));
echo count($events), ':', $events[0]->getData(), "\n";
var_dump(fwrite($w1, 'test 1'), fwrite($w2, 'test 2'), fread($r1, 100), fread($r2, 100));
"#,
    );
    assert_eq!(
        out,
        "2:socket_data_1:socket_data_2\n2:modified_data:socket_data_2\n1:socket_data_2\nint(6)\nint(6)\nstring(6) \"test 1\"\nstring(6) \"test 2\"\n"
    );
}

/// php-src `poll_stream_tcp_read.phpt` and `poll_stream_sock_rw_single_level.phpt`.
#[test]
fn test_io_poll_socket_level_php86() {
    let out = run86(
        r#"<?php
[$r, $w] = stream_socket_pair(STREAM_PF_UNIX, STREAM_SOCK_STREAM, 0);
$context = new Io\Poll\Context(Io\Poll\Backend::Poll);
$context->add(new StreamPollHandle($r), [Io\Poll\Event::Read], 'socket1_data');
$context->add(new StreamPollHandle($w), [Io\Poll\Event::Write], 'socket2_data');
$events = $context->wait(Time\Duration::fromSeconds(0));
echo count($events), ':', $events[0]->getData(), "\n";
fwrite($w, 'test data');
$events = $context->wait(Time\Duration::fromMicroseconds(100000));
echo count($events), "\n";
foreach ($events as $event) {
    echo $event->getData(), ':', $event->hasTriggered(Io\Poll\Event::Read) ? 'R' : '-', $event->hasTriggered(Io\Poll\Event::Write) ? 'W' : '-';
    if ($event->hasTriggered(Io\Poll\Event::Read)) {
        echo ':', fread($event->getHandle()->getStream(), 1024);
    }
    echo "\n";
}
"#,
    );
    assert_eq!(out, "1:socket2_data\n2\nsocket1_data:R-:test data\nsocket2_data:-W\n");
}

/// php-src `poll_stream_tcp_read_one_shot.phpt`.
#[test]
fn test_io_poll_tcp_read_one_shot_php86() {
    let out = run86(
        r#"<?php
function pair(): array {
    $server = stream_socket_server('tcp://127.0.0.1:0');
    $address = stream_socket_get_name($server, false);
    $client = stream_socket_client('tcp://' . $address);
    $conn = stream_socket_accept($server);
    fclose($server);
    return [$client, $conn];
}
[$c1, $s1] = pair();
[$c2, $s2] = pair();
$context = new Io\Poll\Context(Io\Poll\Backend::Poll);
$context->add(new StreamPollHandle($c1), [Io\Poll\Event::Read, Io\Poll\Event::OneShot], 'client1_data');
$context->add(new StreamPollHandle($s1), [Io\Poll\Event::Read, Io\Poll\Event::OneShot], 'server1_data');
$context->add(new StreamPollHandle($c2), [Io\Poll\Event::Read, Io\Poll\Event::OneShot], 'client2_data');
$context->add(new StreamPollHandle($s2), [Io\Poll\Event::Read, Io\Poll\Event::OneShot], 'server2_data');
echo count($context->wait(Time\Duration::fromSeconds(0))), "\n";
fwrite($c1, 'test data'); usleep(10000);
fwrite($c2, 'test data'); usleep(10000);
$events = $context->wait(Time\Duration::fromMicroseconds(100000));
echo count($events), "\n";
foreach ($events as $event) {
    echo $event->getData(), ':', fread($event->getHandle()->getStream(), 1024), "\n";
}
fwrite($c1, 'more data'); usleep(10000);
fwrite($c2, 'more data'); usleep(10000);
echo count($context->wait(Time\Duration::fromMicroseconds(100000))), "\n";
"#,
    );
    assert_eq!(out, "0\n2\nserver1_data:test data\nserver2_data:test data\n0\n");
}

/// php-src `poll_stream_tcp_rw_one_shot.phpt` and `poll_stream_tcp_rw_one_shot_mixed.phpt`.
#[test]
fn test_io_poll_tcp_rw_one_shot_php86() {
    let out = run86(
        r#"<?php
$server = stream_socket_server('tcp://127.0.0.1:0');
$address = stream_socket_get_name($server, false);
$client = stream_socket_client('tcp://' . $address);
$conn = stream_socket_accept($server);
fclose($server);
$context = new Io\Poll\Context(Io\Poll\Backend::Poll);
$context->add(new StreamPollHandle($client), [Io\Poll\Event::Read, Io\Poll\Event::Write, Io\Poll\Event::OneShot], 'client_data');
$context->add(new StreamPollHandle($conn), [Io\Poll\Event::Read, Io\Poll\Event::Write, Io\Poll\Event::OneShot], 'server_data');
echo count($context->wait(Time\Duration::fromSeconds(0))), "\n";
fwrite($client, 'test data'); usleep(10000);
echo count($context->wait(Time\Duration::fromMicroseconds(100000))), "\n";
fwrite($client, 'test data'); usleep(10000);
echo count($context->wait(Time\Duration::fromMicroseconds(100000))), "\n";
"#,
    );
    assert_eq!(out, "2\n0\n0\n");
}

/// php-src `poll_stream_buffered_level.phpt`.
#[test]
fn test_io_poll_buffered_level_php86() {
    let out = run86(
        r#"<?php
[$a, $b] = stream_socket_pair(STREAM_PF_UNIX, STREAM_SOCK_STREAM, 0);
fwrite($b, "first\nsecond\n");
var_dump(fgets($a));
$context = new Io\Poll\Context(Io\Poll\Backend::Poll);
$watcher = $context->add(new StreamPollHandle($a), [Io\Poll\Event::Read], 'a');
$events = $context->wait(Time\Duration::fromSeconds(5));
var_dump(count($events), $events[0] === $watcher, $events[0]->hasTriggered(Io\Poll\Event::Read));
var_dump(count($context->wait(Time\Duration::fromSeconds(0))));
var_dump(fgets($a));
var_dump(count($context->wait(Time\Duration::fromSeconds(0))));
fwrite($b, "third\nfourth\n");
var_dump(fgets($a));
$watcher->modifyEvents([Io\Poll\Event::Write]);
$events = $context->wait(Time\Duration::fromSeconds(0));
var_dump(count($events), $events[0]->hasTriggered(Io\Poll\Event::Write), $events[0]->hasTriggered(Io\Poll\Event::Read));
$watcher->modifyEvents([Io\Poll\Event::Read]);
var_dump(count($context->wait(Time\Duration::fromSeconds(0))));
"#,
    );
    assert_eq!(
        out,
        "string(6) \"first\n\"\nint(1)\nbool(true)\nbool(true)\nint(1)\nstring(7) \"second\n\"\nint(0)\nstring(6) \"third\n\"\nint(1)\nbool(true)\nbool(false)\nint(1)\n"
    );
}

/// php-src `poll_stream_buffered_max_events.phpt`.
#[test]
fn test_io_poll_buffered_max_events_php86() {
    let out = run86(
        r#"<?php
[$a, $b] = stream_socket_pair(STREAM_PF_UNIX, STREAM_SOCK_STREAM, 0);
[$c, $d] = stream_socket_pair(STREAM_PF_UNIX, STREAM_SOCK_STREAM, 0);
[$e, $f] = stream_socket_pair(STREAM_PF_UNIX, STREAM_SOCK_STREAM, 0);
$context = new Io\Poll\Context(Io\Poll\Backend::Poll);
$context->add(new StreamPollHandle($a), [Io\Poll\Event::Read], 'a');
$context->add(new StreamPollHandle($c), [Io\Poll\Event::Read], 'c');
$context->add(new StreamPollHandle($e), [Io\Poll\Event::Read], 'e');
fwrite($b, "first\nsecond\n");
fwrite($d, "first\nsecond\n");
var_dump(fgets($a), fgets($c));
fwrite($f, 'x');
var_dump(count($context->wait(Time\Duration::fromSeconds(0), 1)));
var_dump(count($context->wait(Time\Duration::fromSeconds(0), 2)));
var_dump(count($context->wait(Time\Duration::fromSeconds(0))));
var_dump(count($context->wait(Time\Duration::fromSeconds(0), 3)));
var_dump(fgets($a), fgets($c), fread($e, 10));
var_dump(count($context->wait(Time\Duration::fromSeconds(0))));
"#,
    );
    assert_eq!(
        out,
        "string(6) \"first\n\"\nstring(6) \"first\n\"\nint(1)\nint(2)\nint(3)\nint(3)\nstring(7) \"second\n\"\nstring(7) \"second\n\"\nstring(1) \"x\"\nint(0)\n"
    );
}

/// php-src `poll_stream_buffered_merge.phpt`.
#[test]
fn test_io_poll_buffered_merge_php86() {
    let out = run86(
        r#"<?php
[$a, $b] = stream_socket_pair(STREAM_PF_UNIX, STREAM_SOCK_STREAM, 0);
[$c, $d] = stream_socket_pair(STREAM_PF_UNIX, STREAM_SOCK_STREAM, 0);
$context = new Io\Poll\Context(Io\Poll\Backend::Poll);
$wa = $context->add(new StreamPollHandle($a), [Io\Poll\Event::Read], 'a');
$wc = $context->add(new StreamPollHandle($c), [Io\Poll\Event::Read], 'c');
fwrite($b, "first\nsecond\n");
var_dump(fgets($a));
fwrite($d, 'x');
$events = $context->wait(Time\Duration::fromMilliseconds(100));
$seen = [];
foreach ($events as $e) { $seen[$e->getData()] = $e->hasTriggered(Io\Poll\Event::Read); }
ksort($seen);
var_dump($seen);
fwrite($b, "third\n");
var_dump(fread($c, 10));
$events = $context->wait(Time\Duration::fromMilliseconds(100));
var_dump(count($events), $events[0] === $wa, $events[0]->hasTriggered(Io\Poll\Event::Read));
var_dump(fgets($a), fgets($a));
var_dump(count($context->wait(Time\Duration::fromSeconds(0))));
"#,
    );
    assert_eq!(
        out,
        "string(6) \"first\n\"\narray(2) {\n  [\"a\"]=>\n  bool(true)\n  [\"c\"]=>\n  bool(true)\n}\nstring(1) \"x\"\nint(1)\nbool(true)\nbool(true)\nstring(7) \"second\n\"\nstring(6) \"third\n\"\nint(0)\n"
    );
}

/// php-src `poll_stream_buffered_oneshot.phpt`.
#[test]
fn test_io_poll_buffered_one_shot_php86() {
    let out = run86(
        r#"<?php
[$a, $b] = stream_socket_pair(STREAM_PF_UNIX, STREAM_SOCK_STREAM, 0);
fwrite($b, "first\nsecond\n");
var_dump(fgets($a));
$context = new Io\Poll\Context(Io\Poll\Backend::Poll);
$watcher = $context->add(new StreamPollHandle($a), [Io\Poll\Event::Read, Io\Poll\Event::OneShot], 'a');
$events = $context->wait(Time\Duration::fromSeconds(0));
var_dump(count($events), $events[0] === $watcher);
var_dump(count($context->wait(Time\Duration::fromSeconds(0))));
fwrite($b, "third\n");
var_dump(count($context->wait(Time\Duration::fromMilliseconds(50))));
var_dump($watcher->isActive());
$watcher->modifyEvents([Io\Poll\Event::Read, Io\Poll\Event::OneShot]);
var_dump(count($context->wait(Time\Duration::fromSeconds(0))));
var_dump(count($context->wait(Time\Duration::fromSeconds(0))));
var_dump(fgets($a), fgets($a));
$watcher->modifyEvents([Io\Poll\Event::Read, Io\Poll\Event::OneShot]);
var_dump(count($context->wait(Time\Duration::fromSeconds(0))));
fwrite($b, "fourth\n");
$events = $context->wait(Time\Duration::fromMilliseconds(100));
var_dump(count($events), $events[0]->hasTriggered(Io\Poll\Event::Read));
$watcher->remove();
var_dump($watcher->isActive());
"#,
    );
    assert_eq!(
        out,
        "string(6) \"first\n\"\nint(1)\nbool(true)\nint(0)\nint(0)\nbool(true)\nint(1)\nint(0)\nstring(7) \"second\n\"\nstring(6) \"third\n\"\nint(0)\nint(1)\nbool(true)\nbool(false)\n"
    );
}

/// php-src `poll_stream_oneshot_rearm.phpt`.
#[test]
fn test_io_poll_one_shot_rearm_php86() {
    let out = run86(
        r#"<?php
[$r, $w] = stream_socket_pair(STREAM_PF_UNIX, STREAM_SOCK_STREAM, 0);
$context = new Io\Poll\Context(Io\Poll\Backend::Poll);
$handle = new StreamPollHandle($r);
$watcher = $context->add($handle, [Io\Poll\Event::Read, Io\Poll\Event::OneShot], 'data');
fwrite($w, 'a');
echo 'Events count: ', count($context->wait(Time\Duration::fromMicroseconds(100000))), "\n";
var_dump(fread($r, 10));
fwrite($w, 'b');
echo 'Events count: ', count($context->wait(Time\Duration::fromSeconds(0))), "\n";
var_dump($watcher->isActive());
try {
    $context->add($handle, [Io\Poll\Event::Read]);
} catch (Io\Poll\HandleAlreadyWatchedException $e) {
    echo $e::class, ': ', $e->getMessage(), "\n";
}
$watcher->modifyEvents([Io\Poll\Event::Read, Io\Poll\Event::OneShot]);
$events = $context->wait(Time\Duration::fromMicroseconds(100000));
echo 'Events count: ', count($events), "\n";
var_dump($events[0] === $watcher);
var_dump(fread($r, 10));
$watcher->remove();
var_dump($watcher->isActive());
"#,
    );
    assert_eq!(
        out,
        "Events count: 1\nstring(1) \"a\"\nEvents count: 0\nbool(true)\nIo\\Poll\\HandleAlreadyWatchedException: Handle already added\nEvents count: 1\nbool(true)\nstring(1) \"b\"\nbool(false)\n"
    );
}

/// php-src `poll_stream_modify_oneshot_flag.phpt`.
#[test]
fn test_io_poll_modify_one_shot_flag_php86() {
    let out = run86(
        r#"<?php
use Io\Poll\Event;
[$r, $w] = stream_socket_pair(STREAM_PF_UNIX, STREAM_SOCK_STREAM, 0);
$context = new Io\Poll\Context(Io\Poll\Backend::Poll);
$watcher = $context->add(new StreamPollHandle($r), [Event::Read]);
fwrite($w, 'a');
echo 'Level: ', count($context->wait(Time\Duration::fromSeconds(0))), ' ', count($context->wait(Time\Duration::fromSeconds(0))), "\n";
$watcher->modifyEvents([Event::Read, Event::OneShot]);
echo 'One-shot: ', count($context->wait(Time\Duration::fromSeconds(0))), ' ', count($context->wait(Time\Duration::fromSeconds(0))), "\n";
var_dump($watcher->isActive());
$watcher->modifyEvents([Event::Read]);
echo 'Level: ', count($context->wait(Time\Duration::fromSeconds(0))), ' ', count($context->wait(Time\Duration::fromSeconds(0))), "\n";
$watcher->modifyEvents([Event::Read, Event::OneShot]);
$watcher->modifyEvents([Event::Read, Event::Write, Event::OneShot]);
$events = $context->wait(Time\Duration::fromSeconds(0));
echo 'Read+write one-shot: ', count($events), ' ', count($context->wait(Time\Duration::fromSeconds(0))), "\n";
var_dump($events[0] === $watcher);
"#,
    );
    assert_eq!(
        out,
        "Level: 1 1\nOne-shot: 1 0\nbool(true)\nLevel: 1 1\nRead+write one-shot: 1 0\nbool(true)\n"
    );
}

/// php-src `poll_ctx_wait_error.phpt` and `poll_ctx_wait_error_int64.phpt`.
#[test]
fn test_io_poll_wait_validation_php86() {
    let out = run86(
        r#"<?php
$context = new Io\Poll\Context(Io\Poll\Backend::Poll);
try { $context->wait(timeout: Time\Duration::fromSeconds(1)->negate()); }
catch (Throwable $e) { echo $e::class, ': ', $e->getMessage(), "\n"; }
try { $context->wait(maxEvents: PHP_INT_MIN); }
catch (Throwable $e) { echo $e::class, ': ', $e->getMessage(), "\n"; }
try { $context->wait(maxEvents: -1); }
catch (Throwable $e) { echo $e::class, ': ', $e->getMessage(), "\n"; }
try { $context->wait(maxEvents: 0); }
catch (Throwable $e) { echo $e::class, ': ', $e->getMessage(), "\n"; }
try { $context->wait(maxEvents: PHP_INT_MAX); }
catch (Throwable $e) { echo $e::class, ': ', $e->getMessage(), "\n"; }
"#,
    );
    assert_eq!(
        out,
        "ValueError: Io\\Poll\\Context::wait(): Argument #1 ($timeout) must not be negative\nValueError: Io\\Poll\\Context::wait(): Argument #2 ($maxEvents) must be greater than 0\nValueError: Io\\Poll\\Context::wait(): Argument #2 ($maxEvents) must be greater than 0\nValueError: Io\\Poll\\Context::wait(): Argument #2 ($maxEvents) must be greater than 0\nValueError: Io\\Poll\\Context::wait(): Argument #2 ($maxEvents) must be less than or equal to 2147483647\n"
    );
}

/// php-src `poll_clone_not_allowed.phpt` and `poll_double_construct.phpt`.
#[test]
fn test_io_poll_clone_and_construct_guards_php86() {
    let out = run86(
        r#"<?php
[$r, $w] = stream_socket_pair(STREAM_PF_UNIX, STREAM_SOCK_STREAM, 0);
$context = new Io\Poll\Context(Io\Poll\Backend::Poll);
$handle = new StreamPollHandle($r);
$watcher = $context->add($handle, [Io\Poll\Event::Read]);
try { $copyContext = clone $context; } catch (Throwable $e) { echo $e::class, ': ', $e->getMessage(), "\n"; }
try { $copyHandle = clone $handle; } catch (Throwable $e) { echo $e::class, ': ', $e->getMessage(), "\n"; }
try { $copyWatcher = clone $watcher; } catch (Throwable $e) { echo $e::class, ': ', $e->getMessage(), "\n"; }
try { $handle->__construct($r); } catch (Throwable $e) { echo $e::class, ': ', $e->getMessage(), "\n"; }
try { $context->__construct(); } catch (Throwable $e) { echo $e::class, ': ', $e->getMessage(), "\n"; }
"#,
    );
    assert_eq!(
        out,
        "Error: Trying to clone an uncloneable object of class Io\\Poll\\Context\nError: Trying to clone an uncloneable object of class StreamPollHandle\nError: Trying to clone an uncloneable object of class Io\\Poll\\Watcher\nError: StreamPollHandle object is already constructed\nError: Io\\Poll\\Context object is already constructed\n"
    );
}

/// php-src `poll_stream_add_error_duplicate.phpt` and `poll_stream_handle_closed_stream.phpt`.
#[test]
fn test_io_poll_duplicate_and_closed_handle_php86() {
    let out = run86(
        r#"<?php
[$r, $w] = stream_socket_pair(STREAM_PF_UNIX, STREAM_SOCK_STREAM, 0);
$context = new Io\Poll\Context(Io\Poll\Backend::Poll);
$context->add(new StreamPollHandle($w), [Io\Poll\Event::Write], 'one');
try {
    $context->add(new StreamPollHandle($w), [Io\Poll\Event::Write], 'two');
} catch (Throwable $e) {
    echo $e::class, ': ', $e->getMessage(), "\n";
}
$handle = new StreamPollHandle($r);
$watcher = $context->add($handle, [Io\Poll\Event::Read]);
$late = new StreamPollHandle($w);
fclose($r);
fclose($w);
var_dump($handle->isValid());
var_dump(get_debug_type($handle->getStream()));
try { $watcher->modifyEvents([Io\Poll\Event::Write]); }
catch (Throwable $e) { echo $e::class, ': ', $e->getMessage(), "\n"; }
try { $context->add($late, [Io\Poll\Event::Read]); }
catch (Throwable $e) { echo $e::class, ': ', $e->getMessage(), "\n"; }
echo 'Events count: ', count($context->wait(Time\Duration::fromSeconds(0))), "\n";
$watcher->remove();
var_dump($watcher->isActive());
"#,
    );
    assert_eq!(
        out,
        "Io\\Poll\\HandleAlreadyWatchedException: Handle already added\nbool(false)\nstring(17) \"resource (closed)\"\nIo\\Poll\\InactiveWatcherException: Cannot modify inactive watcher\nIo\\Poll\\InvalidHandleException: Invalid handle for polling\nEvents count: 0\nbool(false)\n"
    );
}

/// php-src `poll_stream_closed_watcher_inactive.phpt`.
#[test]
fn test_io_poll_closed_watcher_inactive_php86() {
    let out = run86(
        r#"<?php
[$r, $w] = stream_socket_pair(STREAM_PF_UNIX, STREAM_SOCK_STREAM, 0);
$context = new Io\Poll\Context(Io\Poll\Backend::Poll);
$watcher = $context->add(new StreamPollHandle($r), [Io\Poll\Event::Read]);
fwrite($w, 'ping');
var_dump($watcher->isActive());
fclose($r);
var_dump($watcher->isActive());
echo 'Events count: ', count($context->wait(Time\Duration::fromSeconds(0))), "\n";
$watcher->remove();
$watcher->remove();
echo "removed\n";
var_dump($watcher->isActive());
try { $watcher->modifyEvents([Io\Poll\Event::Write]); }
catch (Throwable $e) { echo $e::class, ': ', $e->getMessage(), "\n"; }
"#,
    );
    assert_eq!(
        out,
        "bool(true)\nbool(false)\nEvents count: 0\nremoved\nbool(false)\nIo\\Poll\\InactiveWatcherException: Cannot modify inactive watcher\n"
    );
}

/// php-src `poll_watcher_outlives_context.phpt`.
#[test]
fn test_io_poll_watcher_outlives_context_php86() {
    let out = run86(
        r#"<?php
[$r, $w] = stream_socket_pair(STREAM_PF_UNIX, STREAM_SOCK_STREAM, 0);
$context = new Io\Poll\Context(Io\Poll\Backend::Poll);
$watcher = $context->add(new StreamPollHandle($r), [Io\Poll\Event::Read], 'data');
unset($context);
gc_collect_cycles();
var_dump($watcher->isActive());
try { $watcher->remove(); }
catch (Throwable $e) { echo $e::class, ': ', $e->getMessage(), "\n"; }
try { $watcher->modifyEvents([Io\Poll\Event::Write]); }
catch (Throwable $e) { echo $e::class, ': ', $e->getMessage(), "\n"; }
echo "done\n";
"#,
    );
    assert_eq!(
        out,
        "bool(false)\nIo\\Poll\\InactiveWatcherException: Cannot remove inactive watcher\nIo\\Poll\\InactiveWatcherException: Cannot modify inactive watcher\ndone\n"
    );
}

/// php-src `poll_stream_closed_fd_reuse.phpt`.
#[test]
fn test_io_poll_closed_fd_reuse_php86() {
    let out = run86(
        r#"<?php
$context = new Io\Poll\Context(Io\Poll\Backend::Poll);
[$r0, $w0] = stream_socket_pair(STREAM_PF_UNIX, STREAM_SOCK_STREAM, 0);
$old = $context->add(new StreamPollHandle($r0), [Io\Poll\Event::Read], 'old');
fclose($r0);
[$r1, $w1] = stream_socket_pair(STREAM_PF_UNIX, STREAM_SOCK_STREAM, 0);
$new = $context->add(new StreamPollHandle($r1), [Io\Poll\Event::Read], 'new');
var_dump($old->isActive(), $new->isActive());
fwrite($w1, 'ping');
$events = $context->wait(Time\Duration::fromMicroseconds(100000));
var_dump(count($events), $events[0] === $new, $events[0]->getData());
$old->remove();
echo "old removed\n";
$new->remove();
var_dump($new->isActive());
echo 'Events count: ', count($context->wait(Time\Duration::fromSeconds(0))), "\n";
"#,
    );
    assert_eq!(
        out,
        "bool(false)\nbool(true)\nint(1)\nbool(true)\nstring(3) \"new\"\nold removed\nbool(false)\nEvents count: 0\n"
    );
}

/// php-src `poll_stream_closed_multi_context.phpt`.
#[test]
fn test_io_poll_closed_multi_context_php86() {
    let out = run86(
        r#"<?php
[$r, $w] = stream_socket_pair(STREAM_PF_UNIX, STREAM_SOCK_STREAM, 0);
$handle = new StreamPollHandle($r);
$ctx1 = new Io\Poll\Context(Io\Poll\Backend::Poll);
$ctx2 = new Io\Poll\Context(Io\Poll\Backend::Poll);
$w1 = $ctx1->add($handle, [Io\Poll\Event::Read]);
$w2 = $ctx2->add($handle, [Io\Poll\Event::Read]);
fwrite($w, 'ping');
fclose($r);
echo 'ctx1 events count: ', count($ctx1->wait(Time\Duration::fromSeconds(0))), "\n";
echo 'ctx2 events count: ', count($ctx2->wait(Time\Duration::fromSeconds(0))), "\n";
var_dump($w1->isActive(), $w2->isActive());
$w1->remove();
$w2->remove();
echo "removed\n";
"#,
    );
    assert_eq!(
        out,
        "ctx1 events count: 0\nctx2 events count: 0\nbool(false)\nbool(false)\nremoved\n"
    );
}

/// php-src `poll_stream_closed_many.phpt`.
#[test]
fn test_io_poll_closed_many_php86() {
    let out = run86(
        r#"<?php
$context = new Io\Poll\Context(Io\Poll\Backend::Poll);
[$rk, $wk] = stream_socket_pair(STREAM_PF_UNIX, STREAM_SOCK_STREAM, 0);
$keep = $context->add(new StreamPollHandle($rk), [Io\Poll\Event::Read], 'keep');
[$ro, $wo] = stream_socket_pair(STREAM_PF_UNIX, STREAM_SOCK_STREAM, 0);
$oneshot = $context->add(new StreamPollHandle($ro), [Io\Poll\Event::Read, Io\Poll\Event::OneShot], 'oneshot');
fwrite($wo, 'x');
$events = $context->wait(Time\Duration::fromMicroseconds(100000));
var_dump(count($events), $events[0]->getData());
for ($i = 0; $i < 200; $i++) {
    [$rr, $ww] = stream_socket_pair(STREAM_PF_UNIX, STREAM_SOCK_STREAM, 0);
    $watcher = $context->add(new StreamPollHandle($rr), [Io\Poll\Event::Read]);
    fclose($rr);
    $watcher->remove();
    fclose($ww);
}
fwrite($wk, 'y');
$events = $context->wait(Time\Duration::fromMicroseconds(100000));
var_dump(count($events), $events[0]->getData());
var_dump($keep->isActive(), $oneshot->isActive());
$oneshot->modifyEvents([Io\Poll\Event::Read, Io\Poll\Event::OneShot]);
$events = $context->wait(Time\Duration::fromMicroseconds(100000));
var_dump(count($events));
[$r, $w] = stream_socket_pair(STREAM_PF_UNIX, STREAM_SOCK_STREAM, 0);
$new = $context->add(new StreamPollHandle($r), [Io\Poll\Event::Read], 'new');
fwrite($w, 'z');
$events = $context->wait(Time\Duration::fromMicroseconds(100000));
var_dump(count($events));
var_dump($oneshot->isActive());
"#,
    );
    assert_eq!(
        out,
        "int(1)\nstring(7) \"oneshot\"\nint(1)\nstring(4) \"keep\"\nbool(true)\nbool(true)\nint(2)\nint(2)\nbool(true)\n"
    );
}

/// php-src `poll_stream_tcp_read_max_events.phpt`.
#[test]
fn test_io_poll_max_events_php86() {
    let out = run86(
        r#"<?php
function connections(int $count): array {
    $server = stream_socket_server('tcp://127.0.0.1:0');
    $address = stream_socket_get_name($server, false);
    $clients = []; $servers = [];
    for ($i = 0; $i < $count; $i++) {
        $clients[$i] = stream_socket_client('tcp://' . $address);
        $servers[$i] = stream_socket_accept($server);
    }
    fclose($server);
    return [$clients, $servers];
}
[$clients, $servers] = connections(8);
$context = new Io\Poll\Context(Io\Poll\Backend::Poll);
for ($i = 0; $i < 8; $i++) {
    $context->add(new StreamPollHandle($servers[$i]), [Io\Poll\Event::Read], "server$i");
}
for ($i = 0; $i < 8; $i++) {
    fwrite($clients[$i], "test $i data");
    usleep(10000);
}
$seen = [];
for ($round = 1; $round <= 2; $round++) {
    $watchers = $context->wait(Time\Duration::fromMicroseconds(100000), 4);
    echo "Round $round count: ", count($watchers), "\n";
    foreach ($watchers as $watcher) {
        $data = $watcher->getData();
        if (isset($seen[$data])) { echo "Duplicate event for $data\n"; }
        $seen[$data] = true;
        fread($watcher->getHandle()->getStream(), 1024);
    }
}
ksort($seen);
echo 'Seen: ', implode(',', array_keys($seen)), "\n";
echo 'After: ', count($context->wait(Time\Duration::fromSeconds(0))), "\n";
"#,
    );
    assert_eq!(
        out,
        "Round 1 count: 4\nRound 2 count: 4\nSeen: server0,server1,server2,server3,server4,server5,server6,server7\nAfter: 0\n"
    );
}

/// php-src `poll_stream_wait_no_add.phpt`.
#[test]
fn test_io_poll_wait_without_watchers_php86() {
    let out = run86(
        r#"<?php
$context = new Io\Poll\Context(Io\Poll\Backend::Poll);
echo 'Events count: ', count($context->wait(Time\Duration::fromMicroseconds(100000))), "\n";
"#,
    );
    assert_eq!(out, "Events count: 0\n");
}

/// php-src `poll_stream_tcp_read_one_shot_max_events.phpt`: unreturned one-shots stay armed.
#[test]
fn test_io_poll_one_shot_max_events_php86() {
    let out = run86(
        r#"<?php
function connections(int $count): array {
    $server = stream_socket_server('tcp://127.0.0.1:0');
    $address = stream_socket_get_name($server, false);
    $clients = []; $servers = [];
    for ($i = 0; $i < $count; $i++) {
        $clients[$i] = stream_socket_client('tcp://' . $address);
        $servers[$i] = stream_socket_accept($server);
    }
    fclose($server);
    return [$clients, $servers];
}
[$clients, $servers] = connections(8);
$context = new Io\Poll\Context(Io\Poll\Backend::Poll);
for ($i = 0; $i < 8; $i++) {
    $context->add(new StreamPollHandle($servers[$i]), [Io\Poll\Event::Read, Io\Poll\Event::OneShot], "server$i");
}
for ($i = 0; $i < 8; $i++) {
    fwrite($clients[$i], "test $i data");
    usleep(10000);
}
$seen = [];
for ($round = 1; $round <= 2; $round++) {
    $watchers = $context->wait(Time\Duration::fromMicroseconds(100000), 4);
    echo "Round $round count: ", count($watchers), "\n";
    foreach ($watchers as $watcher) {
        $data = $watcher->getData();
        if (isset($seen[$data])) { echo "Duplicate event for $data\n"; }
        $seen[$data] = true;
    }
}
ksort($seen);
echo 'Seen: ', implode(',', array_keys($seen)), "\n";
fwrite($clients[0], 'more data');
echo 'After: ', count($context->wait(Time\Duration::fromMicroseconds(100000))), "\n";
"#,
    );
    assert_eq!(
        out,
        "Round 1 count: 4\nRound 2 count: 4\nSeen: server0,server1,server2,server3,server4,server5,server6,server7\nAfter: 0\n"
    );
}
