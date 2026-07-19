<?php

// Build with `cargo run --features pdo-oci -- examples/pdo-oci/main.php`.
// Oracle Instant Client must be discoverable by the platform dynamic loader.
$dsn = getenv("ELEPHC_OCI_DSN");
if ($dsn === false || $dsn === "") {
    echo "Set ELEPHC_OCI_DSN to an oci: DSN.\n";
    exit(1);
}

$database = new PDO($dsn, null, null, [
    PDO::ATTR_ERRMODE => PDO::ERRMODE_EXCEPTION,
    PDO::ATTR_PREFETCH => 100,
]);
$database->setAttribute(PDO::OCI_ATTR_MODULE, "elephc-example");

$statement = $database->prepare("SELECT :message AS MESSAGE FROM DUAL");
$statement->execute(["message" => "Hello from PDO_OCI"]);
echo $statement->fetchColumn() . "\n";
