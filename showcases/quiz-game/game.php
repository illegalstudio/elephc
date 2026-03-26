<?php
// Game logic — run a quiz round
// Each question: [text, optA, optB, optC, optD, answer]
// Note: only one readline per iteration to avoid runtime bug

function play_round($questions, $num_questions) {
    $score = 0;
    $total = min($num_questions, count($questions));
    $start_time = time();
    $labels = ["a", "b", "c", "d"];

    for ($i = 0; $i < $total; $i++) {
        $q = $questions[$i];
        echo "\nQuestion " . ($i + 1) . "/" . $total . ":\n";
        echo "  " . $q[0] . "\n";

        for ($j = 0; $j < 4; $j++) {
            echo "    " . $labels[$j] . ") " . $q[$j + 1] . "\n";
        }

        $answer = strtolower(trim(readline("Answer: ")));
        $correct = $q[5];

        if ($answer === $correct) {
            echo "  Correct!\n";
            $score++;
        } else {
            $correct_idx = index_of($labels, $correct);
            echo "  Wrong! The answer was " . $correct . ") " . $q[$correct_idx + 1] . "\n";
        }
    }

    $elapsed = time() - $start_time;

    return [
        "score" => (string)$score,
        "total" => (string)$total,
        "time" => (string)$elapsed,
    ];
}

function index_of($arr, $val) {
    for ($i = 0; $i < count($arr); $i++) {
        if ($arr[$i] === $val) {
            return $i;
        }
    }
    return 0;
}
