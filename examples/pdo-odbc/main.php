<?php

// Build/run with the optional unixODBC profile:
// cargo run --features pdo-odbc -- examples/pdo-odbc/main.php
// ELEPHC_ODBC_DSN='odbc:Driver={PostgreSQL Unicode};Servername=127.0.0.1;Port=5432;Database=app;UID=app;PWD=secret' ./examples/pdo-odbc/main
$dsn = (string) getenv("ELEPHC_ODBC_DSN");
try {
    $db = new PDO($dsn, null, null, [PDO::ATTR_ERRMODE => PDO::ERRMODE_EXCEPTION]);
} catch (Throwable $error) {
    echo $dsn . "\n" . $error->getMessage();
    exit(1);
}

$statement = $db->prepare("SELECT CAST(:answer AS INTEGER) AS answer, CAST(:name AS VARCHAR(20)) AS name");
$statement->execute(["answer" => 42, "name" => "elephc"]);
$row = $statement->fetch(PDO::FETCH_ASSOC);

echo $row["answer"] . ":" . $row["name"];
