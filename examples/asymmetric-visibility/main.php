<?php

// PHP 8.4 asymmetric visibility: the balance is readable everywhere but can only
// be changed from inside the class.
class Account
{
    public private(set) int $balance = 0;

    public function deposit(int $amount): void
    {
        $this->balance = $this->balance + $amount;
    }

    public function withdraw(int $amount): bool
    {
        if ($amount > $this->balance) {
            return false;
        }
        $this->balance = $this->balance - $amount;
        return true;
    }
}

$account = new Account();
$account->deposit(100);
$account->deposit(50);
$account->withdraw(30);

// `balance` is public to read.
echo "balance: " . $account->balance . "\n";
echo $account->withdraw(1000) ? "withdrew\n" : "insufficient funds\n";
echo "balance: " . $account->balance . "\n";
