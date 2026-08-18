<?php
// A small order-processing service, with the three problems a profiler should
// be able to tell apart: an N+1, a cache that never releases, and plain CPU.

function connect(): PDO {
    $pdo = new PDO('sqlite::memory:');
    $pdo->exec('CREATE TABLE products (id INTEGER PRIMARY KEY, name TEXT, price INTEGER)');
    for ($i = 1; $i <= 300; $i++) {
        $price = ($i * 37) % 900 + 100;
        $pdo->exec("INSERT INTO products (name, price) VALUES ('product-$i', $price)");
    }
    return $pdo;
}

// PROBLEM 1 — the N+1: one prepare + execute + fetch per line item.
function load_price(PDO $pdo, int $id): int {
    $stmt = $pdo->prepare('SELECT price FROM products WHERE id = ?');
    $stmt->execute([$id]);
    $row = $stmt->fetch();
    return $row ? (int) $row['price'] : 0;
}

// PROBLEM 2 — the audit log keeps every line it ever formats.
function record_audit(array $log, int $id, int $price): array {
    $log[] = "order line " . $id . " priced at " . $price;
    return $log;
}

// PROBLEM 3 — honest CPU work: formatting money, over and over.
function format_money(int $cents): string {
    $units = intdiv($cents, 100);
    $rest = $cents % 100;
    $pad = $rest < 10 ? "0" : "";
    return $units . "." . $pad . $rest . " EUR";
}

function render_invoice(array $lines): int {
    $width = 0;
    for ($i = 0; $i < count($lines); $i++) {
        $width += strlen($lines[$i]);
    }
    return $width;
}

function process_order(PDO $pdo, int $items): int {
    $total = 0;
    $audit = [];
    $lines = [];
    for ($id = 1; $id <= $items; $id++) {
        $price = load_price($pdo, $id);
        $total += $price;
        $audit = record_audit($audit, $id, $price);
        $lines[] = format_money($price);
    }
    return $total + render_invoice($lines) + count($audit);
}

$pdo = connect();
echo process_order($pdo, 250), "\n";
