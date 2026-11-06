<?php
// exception-routing: an order pipeline with a deep exception hierarchy.
//
// elephc works out WHICH exception classes every call can throw, and routes
// them through the catch clauses in PHP order (first match wins). A clause
// that no exception can ever reach is dead code: the optimizer removes it
// before lowering, and whole-program declaration reachability then drops the
// functions and classes that only such a clause referenced.
//
// This file writes ten catch clauses; four of them can never run. See it with
//
//     elephc --emit-ir examples/exception-routing/main.php
//
// Six handlers survive (one `catch_bind` each), and PaymentIncident,
// CarrierDesk, restockAndRetry() and legacyPaymentFallback() are nowhere in
// the output. PHP prints the same report: there the dead clauses simply never
// match.

class ShopException extends RuntimeException {}

class ValidationException extends ShopException {}
class InvalidQuantityException extends ValidationException {}

class InventoryException extends ShopException {}
class OutOfStockException extends InventoryException {}

class PaymentException extends ShopException {}
class CardDeclinedException extends PaymentException {}
class FraudSuspectedException extends PaymentException {}

class ShippingException extends ShopException {}
class CarrierUnavailableException extends ShippingException {}

// Recovery code that only an impossible handler uses. None of it reaches the
// generated program.

class PaymentIncident {
    private string $report;

    public function __construct(PaymentException $cause) {
        $this->report = "incident opened: " . $cause->getMessage();
    }

    public static function open(PaymentException $cause): PaymentIncident {
        return new PaymentIncident($cause);
    }

    public function report(): string {
        return $this->report;
    }
}

class CarrierDesk {
    public static function escalate(CarrierUnavailableException $e): string {
        return "carrier desk ticket: " . $e->getMessage();
    }
}

function restockAndRetry(InventoryException $e): string {
    return "restock requested: " . $e->getMessage();
}

function legacyPaymentFallback(PaymentException $e): string {
    return "legacy gateway fallback: " . $e->getMessage();
}

// The pipeline stages. Each one throws exact classes, so every caller gets a
// precise summary of what can escape. The detail travels in the exception
// code rather than in the message: concatenating a variable into the message
// is an implicit string conversion, which the optimizer conservatively treats
// as able to throw anything (a __toString() could), and a call that may throw
// anything keeps every handler around it reachable.

function validateQuantity(int $qty): void {
    if ($qty <= 0) {
        throw new InvalidQuantityException("quantity must be positive", $qty);
    }
}

function reserveStock(int $available, int $qty): int {
    if ($qty > $available) {
        throw new OutOfStockException("not enough units", $available);
    }
    return $available - $qty;
}

function chargeCard(int $cents): int {
    if ($cents > 100000) {
        throw new FraudSuspectedException("amount above the fraud limit", $cents);
    }
    if ($cents % 100 === 13) {
        throw new CardDeclinedException("card declined by the issuer", $cents);
    }
    return $cents;
}

function bookCarrier(int $grams): int {
    if ($grams > 30000) {
        throw new CarrierUnavailableException("no carrier takes parcels over 30 kg", $grams);
    }
    return $grams > 5000 ? 1299 : 499;
}

// 1. Disjoint catch types. Reserving stock can only fail with a validation or
//    an inventory problem, so the PaymentException clause can never match: it
//    is removed, and PaymentIncident goes with it.
function reserveLine(int $available, int $qty): string {
    try {
        validateQuantity($qty);
        $left = reserveStock($available, $qty);
    } catch (PaymentException $e) {
        return PaymentIncident::open($e)->report();
    } catch (ValidationException $e) {
        return "rejected, " . $e->getMessage() . " (got " . $e->getCode() . ")";
    } catch (InventoryException $e) {
        return "backordered, " . $e->getMessage() . " (" . $e->getCode() . " available)";
    }
    return "reserved, " . $left . " left";
}

// 2. `throw $e` keeps the narrowed type. The inner clause only ever holds a
//    FraudSuspectedException, so the rethrow is exactly that class and the
//    outer InventoryException clause (with restockAndRetry) is removed.
// 3. Children before their parent. chargeCard() throws exactly
//    CardDeclinedException or FraudSuspectedException, and both have their own
//    clause, so the trailing PaymentException clause has nothing left to catch
//    and is removed with legacyPaymentFallback().
function settle(int $cents): string {
    try {
        try {
            $charged = chargeCard($cents);
        } catch (FraudSuspectedException $e) {
            echo "  [fraud screen] payment escalated for manual review\n";
            throw $e;
        }
    } catch (InventoryException $e) {
        return restockAndRetry($e);
    } catch (FraudSuspectedException $e) {
        return "on hold, " . $e->getMessage();
    } catch (CardDeclinedException $e) {
        return "declined, " . $e->getMessage();
    } catch (PaymentException $e) {
        return legacyPaymentFallback($e);
    }
    return "charged " . $charged . " cents";
}

// 4. A parent before its child. Every CarrierUnavailableException is also a
//    ShippingException, so the first clause always wins: the second one is
//    shadowed, and CarrierDesk is removed with it.
function ship(int $grams): string {
    try {
        $postage = bookCarrier($grams);
    } catch (ShippingException $e) {
        return "delayed, " . $e->getMessage();
    } catch (CarrierUnavailableException $e) {
        return CarrierDesk::escalate($e);
    }
    return "booked, " . $postage . " cents postage";
}

// id, quantity, units in stock, amount in cents, parcel weight in grams
$orders = [
    ["A-100", 2, 10, 2599, 800],
    ["A-101", 0, 10, 2599, 800],
    ["A-102", 5, 3, 2599, 800],
    ["A-103", 1, 10, 4013, 800],
    ["A-104", 1, 10, 250000, 800],
    ["A-105", 1, 10, 2599, 42000],
];

foreach ($orders as $order) {
    echo "order " . $order[0] . "\n";
    $stock = reserveLine($order[2], $order[1]);
    $payment = settle($order[3]);
    $shipping = ship($order[4]);
    echo "  stock:    " . $stock . "\n";
    echo "  payment:  " . $payment . "\n";
    echo "  shipping: " . $shipping . "\n";
}
