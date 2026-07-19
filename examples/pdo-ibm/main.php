<?php

// Build/run with an installed IBM Db2 CLI/ODBC driver:
// cargo run --features pdo-ibm -- examples/pdo-ibm/main.php
// ELEPHC_IBM_DSN='ibm:DATABASE=SAMPLE;HOSTNAME=127.0.0.1;PORT=50000;PROTOCOL=TCPIP;UID=db2inst1;PWD=secret' ./examples/pdo-ibm/main
$dsn = (string) getenv("ELEPHC_IBM_DSN");
try {
    $db = new PDO($dsn, null, null, [PDO::ATTR_ERRMODE => PDO::ERRMODE_EXCEPTION]);
} catch (Throwable $error) {
    echo $dsn . "\n" . $error->getMessage();
    exit(1);
}

$statement = $db->query("SELECT CAST(42 AS INTEGER) AS answer, CAST('elephc' AS VARCHAR(20)) AS label FROM SYSIBM.SYSDUMMY1");
$row = $statement->fetch(PDO::FETCH_ASSOC);

echo $row["ANSWER"] . ":" . $row["LABEL"];
