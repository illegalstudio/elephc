<?php
// Contacts — address book CLI with search, CSV export, and file persistence
// Usage: elephc showcases/contacts/main.php && ./showcases/contacts/main

require_once 'db.php';
require_once 'display.php';
require_once 'search.php';
require_once 'export.php';

$db_file = "contacts.txt";
$contacts = db_load($db_file);

echo "=================================\n";
echo "    ADDRESS BOOK (elephc)        \n";
echo "=================================\n";
echo count($contacts) . " contacts loaded.\n";

$running = true;
while ($running) {
    echo "\n";
    echo "[1] Add contact     [5] Search\n";
    echo "[2] List all        [6] Export CSV\n";
    echo "[3] View detail     [7] Import CSV\n";
    echo "[4] Edit contact    [8] Delete\n";
    echo "[q] Quit\n";
    $choice = trim(readline("> "));

    if ($choice === "1") {
        $name = trim(readline("Name: "));
        $email = trim(readline("Email: "));
        $phone = trim(readline("Phone: "));
        $notes = trim(readline("Notes: "));
        if (strlen($name) === 0) {
            echo "Name is required.\n";
        } else {
            $contacts[] = make_contact($name, $email, $phone, $notes);
            echo "Added: " . $name . "\n";
        }
    } elseif ($choice === "2") {
        cmd_list_contacts($contacts);
    } elseif ($choice === "3") {
        cmd_view_contact($contacts);
    } elseif ($choice === "4") {
        cmd_list_contacts($contacts);
        $input = trim(readline("Edit #: "));
        $idx = intval($input) - 1;
        if ($idx >= 0 && $idx < count($contacts)) {
            $c = $contacts[$idx];
            echo "Editing: " . $c["name"] . "\n";
            $name = trim(readline("Name [" . $c["name"] . "]: "));
            if (strlen($name) > 0) {
                $c["name"] = $name;
            }
            $email = trim(readline("Email [" . $c["email"] . "]: "));
            if (strlen($email) > 0) {
                $c["email"] = $email;
            }
            $phone = trim(readline("Phone [" . $c["phone"] . "]: "));
            if (strlen($phone) > 0) {
                $c["phone"] = $phone;
            }
            $notes = trim(readline("Notes [" . $c["notes"] . "]: "));
            if (strlen($notes) > 0) {
                $c["notes"] = $notes;
            }
            $contacts[$idx] = $c;
            echo "Updated.\n";
        } else {
            echo "Invalid number.\n";
        }
    } elseif ($choice === "5") {
        $query = trim(readline("Search: "));
        cmd_search($contacts, $query);
    } elseif ($choice === "6") {
        $filename = trim(readline("Export file [contacts.csv]: "));
        cmd_export_csv($contacts, $filename);
    } elseif ($choice === "7") {
        $filename = trim(readline("CSV file to import: "));
        $contacts = cmd_import_csv($contacts, $filename);
    } elseif ($choice === "8") {
        cmd_list_contacts($contacts);
        $input = trim(readline("Delete #: "));
        $contacts = cmd_delete_contact($contacts, $input);
    } elseif ($choice === "q" || $choice === "Q") {
        $running = false;
    } else {
        echo "Unknown option.\n";
    }

    db_save($db_file, $contacts);
}

echo "Goodbye!\n";
