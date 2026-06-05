<?php

// PDO with the SQLite driver: a small in-memory address book.
// SQLite is statically bundled, so this compiles to a standalone binary with
// no database to install.

$db = new PDO("sqlite::memory:");

$db->exec("CREATE TABLE contacts (
    id    INTEGER PRIMARY KEY,
    name  TEXT NOT NULL,
    email TEXT,
    score REAL
)");

// Prepared statement with named placeholders, reused for several inserts.
$insert = $db->prepare(
    "INSERT INTO contacts (name, email, score) VALUES (:name, :email, :score)"
);
$insert->execute([":name" => "Ada Lovelace",  ":email" => "ada@example.com",  ":score" => 9.5]);
$insert->execute([":name" => "Alan Turing",   ":email" => "alan@example.com", ":score" => 9.0]);
$insert->execute([":name" => "Grace Hopper",  ":email" => "grace@example.com", ":score" => 9.8]);

echo "Inserted, last id = " . $db->lastInsertId() . "\n\n";

// Positional bind: look up one contact.
$one = $db->prepare("SELECT name, score FROM contacts WHERE id = ?");
$one->execute([2]);
$row = $one->fetch(PDO::FETCH_ASSOC);
echo "Contact #2: " . $row["name"] . " (" . $row["score"] . ")\n\n";

// A PDOStatement is Traversable: foreach walks the result set directly,
// streaming one row at a time in the statement's fetch mode.
echo "Leaderboard:\n";
$stmt = $db->query("SELECT name, score FROM contacts ORDER BY score DESC");
$stmt->setFetchMode(PDO::FETCH_ASSOC);
foreach ($stmt as $rank => $contact) {
    echo "  " . ($rank + 1) . ". " . $contact["name"] . " — " . $contact["score"] . "\n";
}
echo "\n";

// A transaction that we roll back, then one we commit.
$db->beginTransaction();
$db->exec("DELETE FROM contacts");
$db->rollBack();
echo "After rollback: " . $db->query("SELECT COUNT(*) FROM contacts")->fetchColumn() . " contacts\n";

// Error handling.
try {
    $db->exec("SELECT * FROM table_that_does_not_exist");
} catch (PDOException $e) {
    echo "Caught expected error: " . $e->getMessage() . "\n";
}
