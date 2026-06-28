<?php
// elephc-web: a tiny Laravel-style framework — routing, controllers, middleware.
//
// Compile: cargo run -- --web examples/web-framework/main.php
// Run:     ./examples/web-framework/main --listen 127.0.0.1:8080 --access-log
// Try:     curl -i  127.0.0.1:8080/
//          curl -i  127.0.0.1:8080/hello/ada
//          curl -i  127.0.0.1:8080/api/users
//          curl -i -X POST -d 'name=Bob' 127.0.0.1:8080/api/users
//          curl -i  127.0.0.1:8080/api/secret                  # 401 without the key
//          curl -i -H 'X-Api-Key: s3cr3t' 127.0.0.1:8080/api/secret
//          curl -i  127.0.0.1:8080/boom                        # 500, the framework catches it
//          curl -i  127.0.0.1:8080/missing                     # 404
//
// Design note: each request runs the whole program from a clean state (workers
// are stateless), so the app is wired up and dispatched on every request.

namespace App\Http {
    // A read-only view of the incoming request, built once from the superglobals.
    class Request
    {
        public string $method;
        public string $path;

        public function __construct()
        {
            $this->method = $_SERVER['REQUEST_METHOD'] ?? 'GET';
            $uri = $_SERVER['REQUEST_URI'] ?? '/';
            $cut = strpos($uri, '?');
            $this->path = $cut === false ? $uri : substr($uri, 0, $cut);
        }

        // Returns the n-th path segment (0-based), e.g. segment 1 of /hello/ada is "ada".
        public function segment(int $index, string $default = ''): string
        {
            $n = 0;
            foreach (explode('/', $this->path) as $part) {
                if ($part === '') {
                    continue;
                }
                if ($n === $index) {
                    return $part;
                }
                $n++;
            }
            return $default;
        }
    }

    // Builds the HTTP response with fluent setters; send() flushes it to elephc-web.
    class Response
    {
        public int $status;
        public string $body;
        public array $headers = [];

        public function __construct(string $body = '', int $status = 200)
        {
            $this->body = $body;
            $this->status = $status;
        }

        public function withHeader(string $name, string $value): Response
        {
            $this->headers[$name] = $value;
            return $this;
        }

        public static function text(string $text, int $status = 200): Response
        {
            return (new Response($text, $status))
                ->withHeader('Content-Type', 'text/plain; charset=utf-8');
        }

        // Takes an already-encoded JSON string (callers run json_encode on a local
        // array, where the assoc shape is preserved).
        public static function json(string $json, int $status = 200): Response
        {
            return (new Response($json, $status))
                ->withHeader('Content-Type', 'application/json');
        }

        // elephc-web buffers output, so headers set here (even by "after" middleware,
        // once the body was already echoed) still make it into the response.
        public function send(): void
        {
            http_response_code($this->status);
            foreach ($this->headers as $name => $value) {
                header($name . ': ' . $value);
            }
            echo $this->body;
        }
    }

    // A single-action controller: each route points at one Handler.
    interface Handler
    {
        public function handle(Request $request): void;
    }

    // Middleware wraps a request: inspect it, call $next, and/or short-circuit.
    interface Middleware
    {
        public function handle(Request $request, callable $next): void;
    }

    // One registered route. The matching lives here so it runs against this
    // route's own (typed) fields rather than untyped values pulled from a list.
    class Route
    {
        public string $method;
        public string $pattern;
        public Handler $handler;

        public function __construct(string $method, string $pattern, Handler $handler)
        {
            $this->method = $method;
            $this->pattern = $pattern;
            $this->handler = $handler;
        }

        // True when this route matches the request. A ":name" pattern segment is a
        // wildcard that matches any single path segment. Segments are compared
        // straight off `explode()` (paths share a leading "" before the first "/").
        public function matches(Request $request): bool
        {
            if ($this->method !== $request->method) {
                return false;
            }
            $pattern = explode('/', $this->pattern);
            $path = explode('/', $request->path);
            if (count($pattern) !== count($path)) {
                return false;
            }
            for ($i = 0; $i < count($pattern); $i++) {
                $seg = (string) $pattern[$i];
                if (strlen($seg) > 0 && $seg[0] === ':') {
                    continue; // wildcard: matches any single segment
                }
                if ($seg !== (string) $path[$i]) {
                    return false;
                }
            }
            return true;
        }

        public function run(Request $request): void
        {
            $this->handler->handle($request);
        }
    }

    // Holds the routes and the global middleware stack, and dispatches a request
    // through the middleware onion to the matched route (or a 404).
    class Router
    {
        private array $routes = [];     // list of Route
        private array $middleware = []; // list of Middleware, outermost-last

        public function add(string $method, string $pattern, Handler $handler): void
        {
            $this->routes[] = new Route($method, $pattern, $handler);
        }

        public function use(Middleware $middleware): void
        {
            $this->middleware[] = $middleware;
        }

        public function dispatch(Request $request): void
        {
            foreach ($this->routes as $route) {
                if (!$route->matches($request)) {
                    continue;
                }
                // Innermost layer: run the matched route's handler.
                $core = function (Request $req) use ($route): void {
                    $route->run($req);
                };
                // Wrap it in the middleware stack (last registered runs outermost).
                $next = $core;
                for ($i = count($this->middleware) - 1; $i >= 0; $i--) {
                    $middleware = $this->middleware[$i];
                    $inner = $next;
                    $next = function (Request $req) use ($middleware, $inner): void {
                        $middleware->handle($req, $inner);
                    };
                }
                $next($request);
                return;
            }
            Response::text("404 Not Found: " . $request->method . " " . $request->path . "\n", 404)->send();
        }
    }
}

namespace App\Middleware {
    use App\Http\Request;
    use App\Http\Response;
    use App\Http\Middleware;

    // Stamps every response with a header so you can confirm middleware ran.
    class RequestId implements Middleware
    {
        public function handle(Request $request, callable $next): void
        {
            $next($request);
            // Headers can be set after output — elephc-web buffers the body.
            header('X-Handled-By: elephc-web-framework');
        }
    }

    // Guards the /api/secret route behind a shared key sent as the X-Api-Key header.
    class RequireApiKey implements Middleware
    {
        private string $key;

        public function __construct(string $key)
        {
            $this->key = $key;
        }

        public function handle(Request $request, callable $next): void
        {
            if ($request->path === '/api/secret'
                && ($_SERVER['HTTP_X_API_KEY'] ?? '') !== $this->key) {
                Response::json(json_encode(['error' => 'unauthorized']), 401)->send();
                return; // short-circuit: the route handler never runs
            }
            $next($request);
        }
    }
}

namespace App\Handlers {
    use App\Http\Request;
    use App\Http\Response;
    use App\Http\Handler;

    class Home implements Handler
    {
        public function handle(Request $request): void
        {
            $help = "elephc-web mini-framework\n\n"
                . "GET  /                this page\n"
                . "GET  /hello/:name     a greeting\n"
                . "GET  /api/users       list users (JSON)\n"
                . "POST /api/users       create a user (JSON, field: name)\n"
                . "GET  /api/secret      needs header  X-Api-Key: s3cr3t\n";
            Response::text($help)->send();
        }
    }

    class Hello implements Handler
    {
        public function handle(Request $request): void
        {
            // /hello/:name -> segment 1 is the name.
            Response::text("Hello, " . $request->segment(1, 'world') . "!\n")->send();
        }
    }

    class UserList implements Handler
    {
        public function handle(Request $request): void
        {
            $users = [
                ['id' => 1, 'name' => 'Ada'],
                ['id' => 2, 'name' => 'Linus'],
            ];
            Response::json(json_encode(['users' => $users]))->send();
        }
    }

    class UserCreate implements Handler
    {
        public function handle(Request $request): void
        {
            $name = $_POST['name'] ?? '';
            if ($name === '') {
                Response::json(json_encode(['error' => 'name is required']), 422)->send();
                return;
            }
            Response::json(json_encode(['created' => ['id' => 3, 'name' => $name]]), 201)->send();
        }
    }

    class Secret implements Handler
    {
        public function handle(Request $request): void
        {
            Response::json(json_encode(['secret' => 'the cake is a lie']))->send();
        }
    }

    class Boom implements Handler
    {
        public function handle(Request $request): void
        {
            throw new \Exception('intentional failure');
        }
    }
}

namespace {
    use App\Http\Request;
    use App\Http\Response;
    use App\Http\Router;
    use App\Middleware\RequestId;
    use App\Middleware\RequireApiKey;
    use App\Handlers\Home;
    use App\Handlers\Hello;
    use App\Handlers\UserList;
    use App\Handlers\UserCreate;
    use App\Handlers\Secret;
    use App\Handlers\Boom;

    $router = new Router();
    $router->use(new RequestId());
    $router->use(new RequireApiKey('s3cr3t'));

    $router->add('GET', '/', new Home());
    $router->add('GET', '/hello/:name', new Hello());
    $router->add('GET', '/api/users', new UserList());
    $router->add('POST', '/api/users', new UserCreate());
    $router->add('GET', '/api/secret', new Secret());
    $router->add('GET', '/boom', new Boom());

    // The framework owns error handling: an uncaught handler exception becomes a 500.
    try {
        $router->dispatch(new Request());
    } catch (\Throwable $e) {
        Response::text("500 Internal Server Error\n", 500)->send();
    }
}
