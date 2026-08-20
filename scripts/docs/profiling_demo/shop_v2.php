<?php
// The same service after acting on the profile:
//  - the N+1 is gone: one prepared statement, reused for every line item;
//  - the audit log no longer hoards: it keeps a running count, not the lines;
//  - format_money is untouched, so it should stay put in the diff.

function connect(): PDO {
    $pdo = new PDO('sqlite::memory:');
    $pdo->exec('CREATE TABLE products (id INTEGER PRIMARY KEY, name TEXT, price INTEGER)');
    for ($i = 1; $i <= 300; $i++) {
        $price = ($i * 37) % 900 + 100;
        $pdo->exec("INSERT INTO products (name, price) VALUES ('product-$i', $price)");
    }
    return $pdo;
}

// FIXED — counts instead of keeping every formatted line.
function record_audit(int $seen): int {
    return $seen + 1;
}

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
    $audited = 0;
    $lines = [];
    // FIXED — one prepare for the whole order instead of one per line item,
    // so load_price disappears from the profile entirely.
    $stmt = $pdo->prepare('SELECT price FROM products WHERE id = ?');
    for ($id = 1; $id <= $items; $id++) {
        $stmt->execute([$id]);
        $row = $stmt->fetch();
        $price = $row ? (int) $row['price'] : 0;
        $total += $price;
        $audited = record_audit($audited);
        $lines[] = format_money($price);
    }
    return $total + render_invoice($lines) + $audited;
}

$pdo = connect();
echo process_order($pdo, 250), "\n";
