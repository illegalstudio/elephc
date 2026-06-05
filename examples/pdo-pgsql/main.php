<?php

// PDO with the PostgreSQL driver: the same API as the SQLite example, but the
// `pgsql:` DSN connects to a running PostgreSQL server.
//
// This example needs a server. The DSN is read from the ELEPHC_PG_DSN
// environment variable; start one with Docker and run the example like:
//
//   docker run -d --name pg -e POSTGRES_PASSWORD=test -e POSTGRES_USER=test \
//       -e POSTGRES_DB=testdb -p 55432:5432 postgres:16-alpine
//   cargo run -- examples/pdo-pgsql/main.php
//   ELEPHC_PG_DSN='pgsql:host=localhost;port=55432;dbname=testdb;user=test;password=test' \
//       ./examples/pdo-pgsql/main

$dsn = (string) getenv("ELEPHC_PG_DSN");
if ($dsn === "") {
    $dsn = "pgsql:host=localhost;port=55432;dbname=testdb;user=test;password=test";
}

$db = new PDO($dsn);

$db->exec("DROP TABLE IF EXISTS contacts");
$db->exec("CREATE TABLE contacts (
    id    SERIAL PRIMARY KEY,
    name  TEXT NOT NULL,
    email TEXT,
    score DOUBLE PRECISION
)");

// Prepared statement with named placeholders (translated to $1, $2, … for
// PostgreSQL), reused for several inserts.
$insert = $db->prepare(
    "INSERT INTO contacts (name, email, score) VALUES (:name, :email, :score)"
);
$insert->execute([":name" => "Ada Lovelace",  ":email" => "ada@example.com",  ":score" => 9.5]);
$insert->execute([":name" => "Alan Turing",   ":email" => "alan@example.com", ":score" => 9.0]);
$insert->execute([":name" => "Grace Hopper",  ":email" => "grace@example.com", ":score" => 9.8]);

// SERIAL primary key feeds lastInsertId() (via the sequence).
echo "Inserted, last id = " . $db->lastInsertId() . "\n\n";

// Positional bind: look up one contact.
$one = $db->prepare("SELECT name, score FROM contacts WHERE id = ?");
$one->execute([2]);
$row = $one->fetch(PDO::FETCH_ASSOC);
echo "Contact #2: " . $row["name"] . " (" . $row["score"] . ")\n\n";

// A PDOStatement is Traversable: foreach streams the result set.
echo "Leaderboard:\n";
$stmt = $db->query("SELECT name, score FROM contacts ORDER BY score DESC");
$stmt->setFetchMode(PDO::FETCH_ASSOC);
foreach ($stmt as $rank => $contact) {
    echo "  " . ($rank + 1) . ". " . $contact["name"] . " — " . $contact["score"] . "\n";
}
echo "\n";

// A transaction we roll back, then one we commit.
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

$db->exec("DROP TABLE contacts");
