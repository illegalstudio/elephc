<?php
$score = 10;

function apply_bonus() {
    global $score;

    if ($score < 0) {
        echo "unexpected\n";
    }

    eval('global $score; $score = $score + 5;');
}

// Superglobals need no `global` statement, inside eval as anywhere else.
function describe_run() {
    eval('echo "arguments=" . $_SERVER["argc"] . "\n";');
}

apply_bonus();
echo "score=" . $score . "\n";
describe_run();
