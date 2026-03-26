<?php
// Contact storage — pipe-delimited file persistence
// Format: name|email|phone|notes|created

function db_load($path) {
    if (!file_exists($path)) {
        file_put_contents($path, "");
    }
    $contacts = [];
    $lines = file($path);
    for ($i = 0; $i < count($lines); $i++) {
        $line = trim($lines[$i]);
        if (strlen($line) === 0) {
            continue;
        }
        $parts = explode("|", $line);
        if (count($parts) >= 5) {
            $contacts[] = [
                "name" => $parts[0],
                "email" => $parts[1],
                "phone" => $parts[2],
                "notes" => $parts[3],
                "created" => $parts[4],
            ];
        }
    }
    return $contacts;
}

function db_save($path, $contacts) {
    $content = "";
    for ($i = 0; $i < count($contacts); $i++) {
        $c = $contacts[$i];
        $content .= $c["name"] . "|" . $c["email"] . "|" . $c["phone"] . "|" . $c["notes"] . "|" . $c["created"] . "\n";
    }
    file_put_contents($path, $content);
}

function make_contact($name, $email, $phone, $notes) {
    return [
        "name" => $name,
        "email" => $email,
        "phone" => $phone,
        "notes" => $notes,
        "created" => date("Y-m-d"),
    ];
}

function cmd_delete_contact($contacts, $input) {
    $idx = intval($input) - 1;
    if ($idx < 0 || $idx >= count($contacts)) {
        echo "Invalid number.\n";
        return $contacts;
    }
    $c = $contacts[$idx];
    $name = $c["name"];
    array_splice($contacts, $idx, 1);
    echo "Deleted: " . $name . "\n";
    return $contacts;
}
