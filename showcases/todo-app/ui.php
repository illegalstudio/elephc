<?php
// UI helpers — display formatting

function print_banner() {
    echo "=============================\n";
    echo "      TODO APP (elephc)      \n";
    echo "=============================\n";
}

function print_menu() {
    echo "[1] Add task\n";
    echo "[2] Complete task\n";
    echo "[3] List all\n";
    echo "[4] List pending\n";
    echo "[5] List completed\n";
    echo "[6] Remove task\n";
    echo "[7] Stats\n";
    echo "[q] Quit\n";
}

function print_todo($index, $todo) {
    $status = $todo["done"] === "1" ? "[x]" : "[ ]";
    $pri = $todo["priority"];
    $label = "";
    if ($pri === "high") {
        $label = " !!";
    } elseif ($pri === "medium") {
        $label = " !";
    }
    echo "  " . ($index + 1) . ". " . $status . " " . $todo["title"] . $label . "\n";
}

function print_todo_list($todos, $filter) {
    $count = 0;
    for ($i = 0; $i < count($todos); $i++) {
        $item = $todos[$i];
        $show = false;
        if ($filter === "all") {
            $show = true;
        } elseif ($filter === "pending" && $item["done"] === "0") {
            $show = true;
        } elseif ($filter === "done" && $item["done"] === "1") {
            $show = true;
        }
        if ($show) {
            print_todo($i, $item);
            $count++;
        }
    }
    if ($count === 0) {
        echo "  (no tasks)\n";
    }
    return $count;
}
