<?php
// High score management with file persistence
// Format: name|score|total|time|date

function load_scores($path) {
    if (!file_exists($path)) {
        file_put_contents($path, "");
    }
    $scores = [];
    $lines = file($path);
    for ($i = 0; $i < count($lines); $i++) {
        $line = trim($lines[$i]);
        if (strlen($line) === 0) {
            continue;
        }
        $parts = explode("|", $line);
        if (count($parts) >= 5) {
            $scores[] = [
                "name" => $parts[0],
                "score" => $parts[1],
                "total" => $parts[2],
                "time" => $parts[3],
                "date" => $parts[4],
            ];
        }
    }
    return $scores;
}

function save_scores($path, $scores) {
    $content = "";
    for ($i = 0; $i < count($scores); $i++) {
        $s = $scores[$i];
        $content .= $s["name"] . "|" . $s["score"] . "|" . $s["total"] . "|" . $s["time"] . "|" . $s["date"] . "\n";
    }
    file_put_contents($path, $content);
}

function add_score($scores, $name, $score, $total, $time) {
    $entry = [
        "name" => $name,
        "score" => (string)$score,
        "total" => (string)$total,
        "time" => (string)$time,
        "date" => date("Y-m-d"),
    ];
    $scores[] = $entry;

    // Sort by score descending (simple bubble sort)
    for ($i = 0; $i < count($scores) - 1; $i++) {
        for ($j = 0; $j < count($scores) - $i - 1; $j++) {
            $a = $scores[$j];
            $b = $scores[$j + 1];
            if (intval($a["score"]) < intval($b["score"])) {
                $scores[$j] = $b;
                $scores[$j + 1] = $a;
            }
        }
    }

    // Keep top 10
    if (count($scores) > 10) {
        $scores = array_slice($scores, 0, 10);
    }

    return $scores;
}

function show_scores($scores) {
    echo "\n--- HIGH SCORES ---\n";
    if (count($scores) === 0) {
        echo "  No scores yet.\n";
        return;
    }
    for ($i = 0; $i < count($scores); $i++) {
        $s = $scores[$i];
        $rank = str_pad((string)($i + 1), 2, " ");
        echo "  " . $rank . ". " . str_pad($s["name"], 12, " ") . " ";
        echo $s["score"] . "/" . $s["total"];
        echo "  (" . $s["time"] . "s)";
        echo "  " . $s["date"] . "\n";
    }
    echo "\n";
}
