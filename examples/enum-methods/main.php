<?php

interface HasSymbol
{
    public function symbol(): string;
}

enum Suit: string implements HasSymbol
{
    case Hearts = "hearts";
    case Diamonds = "diamonds";
    case Clubs = "clubs";
    case Spades = "spades";

    const COUNT = 4;

    // Instance method dispatching on the case.
    public function color(): string
    {
        return match ($this) {
            Suit::Hearts, Suit::Diamonds => "red",
            Suit::Clubs, Suit::Spades => "black",
        };
    }

    // Instance method using the backing value.
    public function symbol(): string
    {
        return $this->value;
    }

    // Static factory.
    public static function trump(): self
    {
        return Suit::Spades;
    }

    // Method using a class constant via self::.
    public function deckSize(): int
    {
        return self::COUNT * 13;
    }
}

echo Suit::Hearts->color(), "/", Suit::Spades->color(), "\n";
echo Suit::Diamonds->symbol(), "\n";
echo Suit::trump()->color(), "\n";
echo Suit::Hearts->deckSize(), "\n";

// Used through the interface it implements.
function describe(HasSymbol $s): string
{
    return $s->symbol();
}
echo describe(Suit::Clubs), "\n";
