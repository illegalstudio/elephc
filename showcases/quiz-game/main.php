<?php
// Quiz Game — trivia with scoring, timer, and high scores
// Usage: elephc showcases/quiz-game/main.php && ./showcases/quiz-game/main

require_once 'questions.php';
require_once 'game.php';
require_once 'scores.php';

echo "================================\n";
echo "    TRIVIA QUIZ GAME (elephc)   \n";
echo "================================\n\n";

$scores_file = "highscores.txt";
$high_scores = load_scores($scores_file);

$running = true;
while ($running) {
    echo "[1] Play\n";
    echo "[2] High scores\n";
    echo "[3] Rules\n";
    echo "[q] Quit\n";
    $choice = trim(readline("> "));

    if ($choice === "1") {
        $name = trim(readline("Your name: "));
        if (strlen($name) === 0) {
            $name = "Player";
        }
        $questions = get_questions();
        shuffle($questions);
        $result = play_round($questions, 5);
        echo "\nGame over, " . $name . "!\n";
        echo "Score: " . $result["score"] . "/" . $result["total"] . "\n";
        echo "Time: " . $result["time"] . "s\n";
        $high_scores = add_score($high_scores, $name, intval($result["score"]), intval($result["total"]), intval($result["time"]));
        save_scores($scores_file, $high_scores);
    } elseif ($choice === "2") {
        show_scores($high_scores);
    } elseif ($choice === "3") {
        echo "\nYou get 5 random questions. Each has 4 options.\n";
        echo "Answer with the letter (a/b/c/d).\n\n";
    } elseif ($choice === "q" || $choice === "Q") {
        $running = false;
    } else {
        echo "Unknown option.\n";
    }
}

echo "Thanks for playing!\n";
