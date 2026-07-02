<?php
session_start();

if (!isset($_SESSION['count'])) {
    $_SESSION['count'] = 0;
}
$_SESSION['count']++;

header('Content-Type: text/plain');
echo "You have visited this page " . $_SESSION['count'] . " time(s).\n";
echo "Your session ID is " . session_id() . "\n";
