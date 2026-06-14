<?php

// PDO with the SQLite driver: a small in-memory address book.
// SQLite is statically bundled, so this compiles to a standalone binary with
// no database to install.

$db = new PDO("sqlite::memory:", null, null, [PDO::ATTR_PERSISTENT => true]);
echo "Persistent option = " . $db->getAttribute(PDO::ATTR_PERSISTENT) . "\n\n";

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

$shared = new PDO("sqlite::memory:", null, null, [PDO::ATTR_PERSISTENT => true]);
echo "Persistent shared count: " . $shared->query("SELECT COUNT(*) FROM contacts")->fetchColumn() . "\n\n";

// Positional bind: look up one contact.
$one = $db->prepare("SELECT name, score FROM contacts WHERE id = ?");
$one->execute([2]);
$row = $one->fetch(PDO::FETCH_ASSOC);
echo "Contact #2: " . $row["name"] . " (" . $row["score"] . ")\n\n";

// FETCH_OBJ creates a real stdClass with dynamic properties.
$object = $db->query("SELECT id, name FROM contacts WHERE id = 1")->fetch(PDO::FETCH_OBJ);
echo "Object fetch: " . gettype($object) . " #" . $object->id . " " . $object->name . "\n\n";

class ContactRow {
    public mixed $id;
    public mixed $name;
}

// FETCH_CLASS creates the requested row class and assigns columns directly.
$classRow = $db->query("SELECT id, name FROM contacts WHERE id = 3")->fetch(PDO::FETCH_CLASS, ContactRow::class);
echo "Class fetch: " . (($classRow instanceof ContactRow) ? "ContactRow" : "other") . " #" . $classRow->id . "\n";

// FETCH_INTO fills and returns an existing object.
$into = new ContactRow();
$same = $db->query("SELECT id, name FROM contacts WHERE id = 2")->fetch(PDO::FETCH_INTO, $into);
echo "Into fetch: " . (($same === $into) ? "same" : "different") . " #" . $into->id . "\n\n";

// Binary values preserve embedded NUL bytes.
$db->exec("CREATE TABLE blobs (payload BLOB)");
$db->exec("INSERT INTO blobs VALUES (X'410042')");
$blob = (string) $db->query("SELECT payload FROM blobs")->fetchColumn();
echo "Blob bytes: " . strlen($blob) . " " . ord($blob[0]) . " " . ord($blob[1]) . " " . ord($blob[2]) . "\n\n";

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
