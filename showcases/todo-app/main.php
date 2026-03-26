<?php
// Todo App — interactive CLI task manager with file persistence
// Usage: elephc showcases/todo-app/main.php && ./showcases/todo-app/main

require_once 'storage.php';
require_once 'ui.php';
require_once 'commands.php';

$todo_file = "todos.txt";
$todos = load_todos($todo_file);

print_banner();

$running = true;
while ($running) {
    echo "\n";
    print_menu();
    $choice = trim(readline("> "));

    if ($choice === "1") {
        $title = trim(readline("Task: "));
        $pri = trim(readline("Priority (high/medium/low): "));
        $todos = cmd_add($todos, $title, $pri);
    } elseif ($choice === "2") {
        print_todo_list($todos, "pending");
        $input = trim(readline("Complete #: "));
        $todos = cmd_complete($todos, $input);
    } elseif ($choice === "3") {
        cmd_list($todos, "all");
    } elseif ($choice === "4") {
        cmd_list($todos, "pending");
    } elseif ($choice === "5") {
        cmd_list($todos, "done");
    } elseif ($choice === "6") {
        print_todo_list($todos, "all");
        $input = trim(readline("Remove #: "));
        $todos = cmd_remove($todos, $input);
    } elseif ($choice === "7") {
        cmd_stats($todos);
    } elseif ($choice === "q" || $choice === "Q") {
        $running = false;
    } else {
        echo "Unknown option.\n";
    }

    save_todos($todo_file, $todos);
}

echo "Bye!\n";
