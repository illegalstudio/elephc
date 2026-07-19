<?php

// Build/run with the optional FreeTDS profile:
// cargo run --features pdo-dblib -- examples/pdo-dblib/main.php
// ELEPHC_DBLIB_DSN='dblib:host=127.0.0.1;port=1433;dbname=app;user=sa;password=secret' ./examples/pdo-dblib/main
$dsn = (string) getenv("ELEPHC_DBLIB_DSN");
try {
    $db = new PDO($dsn, null, null, [PDO::ATTR_ERRMODE => PDO::ERRMODE_EXCEPTION]);
} catch (Throwable $error) {
    echo $dsn . "\n" . $error->getMessage();
    exit(1);
}
$statement = $db->prepare("SELECT :answer AS answer, :name AS name");
$statement->execute(["answer" => 42, "name" => "elephc"]);
$row = $statement->fetch(PDO::FETCH_ASSOC);

echo $row["answer"] . ":" . $row["name"];
