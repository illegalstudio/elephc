<?php

// Build/run with Microsoft ODBC Driver 18 or 17 installed:
// cargo run --features pdo-sqlsrv -- examples/pdo-sqlsrv/main.php
// ELEPHC_SQLSRV_DSN='sqlsrv:Server=127.0.0.1,1433;Database=master;Encrypt=no;TrustServerCertificate=yes;user=sa;password=secret' ./examples/pdo-sqlsrv/main
$dsn = (string) getenv("ELEPHC_SQLSRV_DSN");
try {
    $db = new PDO($dsn, null, null, [PDO::ATTR_ERRMODE => PDO::ERRMODE_EXCEPTION]);
} catch (Throwable $error) {
    echo $dsn . "\n" . $error->getMessage();
    exit(1);
}

$statement = $db->prepare(
    "SELECT CAST(:answer AS INT) AS answer, CAST(:name AS NVARCHAR(40)) AS label",
    [PDO::SQLSRV_ATTR_FETCHES_NUMERIC_TYPE => true]
);
$statement->execute(["answer" => 42, "name" => "éléphant"]);
$row = $statement->fetch(PDO::FETCH_ASSOC);

echo gettype($row["answer"]) . ":" . $row["answer"] . ":" . $row["label"];
