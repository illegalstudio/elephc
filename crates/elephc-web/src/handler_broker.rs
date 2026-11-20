//! Purpose:
//! Starts a threadless isolated-handler broker before Tokio and exposes async
//! request dispatch with reliable cancellation to each web worker.
//!
//! Called from:
//! - `crate::isolated_worker::serve`, before it constructs the Tokio runtime.
//! - `crate::isolated_worker::run_handler_isolated`, for async request dispatch.
//!
//! Key details:
//! - Pool and request isolation share dispatch IDs and response-stream framing.
//! - Descriptor transfers are acknowledged before the sender releases its copy.
//! - A response lease owns the concurrency permit and cancels its exact ID on drop.
//! - The broker process owns and reaps every handler PID; it never ignores `SIGCHLD`.
//! - Worker `SIGTERM` is forwarded to the broker so descendants are reaped before
//!   the worker exits, including under container PID 1 implementations that do not reap orphans.

mod control;
mod process;

use std::io;
use std::os::fd::{AsRawFd, OwnedFd, RawFd};
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicI32, AtomicU32, AtomicU64, Ordering};
use std::sync::{mpsc, Arc};

use tokio::io::unix::AsyncFd;
use tokio::sync::{oneshot, Mutex, OwnedMutexGuard, OwnedSemaphorePermit, Semaphore};

use crate::handler_ipc::{self, HandlerRequest, ResponseFrame, ResponseStart};

/// Worker exit status reserved for a reaped handler-broker failure.
const BROKER_FAILURE_EXIT_CODE: libc::c_int = 87;
/// Small stack sufficient for the broker monitor's blocking `waitpid` loop.
const BROKER_MONITOR_STACK_BYTES: usize = 64 * 1024;
/// Small stack sufficient for forwarding cancellation IDs through one socket.
const CANCEL_SENDER_STACK_BYTES: usize = 64 * 1024;
/// Per-request execution-time ceiling inherited by every handler process.
pub(super) static MAX_EXEC_SECS: AtomicU32 = AtomicU32::new(0);
/// Exact broker PID owned by this isolated worker, read by its `SIGTERM` handler.
static WORKER_BROKER_PID: AtomicI32 = AtomicI32::new(0);
/// Sentinel meaning broker death was not requested by the worker.
const NO_REQUESTED_WORKER_EXIT: libc::c_int = -1;
/// Exit code the broker monitor must use after an intentional broker shutdown.
static REQUESTED_WORKER_EXIT: AtomicI32 = AtomicI32::new(NO_REQUESTED_WORKER_EXIT);

/// Handler process model implemented by the threadless broker.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BrokerMode {
    /// Reuse a fixed set of persistent handler processes.
    Pool,
    /// Fork and reap one disposable handler for every request.
    Request,
}

/// Immutable process-lifecycle settings inherited by the broker.
#[derive(Clone, Copy)]
pub(super) struct BrokerConfig {
    mode: BrokerMode,
    concurrency: usize,
    max_handler_requests: usize,
}

/// Pre-runtime broker endpoint that becomes async only after Tokio is built.
pub(crate) struct PrestartedBroker {
    dispatch: OwnedFd,
    cancel: mpsc::Sender<u64>,
    concurrency: usize,
}

/// Cloneable async dispatch handle shared by the worker's connection tasks.
#[derive(Clone)]
pub(crate) struct HandlerBroker {
    dispatch: Arc<Mutex<AsyncFd<OwnedFd>>>,
    cancel: mpsc::Sender<u64>,
    permits: Arc<Semaphore>,
    next_id: Arc<AtomicU64>,
}

/// One accepted handler response stream and its concurrency lease.
pub(crate) struct HandlerResponse {
    pub(crate) start: ResponseStart,
    reader: tokio::net::unix::OwnedReadHalf,
    lease: RequestLease,
    _permit: OwnedSemaphorePermit,
}

/// Drop guard that cancels an incomplete dispatch by its exact broker ID.
struct RequestLease {
    id: u64,
    cancel: mpsc::Sender<u64>,
    active: bool,
}

impl RequestLease {
    /// Creates an active lease only after broker ownership has been acknowledged.
    fn acknowledged(id: u64, cancel: mpsc::Sender<u64>) -> Self {
        Self {
            id,
            cancel,
            active: true,
        }
    }
}

/// Terminates an over-time handler without affecting its broker or worker.
extern "C" fn handle_exec_timeout(_signal: libc::c_int) {
    const MESSAGE: &[u8] =
        b"elephc-web: handler exceeded --max-execution-time; terminating handler\n";
    unsafe {
        libc::write(2, MESSAGE.as_ptr().cast(), MESSAGE.len());
        libc::_exit(1);
    }
}

/// Installs the alarm handler before the broker and handler processes are created.
fn configure_execution_timeout(seconds: u32) {
    MAX_EXEC_SECS.store(seconds, Ordering::Relaxed);
    if seconds == 0 {
        return;
    }
    unsafe {
        let mut action: libc::sigaction = std::mem::zeroed();
        action.sa_sigaction =
            handle_exec_timeout as extern "C" fn(libc::c_int) as libc::sighandler_t;
        libc::sigemptyset(&mut action.sa_mask);
        action.sa_flags = 0;
        libc::sigaction(libc::SIGALRM, &action, std::ptr::null_mut());
    }
}

/// Forwards worker termination to its broker without exiting from the signal handler.
extern "C" fn handle_worker_shutdown(_signal: libc::c_int) {
    REQUESTED_WORKER_EXIT.store(0, Ordering::SeqCst);
    let pid = WORKER_BROKER_PID.load(Ordering::SeqCst);
    if pid > 0 {
        unsafe {
            libc::kill(pid, libc::SIGTERM);
        }
    }
}

/// Installs cooperative worker shutdown before the broker is forked.
fn install_worker_shutdown_handler() -> io::Result<()> {
    WORKER_BROKER_PID.store(0, Ordering::SeqCst);
    REQUESTED_WORKER_EXIT.store(NO_REQUESTED_WORKER_EXIT, Ordering::SeqCst);
    unsafe {
        let mut action: libc::sigaction = std::mem::zeroed();
        action.sa_sigaction =
            handle_worker_shutdown as extern "C" fn(libc::c_int) as libc::sighandler_t;
        libc::sigemptyset(&mut action.sa_mask);
        action.sa_flags = 0;
        if libc::sigaction(libc::SIGTERM, &action, std::ptr::null_mut()) != 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

impl PrestartedBroker {
    /// Forks a broker with the requested isolation model and live-handler bound.
    pub(crate) fn start(
        handler: extern "C" fn(),
        max_exec_secs: u32,
        mode: BrokerMode,
        concurrency: usize,
        max_handler_requests: usize,
    ) -> io::Result<Self> {
        install_worker_shutdown_handler()?;
        configure_execution_timeout(max_exec_secs);
        let concurrency = concurrency.max(1);
        let (worker_dispatch, broker_dispatch) = control::datagram_pair()?;
        let (worker_cancel, broker_cancel) = control::datagram_pair()?;
        let worker_pid = unsafe { libc::getpid() };
        let config = BrokerConfig {
            mode,
            concurrency,
            max_handler_requests,
        };
        let pid = unsafe { libc::fork() };
        if pid == 0 {
            drop(worker_dispatch);
            drop(worker_cancel);
            unsafe {
                process::broker_loop(
                    broker_dispatch.as_raw_fd(),
                    broker_cancel.as_raw_fd(),
                    worker_pid,
                    handler,
                    config,
                );
                libc::_exit(0);
            }
        }
        if pid < 0 {
            return Err(io::Error::last_os_error());
        }
        WORKER_BROKER_PID.store(pid, Ordering::SeqCst);
        if REQUESTED_WORKER_EXIT.load(Ordering::SeqCst) != NO_REQUESTED_WORKER_EXIT {
            unsafe {
                libc::kill(pid, libc::SIGTERM);
            }
        }
        drop(broker_dispatch);
        drop(broker_cancel);
        if let Err(error) = control::set_nonblocking(worker_dispatch.as_raw_fd()) {
            unsafe { terminate_and_reap_broker(pid) };
            return Err(error);
        }
        let cancel = match start_cancel_sender(worker_cancel) {
            Ok(cancel) => cancel,
            Err(error) => {
                unsafe { terminate_and_reap_broker(pid) };
                return Err(error);
            }
        };
        if let Err(error) = start_broker_monitor(pid) {
            unsafe { terminate_and_reap_broker(pid) };
            return Err(error);
        }
        Ok(Self {
            dispatch: worker_dispatch,
            cancel,
            concurrency,
        })
    }

    /// Registers the already-running broker endpoint with Tokio's readiness driver.
    pub(crate) fn into_async(self) -> io::Result<HandlerBroker> {
        Ok(HandlerBroker {
            dispatch: Arc::new(Mutex::new(AsyncFd::new(self.dispatch)?)),
            cancel: self.cancel,
            permits: Arc::new(Semaphore::new(self.concurrency)),
            next_id: Arc::new(AtomicU64::new(1)),
        })
    }
}

/// Starts the worker thread that sends cancellation IDs without blocking Tokio.
fn start_cancel_sender(socket: OwnedFd) -> io::Result<mpsc::Sender<u64>> {
    let (sender, receiver) = mpsc::channel::<u64>();
    std::thread::Builder::new()
        .name("elephc-web-cancel-sender".to_string())
        .stack_size(CANCEL_SENDER_STACK_BYTES)
        .spawn(move || {
            while let Ok(id) = receiver.recv() {
                if control::send_id(socket.as_raw_fd(), id).is_err() {
                    break;
                }
            }
        })?;
    Ok(sender)
}

/// Starts the post-fork worker thread that owns reaping the dedicated broker.
fn start_broker_monitor(pid: libc::pid_t) -> io::Result<()> {
    std::thread::Builder::new()
        .name("elephc-web-broker-monitor".to_string())
        .stack_size(BROKER_MONITOR_STACK_BYTES)
        .spawn(move || unsafe {
            if let Some(status) = reap_broker(pid) {
                let requested_exit = REQUESTED_WORKER_EXIT.load(Ordering::SeqCst);
                if requested_exit != NO_REQUESTED_WORKER_EXIT {
                    libc::_exit(requested_exit);
                }
                if libc::WIFSIGNALED(status) {
                    eprintln!(
                        "elephc-web: handler broker terminated by signal {}",
                        libc::WTERMSIG(status)
                    );
                } else if libc::WIFEXITED(status) {
                    eprintln!(
                        "elephc-web: handler broker exited with status {}",
                        libc::WEXITSTATUS(status)
                    );
                }
            }
            libc::_exit(BROKER_FAILURE_EXIT_CODE);
        })
        .map(|_| ())
}

/// Stops and reaps the isolated worker's broker before a planned recycle exit.
///
/// The broker monitor owns `waitpid`, so this thread requests exit code 86 and
/// waits for the monitor to terminate the process after all handlers are reaped.
pub(crate) fn finish_worker_recycle() -> ! {
    REQUESTED_WORKER_EXIT.store(crate::isolated_worker::RECYCLE_EXIT_CODE, Ordering::SeqCst);
    let pid = WORKER_BROKER_PID.load(Ordering::SeqCst);
    if pid <= 0 {
        unsafe { libc::_exit(crate::isolated_worker::RECYCLE_EXIT_CODE) }
    }
    unsafe {
        libc::kill(pid, libc::SIGTERM);
        loop {
            libc::pause();
        }
    }
}

/// Terminates a broker whose worker-side setup failed, then reaps it synchronously.
unsafe fn terminate_and_reap_broker(pid: libc::pid_t) {
    libc::kill(pid, libc::SIGTERM);
    let _ = reap_broker(pid);
}

/// Waits for the exact broker child and returns its wait status when available.
unsafe fn reap_broker(pid: libc::pid_t) -> Option<libc::c_int> {
    loop {
        let mut status = 0;
        let waited = libc::waitpid(pid, &mut status, 0);
        if waited == pid {
            return Some(status);
        }
        if waited < 0 && io::Error::last_os_error().raw_os_error() == Some(libc::EINTR) {
            continue;
        }
        return None;
    }
}

impl HandlerBroker {
    /// Dispatches one request and waits only until status and headers are committed.
    pub(crate) async fn dispatch(&self, request: HandlerRequest) -> io::Result<HandlerResponse> {
        let permit = Arc::clone(&self.permits)
            .acquire_owned()
            .await
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "handler broker closed"))?;
        let (worker_stream, broker_stream) = UnixStream::pair()?;
        worker_stream.set_nonblocking(true)?;
        control::set_close_on_exec(worker_stream.as_raw_fd())?;
        control::set_close_on_exec(broker_stream.as_raw_fd())?;
        let dispatch_control = Arc::clone(&self.dispatch).lock_owned().await;
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let lease = Self::send_channel(
            dispatch_control,
            id,
            broker_stream.as_raw_fd(),
            broker_stream,
            self.cancel.clone(),
        )
        .await?;

        let stream = tokio::net::UnixStream::from_std(worker_stream)?;
        let (mut reader, mut writer) = stream.into_split();
        handler_ipc::write_request_async(&mut writer, &request)
            .await
            .map_err(|error| {
                io::Error::new(
                    error.kind(),
                    format!("could not write handler request {id}: {error}"),
                )
            })?;
        // The request frame is length-prefixed, so the handler never relies on EOF to
        // delimit it. Dropping the write half still attempts SHUT_WR, but deliberately
        // ignores ENOTCONN when a fast handler has already returned and closed its peer.
        drop(writer);
        drop(request);
        let first_frame = handler_ipc::read_response_frame(&mut reader)
            .await
            .map_err(|error| {
                io::Error::new(
                    error.kind(),
                    format!("could not read handler response {id}: {error}"),
                )
            })?;
        match first_frame {
            ResponseFrame::Start(start) => Ok(HandlerResponse {
                start,
                reader,
                lease,
                _permit: permit,
            }),
            ResponseFrame::Chunk(_) | ResponseFrame::End => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "handler response did not begin with status and headers",
            )),
        }
    }

    /// Transfers one request channel and waits until the broker acknowledges ownership.
    async fn send_channel(
        dispatch: OwnedMutexGuard<AsyncFd<OwnedFd>>,
        id: u64,
        channel: RawFd,
        broker_stream: UnixStream,
        cancel: mpsc::Sender<u64>,
    ) -> io::Result<RequestLease> {
        loop {
            let mut writable = dispatch.writable().await?;
            match writable.try_io(|fd| {
                control::send_dispatch(fd.get_ref().as_raw_fd(), id, channel, true)
            }) {
                Ok(result) => {
                    result?;
                    break;
                }
                Err(_) => continue,
            }
        }
        let (acknowledged, acknowledgement) = oneshot::channel();
        tokio::spawn(async move {
            let result = Self::receive_ack(&dispatch, id).await;
            drop(broker_stream);
            let result = match result {
                Ok(()) => Ok(RequestLease::acknowledged(id, cancel.clone())),
                Err(error) => {
                    let _ = cancel.send(id);
                    Err(error)
                }
            };
            // If the HTTP task disappeared after sendmsg, dropping the active
            // lease here emits cancellation after the ACK has been drained.
            let _ = acknowledged.send(result);
            drop(dispatch);
        });
        acknowledgement.await.map_err(|_| {
            io::Error::new(
                io::ErrorKind::BrokenPipe,
                "handler broker ACK owner stopped unexpectedly",
            )
        })?
    }

    /// Receives and validates one broker acknowledgement while its mutex is held.
    async fn receive_ack(dispatch: &AsyncFd<OwnedFd>, id: u64) -> io::Result<()> {
        loop {
            let mut readable = dispatch.readable().await?;
            match readable.try_io(|fd| unsafe {
                control::recv_id(fd.get_ref().as_raw_fd())?.ok_or_else(|| {
                    io::Error::new(io::ErrorKind::BrokenPipe, "handler broker closed before ACK")
                })
            }) {
                Ok(Ok(acknowledged)) if acknowledged == id => return Ok(()),
                Ok(Ok(acknowledged)) => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("handler broker acknowledged request {acknowledged}, expected {id}"),
                    ));
                }
                Ok(Err(error)) => return Err(error),
                Err(_) => continue,
            }
        }
    }
}

impl HandlerResponse {
    /// Reads the next response frame and completes the lease only on an explicit end.
    pub(crate) async fn next_frame(&mut self) -> io::Result<ResponseFrame> {
        let frame = handler_ipc::read_response_frame(&mut self.reader).await?;
        if matches!(frame, ResponseFrame::End) {
            self.lease.active = false;
        }
        Ok(frame)
    }
}

impl Drop for RequestLease {
    /// Cancels a handler whose response did not reach its successful end frame.
    fn drop(&mut self) {
        if self.active {
            let _ = self.cancel.send(self.id);
            self.active = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::TryRecvError;
    use std::time::Duration;

    /// Builds the smallest request snapshot needed to exercise broker dispatch cancellation.
    fn empty_request() -> HandlerRequest {
        HandlerRequest {
            method: "GET".to_string(),
            uri: "/".to_string(),
            path: "/".to_string(),
            query: String::new(),
            headers: Vec::new(),
            body: Vec::new(),
            remote_addr: "127.0.0.1".to_string(),
            remote_port: 1,
            server_addr: "127.0.0.1".to_string(),
            server_port: 2,
            protocol: "HTTP/1.1".to_string(),
            request_scheme: "http".to_string(),
            https: false,
        }
    }

    /// Verifies cancellation while waiting for the serialized dispatch lock emits no orphan ID.
    #[test]
    fn cancellation_before_dispatch_does_not_notify_broker() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("build current-thread test runtime");
        runtime.block_on(async {
            let (worker_dispatch, broker_dispatch) = control::datagram_pair()
                .expect("create dispatch socket pair");
            control::set_nonblocking(worker_dispatch.as_raw_fd())
                .expect("make worker dispatch socket nonblocking");
            let (cancel, cancellations) = mpsc::channel();
            let broker = HandlerBroker {
                dispatch: Arc::new(Mutex::new(
                    AsyncFd::new(worker_dispatch).expect("register dispatch socket"),
                )),
                cancel,
                permits: Arc::new(Semaphore::new(1)),
                next_id: Arc::new(AtomicU64::new(1)),
            };
            let dispatch_lock = Arc::clone(&broker.dispatch).lock_owned().await;

            let cancelled = tokio::time::timeout(
                Duration::from_millis(10),
                broker.dispatch(empty_request()),
            )
            .await;
            assert!(cancelled.is_err(), "dispatch unexpectedly passed the held lock");
            assert_eq!(
                cancellations.try_recv(),
                Err(TryRecvError::Empty),
                "a request that was never dispatched emitted an orphan cancellation ID"
            );

            drop(dispatch_lock);
            drop(broker_dispatch);
        });
    }

    /// Verifies cancellation after descriptor send but before ACK still drains
    /// that ACK, emits one cancellation, and leaves the next dispatch aligned.
    #[test]
    fn cancellation_between_send_and_ack_does_not_poison_dispatch_socket() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("build current-thread test runtime");
        runtime.block_on(async {
            let (worker_dispatch, broker_dispatch) = control::datagram_pair()
                .expect("create dispatch socket pair");
            control::set_nonblocking(worker_dispatch.as_raw_fd())
                .expect("make worker dispatch socket nonblocking");
            let dispatch = Arc::new(Mutex::new(
                AsyncFd::new(worker_dispatch).expect("register dispatch socket"),
            ));
            let (cancel, cancellations) = mpsc::channel();
            let (first_received, first_received_waiter) = oneshot::channel();
            let (release_first_ack, release_first_ack_waiter) = mpsc::channel();
            let peer = std::thread::spawn(move || unsafe {
                let first = control::recv_dispatch(broker_dispatch.as_raw_fd())
                    .expect("receive first dispatch")
                    .expect("first dispatch socket closed");
                first_received
                    .send(())
                    .expect("report first dispatch receipt");
                release_first_ack_waiter
                    .recv_timeout(Duration::from_secs(1))
                    .expect("wait for first ACK release");
                control::send_id(broker_dispatch.as_raw_fd(), first.id)
                    .expect("acknowledge first dispatch");
                libc::close(first.channel);

                let second = control::recv_dispatch(broker_dispatch.as_raw_fd())
                    .expect("receive second dispatch")
                    .expect("second dispatch socket closed");
                control::send_id(broker_dispatch.as_raw_fd(), second.id)
                    .expect("acknowledge second dispatch");
                libc::close(second.channel);
            });

            let (_first_worker, first_broker) = UnixStream::pair().expect("first request channel");
            let first_control = Arc::clone(&dispatch).lock_owned().await;
            let first = tokio::spawn(HandlerBroker::send_channel(
                    first_control,
                    1,
                    first_broker.as_raw_fd(),
                    first_broker,
                    cancel.clone(),
                ));
            first_received_waiter
                .await
                .expect("peer stopped before receiving first dispatch");
            first.abort();
            match first.await {
                Err(error) if error.is_cancelled() => {}
                Err(error) => panic!("first dispatch task failed unexpectedly: {error}"),
                Ok(_) => panic!("first dispatch unexpectedly received its gated ACK"),
            }
            release_first_ack
                .send(())
                .expect("release first broker ACK");

            let (cancelled, cancellations) = tokio::task::spawn_blocking(move || {
                let cancelled = cancellations.recv_timeout(Duration::from_secs(1));
                (cancelled, cancellations)
            })
            .await
            .expect("cancellation observer panicked");
            assert_eq!(
                cancelled,
                Ok(1),
                "cancelled ACK waiter did not cancel its acknowledged request"
            );

            let (_second_worker, second_broker) =
                UnixStream::pair().expect("second request channel");
            let second_control = Arc::clone(&dispatch).lock_owned().await;
            let mut second = HandlerBroker::send_channel(
                second_control,
                2,
                second_broker.as_raw_fd(),
                second_broker,
                cancel,
            )
            .await
            .expect("second dispatch must consume its own ACK");
            second.active = false;
            peer.join().expect("dispatch peer panicked");
            assert_eq!(cancellations.try_recv(), Err(TryRecvError::Empty));
        });
    }
}
