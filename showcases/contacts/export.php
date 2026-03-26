<?php
// CSV export/import

function cmd_export_csv($contacts, $filename) {
    if (count($contacts) === 0) {
        echo "No contacts to export.\n";
        return;
    }
    if (strlen($filename) === 0) {
        $filename = "contacts.csv";
    }

    $fp = fopen($filename, "w");
    fputcsv($fp, ["Name", "Email", "Phone", "Notes", "Created"]);
    for ($i = 0; $i < count($contacts); $i++) {
        $c = $contacts[$i];
        fputcsv($fp, [$c["name"], $c["email"], $c["phone"], $c["notes"], $c["created"]]);
    }
    fclose($fp);
    echo "Exported " . count($contacts) . " contacts to " . $filename . "\n";
}

function cmd_import_csv($contacts, $filename) {
    if (!file_exists($filename)) {
        echo "File not found: " . $filename . "\n";
        return $contacts;
    }

    $fp = fopen($filename, "r");
    $header = fgetcsv($fp);
    $imported = 0;

    while (!feof($fp)) {
        $row = fgetcsv($fp);
        if ($row === false) {
            break;
        }
        if (count($row) < 4) {
            continue;
        }
        $contacts[] = make_contact($row[0], $row[1], $row[2], $row[3]);
        $imported++;
    }
    fclose($fp);
    echo "Imported " . $imported . " contacts from " . $filename . "\n";
    return $contacts;
}
