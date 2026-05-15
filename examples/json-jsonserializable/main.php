<?php
// json-jsonserializable — demonstrates the JsonSerializable interface.
//
// elephc honors the standard PHP contract: when an object's class implements
// JsonSerializable, json_encode() calls $obj->jsonSerialize() and encodes the
// returned value instead of walking public properties.

class Money implements JsonSerializable
{
    public int $amountCents;
    public string $currency;
    private string $internalNote = "ignored";

    public function __construct(int $cents, string $currency)
    {
        $this->amountCents = $cents;
        $this->currency = $currency;
    }

    public function jsonSerialize(): mixed
    {
        // Re-shape the object into a public-facing dict that strips the
        // internal note and exposes the formatted amount.
        $major = (int) ($this->amountCents / 100);
        $minor = $this->amountCents - $major * 100;
        return [
            "currency" => $this->currency,
            "amount"   => $major,
            "cents"    => $minor,
        ];
    }
}

class Order
{
    public int $id;
    public Money $total;

    public function __construct(int $id, Money $total)
    {
        $this->id = $id;
        $this->total = $total;
    }
}

// json_encode dispatches to Money::jsonSerialize() and walks Order's public
// properties. The non-JsonSerializable Order class is encoded by reading its
// public fields directly.
$order = new Order(42, new Money(1995, "EUR"));
echo json_encode($order) . "\n";

// JsonSerializable also fires when the object is nested inside an array or
// associative array.
$rows = [new Money(100, "USD"), new Money(250, "GBP")];
echo json_encode($rows) . "\n";

$assoc = ["a" => new Money(7, "JPY"), "b" => new Money(0, "EUR")];
echo json_encode($assoc) . "\n";

// instanceof works against the builtin JsonSerializable interface.
echo ($order->total instanceof JsonSerializable ? "yes" : "no") . "\n";
echo ($order instanceof JsonSerializable ? "yes" : "no") . "\n";
