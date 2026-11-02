<?php
namespace Elephc\Web {

/** ElephC's native entry adapter. Shared application code only needs the contract. */
final class Worker
{
    private static ?WorkerApplication $application = null;
    private static bool $booted = false;
    private static bool $stopped = false;
    private static bool $bootstrapping = false;

    /** Opens registration only in the generated post-fork bootstrap callback. */
    public static function beginBootstrap(): void
    {
        if (self::$booted || self::$stopped || self::$bootstrapping) {
            throw new \LogicException('Invalid application bootstrap phase');
        }
        self::$bootstrapping = true;
    }

    /** Registers exactly one owning application during the worker bootstrap. */
    public static function register(WorkerApplication $application): void
    {
        if (!self::$bootstrapping) {
            throw new \LogicException('Application registration requires the worker bootstrap');
        }
        if (self::$application !== null || self::$booted || self::$stopped) {
            throw new \LogicException('A worker application is already registered');
        }
        self::$application = $application;
    }

    /** Commits the bootstrap only after its entry script completes successfully. */
    public static function completeBootstrap(): void
    {
        if (!self::$bootstrapping || \elephc_web_application_phase() !== 1) {
            throw new \LogicException('Invalid application bootstrap commit phase');
        }
        if (self::$application === null) {
            throw new \LogicException('Bootstrap did not register a worker application');
        }
        self::$booted = true;
        self::$bootstrapping = false;
    }

    /** Reports whether this worker may begin serving HTTP requests. */
    public static function ready(): int
    {
        return self::$booted ? 1 : 0;
    }

    /** Invokes the registered application with an independently owned request. */
    public static function dispatch(): void
    {
        $application = self::$application;
        if (!self::$booted || self::$stopped || $application === null) {
            throw new \LogicException('No active worker application');
        }
        if ($application !== null) {
            $request = new Request(
                \elephc_web_method(), \elephc_web_uri(), \elephc_web_path(),
                (string) \file_get_contents('php://input'),
            );
            $application->handle($request);
        }
    }

    /** Detaches registration before invoking shutdown, including a throwing shutdown. */
    public static function stop(): void
    {
        if (self::$stopped) { return; }
        if (\elephc_web_application_phase() !== 3) {
            throw new \LogicException('Application shutdown requires worker termination');
        }
        self::$stopped = true;
        self::$bootstrapping = false;
        $booted = self::$booted;
        self::$booted = false;
        $application = self::$application;
        self::$application = null;
        if ($booted && $application !== null) {
            $application->shutdown();
        }
    }

    /** Prevents a bootstrap exception from leaving an earlier commit visible. */
    public static function failBootstrap(): void
    {
        if (\elephc_web_application_phase() >= 2) {
            throw new \LogicException('Cannot fail a committed worker application');
        }
        self::$booted = false;
        self::$bootstrapping = false;
    }
}

}

namespace {
    function __elephc_web_application_bootstrap_failure(\Throwable $error): void
    {
        \Elephc\Web\Worker::failBootstrap();
        error_log('elephc-web: bootstrap ' . get_class($error) . ': ' . $error->getMessage()
            . ' at ' . $error->getFile() . ':' . $error->getLine());
    }

    function __elephc_web_application_bootstrap_prepare(): void
    {
        $_ENV = [];
        $_SERVER = [];
        foreach (getenv() as $key => $value) {
            $_ENV[$key] = $value;
            $_SERVER[$key] = $value;
        }
        $_GET = [];
        $_POST = [];
        $_COOKIE = [];
        $_REQUEST = [];
        $_FILES = [];
        $_SESSION = [];
    }

    function __elephc_web_application_context_option(string $wrapper, string $option, string $value): bool
    {
        $options = stream_context_get_options(stream_context_get_default());
        $next = [];
        $replaced = false;
        foreach ($options as $name => $settings) {
            if ($name === $wrapper) {
                $updated = [];
                foreach ($settings as $key => $previous) {
                    if ($key !== $option) { $updated[$key] = $previous; }
                }
                $updated[$option] = $value;
                $next[$name] = $updated;
                $replaced = true;
            } else {
                $next[$name] = $settings;
            }
        }
        if (!$replaced) { $next[$wrapper] = [$option => $value]; }
        stream_context_set_option(stream_context_get_default(), $next);
        return true;
    }

    function __elephc_web_application_ready(): int
    {
        return \Elephc\Web\Worker::ready();
    }

    function __elephc_web_application_request(): void
    {
        try {
            elephc_web_application_request_begin();
            __elephc_web_prepare_request();
            \Elephc\Web\Worker::dispatch();
        } catch (\Throwable) {
            elephc_web_handle_uncaught_exception();
        } finally {
            try {
                if (elephc_web_session_get_status() === PHP_SESSION_ACTIVE
                    && \__ElephcSessionState::$shutdown) {
                    session_write_close();
                }
            } catch (\Throwable) {
                elephc_web_handle_uncaught_exception();
            }
            try {
                elephc_web_application_request_end();
            } catch (\Throwable) {
                elephc_web_handle_uncaught_exception();
            }
        }
    }

    function __elephc_web_application_shutdown(): void
    {
        try {
            \Elephc\Web\Worker::stop();
        } catch (\Throwable) {
            error_log('elephc-web: application shutdown failed');
        } finally {
            try {
                elephc_web_application_shutdown_cleanup();
            } catch (\Throwable) {
                error_log('elephc-web: application cleanup failed');
            }
        }
    }
}
