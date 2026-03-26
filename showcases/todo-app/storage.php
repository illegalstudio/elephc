<?php
// Persistence layer — load/save todos as pipe-delimited text
// Format: title|done|priority|created

function load_todos($path) {
    if (!file_exists($path)) {
        file_put_contents($path, "");
    }
    $todos = [];
    $lines = file($path);
    for ($i = 0; $i < count($lines); $i++) {
        $line = trim($lines[$i]);
        if (strlen($line) === 0) {
            continue;
        }
        $parts = explode("|", $line);
        if (count($parts) >= 4) {
            $todos[] = [
                "title" => $parts[0],
                "done" => $parts[1],
                "priority" => $parts[2],
                "created" => $parts[3],
            ];
        }
    }
    return $todos;
}

function save_todos($path, $todos) {
    $content = "";
    for ($i = 0; $i < count($todos); $i++) {
        $t = $todos[$i];
        $content .= $t["title"] . "|" . $t["done"] . "|" . $t["priority"] . "|" . $t["created"] . "\n";
    }
    file_put_contents($path, $content);
}

function make_todo($title, $priority) {
    return [
        "title" => $title,
        "done" => "0",
        "priority" => $priority,
        "created" => date("Y-m-d H:i:s"),
    ];
}
