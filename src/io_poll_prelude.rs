//! Purpose:
//! Declares PHP 8.6's `Io\Poll` API and implements its platform backends in PHP.
//!
//! Called from:
//! - `crate::core_prelude::inject_if_used` when a program reaches `Io\Poll` or
//!   `StreamPollHandle`.
//!
//! Key details:
//! - The surface is parsed from one internal PHP source, so namespaces, enums, interfaces,
//!   exception hierarchies, and readonly metadata stay identical to the engine's declarations.
//! - `Epoll` is advertised on Linux and `Kqueue` on Darwin and `Auto` prefers them; the wait
//!   itself runs on the shared runtime machinery: `stream_select()` for level readiness
//!   (descriptors 0..1023) plus a zero-timeout `poll(2)` revents probe that separates
//!   `HangUp`/`ReadHangUp` and `Error` from a bare read/write bit. `EventPorts` and `WSAPoll`
//!   stay unavailable because elephc has no Solaris or Windows target.
//! - Edge triggering is refused: a shared level-triggered wait cannot observe a readiness
//!   transition that happens between two calls. True edge support needs the native
//!   epoll/kqueue syscalls.
//! - Watchers hold the context through `WeakReference`, so a watcher does not keep a dropped
//!   context alive and `isActive()` turns false exactly when the context is gone.

use crate::compiler_stack::with_compiler_stack;
use crate::parser::ast::Program;

/// The complete `Io\Poll` surface and its `Poll` backend.
const IO_POLL_PRELUDE_SRC: &str = r#"<?php

namespace Io {
    class IoException extends \Exception {}
}

namespace Io\Poll {
    enum Backend {
        case Auto;
        case Poll;
        case Epoll;
        case Kqueue;
        case EventPorts;
        case WSAPoll;

        public static function getAvailableBackends(): array {
            $backends = [self::Poll];
            if (\PHP_OS === 'Linux') {
                $backends[] = self::Epoll;
            }
            if (\PHP_OS === 'Darwin') {
                $backends[] = self::Kqueue;
            }
            return $backends;
        }

        public function isAvailable(): bool {
            if ($this === self::Poll) {
                return true;
            }
            if ($this === self::Epoll) {
                return \PHP_OS === 'Linux';
            }
            if ($this === self::Kqueue) {
                return \PHP_OS === 'Darwin';
            }
            return false;
        }

        public function supportsEdgeTriggering(): bool {
            // The shared stream_select()/poll(2) machinery cannot observe a readiness
            // transition that happens between two waits; true edge support needs the
            // epoll/kqueue syscalls themselves. Keep the safe refusal until then.
            return false;
        }
    }

    enum Event {
        case Read;
        case Write;
        case Error;
        case HangUp;
        case ReadHangUp;
        case OneShot;
        case EdgeTriggered;
    }

    interface Handle {}

    final class Watcher {
        private \WeakReference $context;
        private \StreamPollHandle $handle;
        private array $watchedEvents;
        private array $triggeredEvents = [];
        private mixed $data;
        private bool $active = true;
        private bool $fired = false;

        public function __construct(\WeakReference $context, \StreamPollHandle $handle, array $events, mixed $data = null) {
            $this->context = $context;
            $this->handle = $handle;
            $this->watchedEvents = $events;
            $this->data = $data;
        }

        public function getHandle(): \StreamPollHandle {
            return $this->handle;
        }

        public function getWatchedEvents(): array {
            return $this->watchedEvents;
        }

        public function getTriggeredEvents(): array {
            return $this->triggeredEvents;
        }

        public function getData(): mixed {
            return $this->data;
        }

        public function hasTriggered(Event $event): bool {
            return in_array($event, $this->triggeredEvents, true);
        }

        public function isActive(): bool {
            if (!$this->active) {
                return false;
            }
            if (!$this->handle->isValid()) {
                return false;
            }
            return $this->context->get() !== null;
        }

        public function modify(array $events, mixed $data = null): void {
            $this->modifyEvents($events);
            $this->modifyData($data);
        }

        public function modifyEvents(array $events): void {
            $context = $this->context->get();
            if ($context === null || !$this->active || !$this->handle->isValid()) {
                throw new InactiveWatcherException('Cannot modify inactive watcher');
            }
            $context->validateEvents($events);
            $this->watchedEvents = $events;
            $this->triggeredEvents = [];
            $this->fired = false;
        }

        public function modifyData(mixed $data): void {
            if ($this->context->get() === null || !$this->active) {
                throw new InactiveWatcherException('Cannot modify inactive watcher');
            }
            $this->data = $data;
        }

        public function remove(): void {
            $context = $this->context->get();
            if ($context === null) {
                throw new InactiveWatcherException('Cannot remove inactive watcher');
            }
            if (!$this->active) {
                return;
            }
            $this->active = false;
            $this->triggeredEvents = [];
            $context->__elephcRemoveWatcher($this);
        }

        public function __elephcResetTriggered(): void {
            $this->triggeredEvents = [];
        }

        public function __elephcMarkTriggered(array $events): void {
            $this->triggeredEvents = $events;
            if (in_array(Event::OneShot, $this->watchedEvents, true)) {
                $this->fired = true;
            }
        }

        public function __elephcFired(): bool {
            return $this->fired;
        }


        public function __elephcActive(): bool {
            return $this->active && $this->handle->isValid();
        }

        public function __elephcStream(): mixed {
            return $this->handle->getStream();
        }

        public function __debugInfo(): array {
            return [];
        }

        public function __clone() {
            throw new \Error('Trying to clone an uncloneable object of class Io\\Poll\\Watcher');
        }
    }

    final class Context {
        private Backend $backend;
        private bool $constructed = false;
        private array $watchers = [];

        public function __construct(Backend $backend = Backend::Auto) {
            if ($this->constructed) {
                throw new \Error('Io\\Poll\\Context object is already constructed');
            }
            $this->constructed = true;
            if ($backend === Backend::Auto) {
                if (Backend::Epoll->isAvailable()) {
                    $actual = Backend::Epoll;
                } elseif (Backend::Kqueue->isAvailable()) {
                    $actual = Backend::Kqueue;
                } else {
                    $actual = Backend::Poll;
                }
            } else {
                $actual = $backend;
            }
            if (!$actual->isAvailable()) {
                throw new BackendUnavailableException('Backend ' . $actual->name . ' not available');
            }
            $this->backend = $actual;
        }

        public function add(Handle $handle, array $events, mixed $data = null): Watcher {
            if (!$handle instanceof \StreamPollHandle) {
                throw new InvalidHandleException('Invalid handle for polling');
            }
            if (!$handle->isValid()) {
                throw new InvalidHandleException('Invalid handle for polling');
            }
            $this->validateEvents($events);
            foreach ($this->watchers as $existing) {
                if ($existing->__elephcActive() && $existing->__elephcStream() === $handle->getStream()) {
                    throw new HandleAlreadyWatchedException('Handle already added');
                }
            }
            $watcher = new Watcher(\WeakReference::create($this), $handle, $events, $data);
            $this->watchers[] = $watcher;
            return $watcher;
        }

        public function wait(?\Time\Duration $timeout = null, ?int $maxEvents = null): array {
            if ($timeout !== null && $timeout->negative) {
                throw new \ValueError('Io\\Poll\\Context::wait(): Argument #1 ($timeout) must not be negative');
            }
            if ($maxEvents !== null) {
                if ($maxEvents < 1) {
                    throw new \ValueError('Io\\Poll\\Context::wait(): Argument #2 ($maxEvents) must be greater than 0');
                }
                if ($maxEvents > 2147483647) {
                    throw new \ValueError('Io\\Poll\\Context::wait(): Argument #2 ($maxEvents) must be less than or equal to 2147483647');
                }
            }
            $limit = $maxEvents === null ? 2147483647 : $maxEvents;
            $seconds = 3600;
            $micros = 0;
            if ($timeout !== null) {
                $seconds = $timeout->seconds;
                $micros = intdiv($timeout->nanoseconds, 1000);
            }

            $active = [];
            foreach ($this->watchers as $watcher) {
                $watcher->__elephcResetTriggered();
                if (!$watcher->__elephcActive() || $watcher->__elephcFired()) {
                    continue;
                }
                $active[] = $watcher;
            }
            if ($active === []) {
                if ($timeout !== null && ($seconds > 0 || $micros > 0)) {
                    usleep((int) ($seconds * 1000000 + $micros));
                }
                return [];
            }

            $deadline = null;
            if ($timeout !== null) {
                $deadline = hrtime(true) + $seconds * 1000000000 + $micros * 1000;
            }

            while (true) {
                $read = [];
                $write = [];
                $buffered = [];
                foreach ($active as $watcher) {
                    $stream = $watcher->__elephcStream();
                    $events = $watcher->getWatchedEvents();
                    $wantsRead = in_array(Event::Read, $events, true);
                    $wantsWrite = in_array(Event::Write, $events, true);
                    $wantsError = in_array(Event::Error, $events, true);
                    $wantsHangUp = in_array(Event::HangUp, $events, true)
                        || in_array(Event::ReadHangUp, $events, true);
                    $meta = stream_get_meta_data($stream);
                    $hasBuffered = $wantsRead && ($meta['unread_bytes'] ?? 0) > 0;
                    if ($hasBuffered) {
                        $buffered[] = $watcher;
                    } elseif ($wantsRead || $wantsError || $wantsHangUp) {
                        // A hang-up makes the descriptor READABLE; the zero-timeout poll below
                        // tells that readiness apart from ordinary data.
                        $read[] = $stream;
                    }
                    if ($wantsWrite) {
                        $write[] = $stream;
                    }
                }

                if ($read === [] && $write === [] && $buffered === []) {
                    // Only modifiers that cannot make a descriptor ready: wait out the timeout.
                    if ($deadline !== null) {
                        $remaining = $deadline - hrtime(true);
                        if ($remaining > 0) {
                            usleep((int) intdiv($remaining, 1000));
                        }
                    }
                    return [];
                }

                $ready = false;
                if ($read !== [] || $write !== []) {
                    $selectSeconds = $seconds;
                    $selectMicros = $micros;
                    if ($buffered !== []) {
                        $selectSeconds = 0;
                        $selectMicros = 0;
                    }
                    $except = null;
                    $selected = stream_select($read, $write, $except, $selectSeconds, $selectMicros);
                    if ($selected === false) {
                        throw new FailedPollWaitException('Failed to wait for events', FailedPollOperationException::ERROR_SYSTEM);
                    }
                    $ready = true;
                }

                $events = [];
                foreach ($active as $watcher) {
                    if (count($events) >= $limit) {
                        break;
                    }
                    $watched = $watcher->getWatchedEvents();
                    $stream = $watcher->__elephcStream();
                    $inRead = $ready && in_array($stream, $read, true);
                    $inWrite = $ready && in_array($stream, $write, true);
                    $isBuffered = in_array($watcher, $buffered, true);
                    $triggered = [];
                    if ($isBuffered) {
                        $triggered[] = Event::Read;
                        if ($inWrite && in_array(Event::Write, $watched, true)) {
                            $triggered[] = Event::Write;
                        }
                    } else {
                        $flags = ($inRead || $inWrite) ? __elephc_poll_revents($stream) : 0;
                        $hungUp = ($flags & 16) !== 0;
                        $readHangUp = ($flags & 8192) !== 0;
                        // POLLERR is Event::Error. POLLPRI is priority data, which PHP 8.6 does not
                        // expose as an event; it must not be reported as Error.
                        $errored = ($flags & 8) !== 0;
                        if (($flags & 32) !== 0) {
                            continue;
                        }
                        if ($hungUp && in_array(Event::HangUp, $watched, true)) {
                            $triggered[] = Event::HangUp;
                        }
                        if (($hungUp || $readHangUp) && in_array(Event::ReadHangUp, $watched, true)) {
                            $triggered[] = Event::ReadHangUp;
                        }
                        if (($inRead || $hungUp) && in_array(Event::Read, $watched, true)) {
                            $triggered[] = Event::Read;
                        }
                        if ($inWrite && in_array(Event::Write, $watched, true)) {
                            $triggered[] = Event::Write;
                        }
                        if ($errored && in_array(Event::Error, $watched, true)) {
                            $triggered[] = Event::Error;
                        }
                    }
                    if ($triggered !== []) {
                        $watcher->__elephcMarkTriggered($triggered);
                        $events[] = $watcher;
                    }
                }
                if ($events !== []) {
                    return $events;
                }

                if ($deadline === null) {
                    // A level-ready descriptor that produced no reportable event would
                    // otherwise spin: give the peer a moment to change state.
                    if ($ready) {
                        usleep(1000);
                    }
                    continue;
                }
                $remaining = $deadline - hrtime(true);
                if ($remaining <= 0) {
                    return [];
                }
                $seconds = intdiv($remaining, 1000000000);
                $micros = intdiv($remaining % 1000000000, 1000);
                if ($ready) {
                    // The descriptor is still level-ready but has nothing this watcher
                    // reports; poll for the error/hang-up transition at 1 ms granularity
                    // instead of returning early or spinning.
                    $nap = $remaining < 1000000 ? $remaining : 1000000;
                    usleep((int) intdiv($nap, 1000));
                }
            }
        }

        public function getBackend(): Backend {
            return $this->backend;
        }

        public function __elephcRemoveWatcher(Watcher $watcher): void {
            foreach ($this->watchers as $index => $candidate) {
                if ($candidate === $watcher) {
                    unset($this->watchers[$index]);
                    $this->watchers = array_values($this->watchers);
                    return;
                }
            }
        }

        public function validateEvents(array $events): void {
            if ($events === []) {
                throw new \ValueError('At least one event must be watched');
            }
            foreach ($events as $event) {
                if (!$event instanceof Event) {
                    throw new \TypeError('Watched events must be of type Io\\Poll\\Event');
                }
                if ($event === Event::EdgeTriggered) {
                    throw new BackendUnavailableException('Backend ' . $this->backend->name . ' does not support edge triggering');
                }
            }
        }

        public function __debugInfo(): array {
            return [];
        }

        public function __clone() {
            throw new \Error('Trying to clone an uncloneable object of class Io\\Poll\\Context');
        }
    }

    class PollException extends \Io\IoException {}

    abstract class FailedPollOperationException extends PollException {
        public const ERROR_NONE = 0;
        public const ERROR_SYSTEM = 1;
        public const ERROR_NOMEM = 2;
        public const ERROR_INVALID = 3;
        public const ERROR_EXISTS = 4;
        public const ERROR_NOTFOUND = 5;
        public const ERROR_TIMEOUT = 6;
        public const ERROR_INTERRUPTED = 7;
        public const ERROR_PERMISSION = 8;
        public const ERROR_TOOBIG = 9;
        public const ERROR_AGAIN = 10;
        public const ERROR_NOSUPPORT = 11;
    }

    class FailedContextInitializationException extends FailedPollOperationException {}
    class FailedHandleAddException extends FailedPollOperationException {}
    class FailedWatcherModificationException extends FailedPollOperationException {}
    class FailedPollWaitException extends FailedPollOperationException {}
    class BackendUnavailableException extends PollException {}
    class InactiveWatcherException extends PollException {}
    class HandleAlreadyWatchedException extends PollException {}
    class InvalidHandleException extends PollException {}
}

namespace {
    final class StreamPollHandle implements \Io\Poll\Handle {
        private mixed $stream;
        private bool $constructed = false;

        public function __construct($stream) {
            if ($this->constructed) {
                throw new \Error('StreamPollHandle object is already constructed');
            }
            if (!is_resource($stream) || get_resource_type($stream) === 'Unknown') {
                throw new \TypeError('StreamPollHandle::__construct(): Argument #1 ($stream) must be an open stream resource');
            }
            $this->constructed = true;
            $this->stream = $stream;
        }

        public function getStream(): mixed {
            return $this->stream;
        }

        public function isValid(): bool {
            return is_resource($this->stream) && get_resource_type($this->stream) !== 'Unknown';
        }

        public function __debugInfo(): array {
            return [];
        }

        public function __clone() {
            throw new \Error('Trying to clone an uncloneable object of class StreamPollHandle');
        }
    }
}
"#;

/// Returns whether `usage` reaches the `Io\Poll` surface.
pub(crate) fn requested(usage: &crate::prelude_prune::usage::Usage) -> bool {
    usage.classes.iter().any(|name| {
        name == "streampollhandle"
            || name == "io\\poll\\handle"
            || name.starts_with("io\\poll\\")
    })
}

/// Injects the surface when only an autoloaded class reaches it.
///
/// The early `core_prelude::inject_if_used` pass runs before `autoload::run`, so a class
/// loaded from a PSR-4 file is not part of the AST it scans. This late pass runs after
/// autoloading, resolves the prelude's own names, and skips work when the early pass already
/// recorded the group.
#[allow(dead_code)]
pub(crate) fn inject_after_autoload(
    program: Program,
    inventory: &mut crate::optimize::reachability::PreludeInventory,
) -> Result<Program, crate::errors::CompileError> {
    with_compiler_stack(|| {
        if inventory.groups.contains_key("io-poll") {
            return Ok(program);
        }
        let usage = crate::prelude_prune::usage::collect(&program);
        let requested = crate::codegen_support::compile_php_version()
            >= crate::php_version::PhpVersion::Php86
            && requested(&usage);
        if !requested {
            return Ok(program);
        }
        let mut combined = crate::name_resolver::resolve(declarations())?;
        inventory.record_program("io-poll", &combined);
        combined.extend(program);
        Ok(combined)
    })
}

/// Returns the parsed `Io\Poll` declarations.
pub(crate) fn declarations() -> Program {
    with_compiler_stack(|| {
        let tokens = crate::lexer::tokenize(IO_POLL_PRELUDE_SRC)
            .expect("io poll prelude must tokenize");
        crate::parser::parse_internal(&tokens).expect("io poll prelude must parse")
    })
}
