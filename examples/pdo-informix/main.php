<?php

// Build/run with the optional IBM/HCL Client SDK profile:
// cargo run --features pdo-informix -- examples/pdo-informix/main.php
// ELEPHC_INFORMIX_DSN='informix:Driver={IBM INFORMIX ODBC DRIVER};Server=ol_informix;Database=app;UID=app;PWD=secret' ./examples/pdo-informix/main
$dsn = (string) getenv("ELEPHC_INFORMIX_DSN");
try {
    $db = new PDO($dsn, null, null, [PDO::ATTR_ERRMODE => PDO::ERRMODE_EXCEPTION]);
} catch (Throwable $error) {
    echo $dsn . "\n" . $error->getMessage();
    exit(1);
}

$statement = $db->prepare("SELECT CAST(:answer AS INTEGER) AS answer, CAST(:name AS VARCHAR(20)) AS name FROM systables WHERE tabid = 1");
$statement->execute(["answer" => 42, "name" => "elephc"]);
$row = $statement->fetch(PDO::FETCH_ASSOC);

echo $row["ANSWER"] . ":" . $row["NAME"];
