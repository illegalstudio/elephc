<?php
// elephc monitor --apm — a small shop API whose routes behave differently
// enough for transaction monitoring to tell them apart.
//
// Build it with the monitoring capability, start it with --apm, and point the
// collector at its endpoint:
//   cargo run -- build --web --with-monitoring examples/web-apm/main.php
//   ELEPHC_PROBE_ADDR=127.0.0.1:9411 examples/web-apm/main --listen 127.0.0.1:8080 --apm &
//   cargo run -- monitor 127.0.0.1:9411 --apm --key examples/web-apm/main.key \
//       --store /tmp/shop-apm --serve 127.0.0.1:9000 --capture-every 5
//   open http://127.0.0.1:9000/
//
// Or let examples/web-apm/demo.sh do all of that and send traffic as well.
//
// --apm counts every request per route — status class, wall time, database
// statements and the time spent waiting on them — from the first one, whether or
// not anyone is watching. The route is the request's shape, so /products/17 and
// /products/42 are one row: GET /products/{id}. With --capture-every, the
// collector keeps the exact profile of the slowest requests and of failures.

$path = parse_url($_SERVER['REQUEST_URI'] ?? '/', PHP_URL_PATH);
$method = $_SERVER['REQUEST_METHOD'] ?? 'GET';

// A catalogue built on every request, so the listing has a real cost.
function catalogue(int $size): array {
    $items = [];
    for ($i = 1; $i <= $size; $i++) {
        $items[] = ['id' => $i, 'name' => 'product-' . $i, 'price' => ($i * 37) % 1000];
    }
    return $items;
}

// The shop's database: an in-memory SQLite one, so the example needs nothing
// installed. Opened by the routes that use it — two statements — with fifty
// products and a few order lines.
function open_db(): PDO {
    $pdo = new PDO('sqlite::memory:');
    $pdo->exec('CREATE TABLE products (id INTEGER PRIMARY KEY, name TEXT, price INTEGER);
                CREATE TABLE lines (order_id INTEGER, product_id INTEGER, qty INTEGER)');
    $rows = [];
    for ($i = 1; $i <= 50; $i++) {
        $rows[] = '(' . $i . ", 'product-" . $i . "', " . (($i * 37) % 1000) . ')';
    }
    $lines = [];
    for ($o = 1; $o <= 20; $o++) {
        for ($l = 1; $l <= 8; $l++) {
            $lines[] = '(' . $o . ', ' . ((($o * 7 + $l) % 50) + 1) . ', ' . $l . ')';
        }
    }
    $pdo->exec('INSERT INTO products VALUES ' . implode(', ', $rows)
        . '; INSERT INTO lines VALUES ' . implode(', ', $lines));
    return $pdo;
}

// Fast: one product by id, one query, or a 404.
function show_product(PDO $db, int $id): void {
    $stmt = $db->prepare('SELECT id, name, price FROM products WHERE id = ?');
    $stmt->execute([$id]);
    $row = $stmt->fetch(PDO::FETCH_ASSOC);
    if ($row === false) {
        http_response_code(404);
        echo json_encode(['error' => 'no such product']);
        return;
    }
    echo json_encode($row);
}

// The N+1: one query for the order's lines, then one more per line for its
// price, where a join would do. The table shows it as statements per request.
function show_order(PDO $db, int $id): void {
    $stmt = $db->prepare('SELECT product_id, qty FROM lines WHERE order_id = ?');
    $stmt->execute([$id]);
    $total = 0;
    foreach ($stmt->fetchAll(PDO::FETCH_ASSOC) as $line) {
        $price = $db->prepare('SELECT price FROM products WHERE id = ?');
        $price->execute([$line['product_id']]);
        $total += (int) $price->fetchColumn() * (int) $line['qty'];
    }
    echo json_encode(['order' => $id, 'total' => $total]);
}

// Medium: sorts and encodes the whole catalogue.
function list_products(): void {
    $items = catalogue(500);
    usort($items, fn($a, $b) => $a['price'] <=> $b['price']);
    echo json_encode(array_slice($items, 0, 20));
}

// Slow on purpose: a payment provider that takes its time.
function checkout(): void {
    usleep(15000);
    echo json_encode(['status' => 'paid']);
}

// Fails on purpose, one call in ten: the 5xx a dashboard should surface, and the
// error the page groups under Errors.
function flaky_report(int $seed): void {
    if ($seed % 10 === 0) {
        throw new RuntimeException('report backend unavailable');
    }
    echo json_encode(['total' => array_sum(array_column(catalogue(200), 'price'))]);
}

// `/products/42` -> ['products', '42']; the id is numeric or the route is unknown.
$parts = explode('/', trim($path, '/'));
$id = count($parts) === 2 && ctype_digit($parts[1]) ? (int) $parts[1] : null;

if ($path === '/products') {
    list_products();
} elseif ($parts[0] === 'products' && $id !== null) {
    show_product(open_db(), $id);
} elseif ($parts[0] === 'orders' && $id !== null) {
    show_order(open_db(), $id);
} elseif ($path === '/checkout' && $method === 'POST') {
    checkout();
} elseif ($parts[0] === 'reports' && $id !== null) {
    flaky_report($id);
} elseif ($path === '/') {
    echo "shop: GET /products, GET /products/{id}, GET /orders/{id}, POST /checkout, GET /reports/{id}\n";
} else {
    http_response_code(404);
    echo json_encode(['error' => 'not found']);
}
