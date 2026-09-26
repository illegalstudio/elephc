<?php
// Isolates query-fixture value ownership without any mbstring call or native test shim.
class QueryTernaryOwner {
    public function __construct(public bool $fail) {}
    public function __destruct() {
        echo "destroy\n";
        if ($this->fail) { throw new RuntimeException("query ternary"); }
    }
}
function query_ternary_closure(bool $fail): callable {
    $owner = new QueryTernaryOwner($fail);
    return function() use ($owner): void {};
}
function query_ternary_probe(bool $fail, bool $closure): void {
    $root = ["key" => $closure ? query_ternary_closure($fail) : new QueryTernaryOwner($fail)];
    try { unset($root["key"]); echo "removed\n"; }
    catch (Throwable $error) { echo "caught:", $error->getMessage(), "\n"; }
    echo count($root), "\n";
}
query_ternary_probe(false, false);
query_ternary_probe(true, false);
query_ternary_probe(false, true);
query_ternary_probe(true, true);
echo "done\n";
