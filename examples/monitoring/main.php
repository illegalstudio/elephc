<?php
// elephc monitor — a program with three problems a profiler should tell apart.
//
// Build it with the capability, then read it:
//   cargo run -- build --with-monitoring examples/monitoring/main.php
//   cargo run -- monitor examples/monitoring/main.php
//
// The capability is dormant until asked, so running ./main on its own behaves
// and costs like a build without the flag — the point of shipping the artifact
// you profile.
//
// A long-running service is read through its endpoint instead of being launched:
//   ELEPHC_PROBE_ADDR=127.0.0.1:9411 ./main &
//   cargo run -- monitor 127.0.0.1:9411 --key ./main.key            # sampled
//   cargo run -- monitor 127.0.0.1:9411 --key ./main.key --exact    # one slice
//
// Keep `main.key` like a `.env` secret: holding it is what allows profiling.

// PROBLEM 1 — the N+1. One prepare, execute and fetch per line, where a single
// statement would do. The profile reports exact query counts, so this reads as
// a certainty rather than a suspicion.
function load_price(PDO $pdo, int $id): int {
    $stmt = $pdo->prepare('SELECT price FROM products WHERE id = ?');
    $stmt->execute([$id]);
    $row = $stmt->fetch();
    return $row ? (int) $row['price'] : 0;
}

// PROBLEM 2 — the audit log keeps every line it ever formats. Retained objects
// (allocated minus freed) is the dimension that makes this visible; time alone
// would not.
function record_audit(array $log, int $id, int $price): array {
    $log[] = "line " . $id . " priced at " . $price;
    return $log;
}

// PROBLEM 3 — honest CPU. Formatting money, over and over, with nothing wrong
// with it except how often it runs.
function format_money(int $cents): string {
    $units = intdiv($cents, 100);
    $rest = $cents % 100;
    return $units . "." . ($rest < 10 ? "0" : "") . $rest . " EUR";
}

function connect(): PDO {
    $pdo = new PDO('sqlite::memory:');
    $pdo->exec('CREATE TABLE products (id INTEGER PRIMARY KEY, price INTEGER)');
    for ($i = 1; $i <= 200; $i++) {
        $pdo->exec("INSERT INTO products (price) VALUES (" . (($i * 37) % 900 + 100) . ")");
    }
    return $pdo;
}

function process_order(PDO $pdo, int $lines): int {
    $total = 0;
    $audit = [];
    $rendered = [];
    for ($id = 1; $id <= $lines; $id++) {
        $price = load_price($pdo, $id);
        $total += $price;
        $audit = record_audit($audit, $id, $price);
        $rendered[] = format_money($price);
    }
    return $total + count($audit) + count($rendered);
}

$pdo = connect();
echo process_order($pdo, 200), "\n";
