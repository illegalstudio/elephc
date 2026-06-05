<?php

// Destructors (__destruct) run automatically when an object's last reference
// goes away: at the end of a scope, when a variable is overwritten or unset,
// and at program exit. This enables RAII-style cleanup — acquire a resource in
// the constructor, release it in the destructor.

class ScopedLog
{
    private string $name;

    public function __construct(string $name)
    {
        $this->name = $name;
        echo "open(" . $this->name . ")\n";
    }

    public function __destruct()
    {
        echo "close(" . $this->name . ")\n";
    }
}

function withResource(): void
{
    // $job is released when this function returns, so "close(job)" prints
    // before control goes back to the caller.
    $job = new ScopedLog("job");
    echo "  ...working...\n";
}

withResource();

// Reassigning a variable releases the value it held, running its destructor
// right away.
$current = new ScopedLog("first");
$current = new ScopedLog("second");

// The "second" log above is still referenced by $current here, so its
// destructor runs last, during program shutdown.
echo "done\n";
