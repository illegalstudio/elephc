<?php
// Search — find contacts by name, email, or phone

function cmd_search($contacts, $query) {
    if (strlen($query) === 0) {
        echo "Empty query.\n";
        return;
    }
    $query_lower = strtolower($query);
    $found = 0;

    echo "Results for \"" . $query . "\":\n";
    for ($i = 0; $i < count($contacts); $i++) {
        $c = $contacts[$i];
        $match = false;
        if (str_contains(strtolower($c["name"]), $query_lower)) {
            $match = true;
        }
        if (str_contains(strtolower($c["email"]), $query_lower)) {
            $match = true;
        }
        if (str_contains($c["phone"], $query)) {
            $match = true;
        }
        if ($match) {
            $found++;
            echo "  " . $found . ". " . $c["name"];
            if (strlen($c["email"]) > 0) {
                echo " <" . $c["email"] . ">";
            }
            echo "\n";
        }
    }
    if ($found === 0) {
        echo "  No matches.\n";
    }
}
