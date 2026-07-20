<?php
// Runtime session configuration via ini_set()/ini_get() (scoped to session.*).
// Extend the session lifetime to one hour before the session starts.
ini_set('session.gc_maxlifetime', '3600');
// Compile with --php-version=8.2, 8.3, 8.4, or 8.5 (8.5 is the default).
// Strict mode is opt-in in every maintained PHP version; lazy write defaults on.
// Unchanged requests touch the file timestamp without rewriting its payload.
ini_set('session.use_strict_mode', '1');
ini_set('session.lazy_write', '1');

session_start();

if (!isset($_SESSION['count'])) {
    $_SESSION['count'] = 0;
}
$_SESSION['count']++;

header('Content-Type: text/plain');
echo "You have visited this page " . $_SESSION['count'] . " time(s).\n";
echo "Your session ID is " . session_id() . "\n";
echo "Session lifetime is " . ini_get('session.gc_maxlifetime') . " seconds.\n";
