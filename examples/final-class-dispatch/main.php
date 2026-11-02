<?php
// A final class keeps inherited behavior and makes its method targets fixed.
class PriceList {
    public function shipping(): int {
        return 5;
    }
}

final class MemberPrices extends PriceList {
    public function total(int $items): int {
        return $items * 8 + $this->shipping();
    }
}

$prices = new MemberPrices();
echo 'Three items with shipping: ', $prices->total(3), "\n";
