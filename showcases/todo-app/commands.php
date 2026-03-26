<?php
// Command handlers
// Note: readline calls must happen at top level (main.php),
// values are passed as parameters to avoid a runtime bug with
// multiple readline() calls inside functions.

function cmd_add($todos, $title, $pri) {
    if (strlen($title) === 0) {
        echo "Empty title, skipped.\n";
        return $todos;
    }
    if ($pri !== "high" && $pri !== "medium" && $pri !== "low") {
        $pri = "low";
    }
    $todos[] = make_todo($title, $pri);
    echo "Added: " . $title . "\n";
    return $todos;
}

function cmd_complete($todos, $input) {
    $n = print_todo_list($todos, "pending");
    if ($n === 0) {
        return $todos;
    }
    $idx = intval($input) - 1;
    if ($idx < 0 || $idx >= count($todos)) {
        echo "Invalid number.\n";
        return $todos;
    }
    $item = $todos[$idx];
    if ($item["done"] === "1") {
        echo "Already done.\n";
        return $todos;
    }
    $item["done"] = "1";
    $todos[$idx] = $item;
    echo "Completed: " . $item["title"] . "\n";
    return $todos;
}

function cmd_list($todos, $filter) {
    $labels = [
        "all" => "All tasks",
        "pending" => "Pending tasks",
        "done" => "Completed tasks",
    ];
    echo $labels[$filter] . ":\n";
    print_todo_list($todos, $filter);
}

function cmd_remove($todos, $input) {
    print_todo_list($todos, "all");
    $idx = intval($input) - 1;
    if ($idx < 0 || $idx >= count($todos)) {
        echo "Invalid number.\n";
        return $todos;
    }
    $item = $todos[$idx];
    $title = $item["title"];
    array_splice($todos, $idx, 1);
    echo "Removed: " . $title . "\n";
    return $todos;
}

function cmd_stats($todos) {
    $total = count($todos);
    $done = 0;
    $high = 0;
    for ($i = 0; $i < $total; $i++) {
        $item = $todos[$i];
        if ($item["done"] === "1") {
            $done++;
        }
        if ($item["priority"] === "high" && $item["done"] === "0") {
            $high++;
        }
    }
    $pending = $total - $done;
    echo "Total: " . $total . "\n";
    echo "Done: " . $done . "\n";
    echo "Pending: " . $pending . "\n";
    if ($total > 0) {
        $pct = round(($done / $total) * 100, 1);
        echo "Progress: " . $pct . "%\n";
    }
    if ($high > 0) {
        echo "High priority pending: " . $high . "\n";
    }
}
