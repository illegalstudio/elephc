<?php
// Display helpers — listing and detail views

function cmd_list_contacts($contacts) {
    if (count($contacts) === 0) {
        echo "  (no contacts)\n";
        return;
    }
    echo "\n";
    $header = str_pad("#", 4, " ") . str_pad("Name", 20, " ") . str_pad("Email", 25, " ") . "Phone";
    echo $header . "\n";
    echo str_repeat("-", strlen($header) + 5) . "\n";
    for ($i = 0; $i < count($contacts); $i++) {
        $c = $contacts[$i];
        $line = str_pad((string)($i + 1), 4, " ");
        $line .= str_pad($c["name"], 20, " ");
        $line .= str_pad($c["email"], 25, " ");
        $line .= $c["phone"];
        echo $line . "\n";
    }
}

function cmd_view_contact($contacts) {
    if (count($contacts) === 0) {
        echo "  (no contacts)\n";
        return;
    }
    cmd_list_contacts($contacts);
    $input = trim(readline("View #: "));
    $idx = intval($input) - 1;
    if ($idx < 0 || $idx >= count($contacts)) {
        echo "Invalid number.\n";
        return;
    }
    $c = $contacts[$idx];
    echo "\n--- " . $c["name"] . " ---\n";
    echo "  Email:   " . $c["email"] . "\n";
    echo "  Phone:   " . $c["phone"] . "\n";
    echo "  Notes:   " . $c["notes"] . "\n";
    echo "  Added:   " . $c["created"] . "\n";
}
