<?php

// PDO with the MySQL / MariaDB driver: the same API as the SQLite example, but
// the `mysql:` DSN connects to a running MySQL/MariaDB server.
//
// This example needs a server. The DSN is read from the ELEPHC_MY_DSN
// environment variable; start one with Docker and run the example like:
//
//   docker run -d --name my -e MARIADB_ROOT_PASSWORD=rootpw \
//       -e MARIADB_DATABASE=testdb -e MARIADB_USER=test \
//       -e MARIADB_PASSWORD=test -p 33060:3306 mariadb:11
//   cargo run -- examples/pdo-mysql/main.php
//   ELEPHC_MY_DSN='mysql:host=127.0.0.1;port=33060;dbname=testdb;user=test;password=test' \
//       ./examples/pdo-mysql/main

$dsn = (string) getenv("ELEPHC_MY_DSN");
if ($dsn === "") {
    $dsn = "mysql:host=127.0.0.1;port=33060;dbname=testdb;user=test;password=test";
}

$db = new PDO($dsn);
echo "Driver: " . $db->getAttribute(PDO::ATTR_DRIVER_NAME) . "\n\n";

$db->exec("DROP TABLE IF EXISTS contacts");
$db->exec("CREATE TABLE contacts (
    id    INTEGER PRIMARY KEY AUTO_INCREMENT,
    name  TEXT NOT NULL,
    email TEXT,
    score DOUBLE
)");

// Prepared statement with named placeholders (rewritten to MySQL's positional
// `?`), reused for several inserts.
$insert = $db->prepare(
    "INSERT INTO contacts (name, email, score) VALUES (:name, :email, :score)"
);
$insert->execute([":name" => "Ada Lovelace",  ":email" => "ada@example.com",  ":score" => 9.5]);
$insert->execute([":name" => "Alan Turing",   ":email" => "alan@example.com", ":score" => 9.0]);
$insert->execute([":name" => "Grace Hopper",  ":email" => "grace@example.com", ":score" => 9.8]);

// AUTO_INCREMENT primary key feeds lastInsertId().
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

// A transaction we roll back, then one we commit (InnoDB tables are
// transactional; MySQL implicitly commits around DDL, so the table exists first).
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
