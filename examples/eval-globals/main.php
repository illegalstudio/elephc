<?php
$score = 10;

function apply_bonus() {
    global $score;

    if ($score < 0) {
        echo "unexpected\n";
    }

    eval('global $score; $score = $score + 5;');
}

apply_bonus();
echo "score=" . $score . "\n";
