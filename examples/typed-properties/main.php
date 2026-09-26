<?php

class User {
    public int $id;
    public string $name = "Ada";
    public ?string $email = null;
    // `Foo::class` is a compile-time string, so it can be a default.
    public static string $repository = UserRepository::class;

    public function __construct($id) {
        $this->id = $id;
    }

    public function label() {
        return $this->name . ":" . $this->id;
    }
}

final class UserRepository {}

$user = new User(42);
echo $user->label();
echo PHP_EOL;
echo "repository: " . User::$repository . PHP_EOL;

if (is_null($user->email)) {
    echo "missing email";
    echo PHP_EOL;
}

// A NULLABLE or union array property takes an array literal default, in either spelling. The
// value is boxed like any other `mixed` payload, so the slot can still hold null later.
class Request
{
    public ?array $query = ["page" => 1, "sort" => "name"];
    public ?array $tags = ["new", "featured"];
    public mixed $meta = ["source" => "web"];
    public ?array $body = null;

    public function clear(): void
    {
        $this->query = null;
    }
}

$request = new Request();
echo "query: " . $request->query["page"] . "/" . $request->query["sort"] . PHP_EOL;
echo "tags: " . implode(", ", $request->tags) . PHP_EOL;
echo "meta: " . $request->meta["source"] . PHP_EOL;
echo "body is null: " . (is_null($request->body) ? "yes" : "no") . PHP_EOL;
$request->clear();
echo "query after clear: " . (is_null($request->query) ? "null" : "set") . PHP_EOL;
