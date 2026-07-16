<?php

// Build/run with the optional pure-Rust Firebird profile:
// cargo run --features pdo-firebird -- examples/pdo-firebird/main.php
// ELEPHC_FIREBIRD_DSN='firebird:dbname=127.0.0.1/3050:/data/app.fdb;charset=UTF8;user=SYSDBA;password=secret' ./examples/pdo-firebird/main
$dsn = (string) getenv("ELEPHC_FIREBIRD_DSN");
try {
    $db = new PDO($dsn, null, null, [PDO::ATTR_ERRMODE => PDO::ERRMODE_EXCEPTION]);
} catch (Throwable $error) {
    echo $dsn . "\n" . $error->getMessage();
    exit(1);
}

$statement = $db->prepare(
    "SELECT CAST(:answer AS INTEGER) AS answer, CAST(:name AS VARCHAR(20)) AS name FROM RDB\$DATABASE"
);
$statement->execute(["answer" => 42, "name" => "elephc"]);
$row = $statement->fetch(PDO::FETCH_ASSOC);

echo $row["ANSWER"] . ":" . trim($row["NAME"]);
