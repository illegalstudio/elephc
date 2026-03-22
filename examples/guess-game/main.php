<?php
// A simple number guessing game logic demo
// (no stdin yet, so we simulate guesses)

function check_guess($guess, $target) {
    if ($guess == $target) {
        echo "Correct! The number was " . $target . "\n";
        return 1;
    }
    if ($guess < $target) {
        echo "Too low! (guessed " . $guess . ")\n";
    } else {
        echo "Too high! (guessed " . $guess . ")\n";
    }
    return 0;
}

$target = 42;

echo "Guess the number between 1 and 100\n";
check_guess(25, $target);
check_guess(50, $target);
check_guess(37, $target);
check_guess(43, $target);
check_guess(42, $target);
