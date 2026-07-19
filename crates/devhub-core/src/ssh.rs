use std::io::{Read, Write};
use std::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, ExitStatus, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use crate::{CancellationToken, OPERATION_CANCELLED};

const POLL_INTERVAL: Duration = Duration::from_millis(10);

#[derive(Debug)]
pub(crate) struct SshOutput {
    pub(crate) status: ExitStatus,
    pub(crate) stdout: Vec<u8>,
    pub(crate) stderr: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OutputStream {
    Stdout,
    Stderr,
}

impl OutputStream {
    fn label(self) -> &'static str {
        match self {
            Self::Stdout => "stdout",
            Self::Stderr => "stderr",
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum SshRunError {
    Cancelled,
    Timeout {
        operation: String,
        timeout: Duration,
    },
    OutputLimitExceeded {
        stream: OutputStream,
        limit: usize,
    },
    Start(String),
    Input(String),
    OutputRead {
        stream: OutputStream,
        message: String,
    },
    Wait(String),
    WorkerPanicked(&'static str),
}

impl std::fmt::Display for SshRunError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => formatter.write_str(OPERATION_CANCELLED),
            Self::Timeout { operation, timeout } => {
                write!(formatter, "{operation} timed out after ")?;
                if timeout.subsec_nanos() == 0 && timeout.as_secs() > 0 {
                    write!(formatter, "{}s", timeout.as_secs())
                } else {
                    write!(formatter, "{}ms", timeout.as_millis())
                }
            }
            Self::OutputLimitExceeded { stream, limit } => write!(
                formatter,
                "OutputLimitExceeded: SSH {} exceeded {limit} bytes",
                stream.label()
            ),
            Self::Start(message) | Self::Input(message) | Self::Wait(message) => {
                formatter.write_str(message)
            }
            Self::WorkerPanicked(message) => formatter.write_str(message),
            Self::OutputRead { stream, message } => {
                write!(formatter, "reading SSH {}: {message}", stream.label())
            }
        }
    }
}

impl std::error::Error for SshRunError {}

pub(crate) struct SshRunner<'a> {
    context: RunnerContext<'a>,
    timeout: Duration,
    output_limit: usize,
    cancellation: &'a CancellationToken,
}

#[derive(Debug, Clone, Copy)]
enum RunnerContext<'a> {
    Ssh(&'a str),
    Process(&'a str),
}

impl RunnerContext<'_> {
    fn operation(self) -> String {
        match self {
            Self::Ssh(host) => format!("SSH operation for {host}"),
            Self::Process(label) => label.to_string(),
        }
    }

    fn start(self, error: impl std::fmt::Display) -> String {
        match self {
            Self::Ssh(host) => format!("starting ssh for {host}: {error}"),
            Self::Process(label) => format!("starting {label}: {error}"),
        }
    }

    fn input(self, error: impl std::fmt::Display) -> String {
        match self {
            Self::Ssh(host) => format!("sending SSH request to {host}: {error}"),
            Self::Process(label) => format!("sending input to {label}: {error}"),
        }
    }
}

impl<'a> SshRunner<'a> {
    pub(crate) fn new(
        host: &'a str,
        timeout: Duration,
        output_limit: usize,
        cancellation: &'a CancellationToken,
    ) -> Self {
        Self {
            context: RunnerContext::Ssh(host),
            timeout,
            output_limit,
            cancellation,
        }
    }

    pub(crate) fn new_process(
        label: &'a str,
        timeout: Duration,
        output_limit: usize,
        cancellation: &'a CancellationToken,
    ) -> Self {
        Self {
            context: RunnerContext::Process(label),
            timeout,
            output_limit,
            cancellation,
        }
    }

    pub(crate) fn run(&self, mut command: Command, input: &[u8]) -> Result<SshOutput, SshRunError> {
        if self.cancellation.is_cancelled() {
            return Err(SshRunError::Cancelled);
        }

        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let child = command
            .spawn()
            .map_err(|error| SshRunError::Start(self.context.start(error)))?;
        let mut child = ManagedChild::new(child);
        let stdin = child.take_stdin()?;
        let stdout = child.take_stdout()?;
        let stderr = child.take_stderr()?;
        let (events_tx, events_rx) = mpsc::channel();

        let stdin_worker = spawn_stdin_worker(stdin, input.to_vec(), events_tx.clone());
        let stdout_worker = spawn_output_worker(
            stdout,
            OutputStream::Stdout,
            self.output_limit,
            events_tx.clone(),
        );
        let stderr_worker =
            spawn_output_worker(stderr, OutputStream::Stderr, self.output_limit, events_tx);

        let started = Instant::now();
        let outcome = loop {
            match child.poll() {
                Ok(Some(status)) => break Ok(status),
                Ok(None) => {}
                Err(error) => break Err(error),
            }

            if self.cancellation.is_cancelled() {
                break Err(SshRunError::Cancelled);
            }
            if started.elapsed() >= self.timeout {
                break Err(SshRunError::Timeout {
                    operation: self.context.operation(),
                    timeout: self.timeout,
                });
            }

            match events_rx.try_recv() {
                Ok(WorkerEvent::StdinFailed(message)) => {
                    break Err(SshRunError::Input(self.context.input(message)));
                }
                Ok(WorkerEvent::OutputLimitExceeded(stream)) => {
                    break Err(SshRunError::OutputLimitExceeded {
                        stream,
                        limit: self.output_limit,
                    });
                }
                Ok(WorkerEvent::OutputReadFailed(stream, message)) => {
                    break Err(SshRunError::OutputRead { stream, message });
                }
                Err(mpsc::TryRecvError::Empty | mpsc::TryRecvError::Disconnected) => {}
            }

            thread::sleep(POLL_INTERVAL);
        };

        let (status, terminal_error) = match outcome {
            Ok(status) => (Some(status), None),
            Err(error) => {
                child.terminate_and_reap()?;
                (None, Some(error))
            }
        };

        let stdin_result = join_worker(stdin_worker, "SSH stdin worker panicked")?;
        let stdout_result = join_worker(stdout_worker, "SSH stdout worker panicked")?;
        let stderr_result = join_worker(stderr_worker, "SSH stderr worker panicked")?;

        if let Some(error) = terminal_error {
            return Err(error);
        }
        stdin_result.map_err(|message| SshRunError::Input(self.context.input(message)))?;
        let stdout = stdout_result.map_err(|error| error.into_run_error(self.output_limit))?;
        let stderr = stderr_result.map_err(|error| error.into_run_error(self.output_limit))?;

        Ok(SshOutput {
            status: status.expect("successful SSH outcome must include an exit status"),
            stdout,
            stderr,
        })
    }
}

fn spawn_stdin_worker(
    mut stdin: ChildStdin,
    input: Vec<u8>,
    events: mpsc::Sender<WorkerEvent>,
) -> thread::JoinHandle<Result<(), String>> {
    thread::spawn(move || {
        let result = stdin.write_all(&input).map_err(|error| error.to_string());
        if let Err(message) = &result {
            let _ = events.send(WorkerEvent::StdinFailed(message.clone()));
        }
        result
    })
}

fn spawn_output_worker<R: Read + Send + 'static>(
    reader: R,
    stream: OutputStream,
    limit: usize,
    events: mpsc::Sender<WorkerEvent>,
) -> thread::JoinHandle<Result<Vec<u8>, StreamReadError>> {
    thread::spawn(move || read_bounded(reader, stream, limit, events))
}

fn read_bounded(
    mut reader: impl Read,
    stream: OutputStream,
    limit: usize,
    events: mpsc::Sender<WorkerEvent>,
) -> Result<Vec<u8>, StreamReadError> {
    let mut output = Vec::with_capacity(limit.min(64 * 1024));
    let mut chunk = [0_u8; 16 * 1024];
    loop {
        let read = match reader.read(&mut chunk) {
            Ok(read) => read,
            Err(error) => {
                let message = error.to_string();
                let _ = events.send(WorkerEvent::OutputReadFailed(stream, message.clone()));
                return Err(StreamReadError::Io { stream, message });
            }
        };
        if read == 0 {
            return Ok(output);
        }
        if read > limit.saturating_sub(output.len()) {
            let _ = events.send(WorkerEvent::OutputLimitExceeded(stream));
            return Err(StreamReadError::Limit { stream });
        }
        output.extend_from_slice(&chunk[..read]);
    }
}

fn join_worker<T>(
    worker: thread::JoinHandle<T>,
    panic_message: &'static str,
) -> Result<T, SshRunError> {
    worker
        .join()
        .map_err(|_| SshRunError::WorkerPanicked(panic_message))
}

enum WorkerEvent {
    StdinFailed(String),
    OutputLimitExceeded(OutputStream),
    OutputReadFailed(OutputStream, String),
}

#[derive(Debug)]
enum StreamReadError {
    Limit {
        stream: OutputStream,
    },
    Io {
        stream: OutputStream,
        message: String,
    },
}

impl StreamReadError {
    fn into_run_error(self, limit: usize) -> SshRunError {
        match self {
            Self::Limit { stream } => SshRunError::OutputLimitExceeded { stream, limit },
            Self::Io { stream, message } => SshRunError::OutputRead { stream, message },
        }
    }
}

struct ManagedChild {
    child: Child,
    status: Option<ExitStatus>,
}

impl ManagedChild {
    fn new(child: Child) -> Self {
        Self {
            child,
            status: None,
        }
    }

    fn take_stdin(&mut self) -> Result<ChildStdin, SshRunError> {
        self.child
            .stdin
            .take()
            .ok_or_else(|| SshRunError::Input("opening SSH standard input failed".to_string()))
    }

    fn take_stdout(&mut self) -> Result<ChildStdout, SshRunError> {
        self.child
            .stdout
            .take()
            .ok_or_else(|| SshRunError::OutputRead {
                stream: OutputStream::Stdout,
                message: "opening SSH standard output failed".to_string(),
            })
    }

    fn take_stderr(&mut self) -> Result<ChildStderr, SshRunError> {
        self.child
            .stderr
            .take()
            .ok_or_else(|| SshRunError::OutputRead {
                stream: OutputStream::Stderr,
                message: "opening SSH standard error failed".to_string(),
            })
    }

    fn poll(&mut self) -> Result<Option<ExitStatus>, SshRunError> {
        if let Some(status) = self.status {
            return Ok(Some(status));
        }
        match self.child.try_wait() {
            Ok(Some(status)) => {
                self.status = Some(status);
                Ok(Some(status))
            }
            Ok(None) => Ok(None),
            Err(error) => Err(SshRunError::Wait(format!(
                "waiting for SSH operation: {error}"
            ))),
        }
    }

    fn terminate_and_reap(&mut self) -> Result<ExitStatus, SshRunError> {
        if let Some(status) = self.status {
            return Ok(status);
        }
        let _ = self.child.kill();
        let status = self.child.wait().map_err(|error| {
            SshRunError::Wait(format!("reaping SSH operation after termination: {error}"))
        })?;
        self.status = Some(status);
        Ok(status)
    }
}

impl Drop for ManagedChild {
    fn drop(&mut self) {
        if self.status.is_none() {
            let _ = self.child.kill();
            if let Ok(status) = self.child.wait() {
                self.status = Some(status);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};
    use std::sync::OnceLock;

    const TEST_LIMIT: usize = 256 * 1024;

    #[test]
    fn drains_stdout_and_stderr_while_the_process_runs() {
        let output = run_fake(
            &["dual", "131072", "131072"],
            b"",
            Duration::from_secs(5),
            TEST_LIMIT,
            &CancellationToken::new(),
        )
        .unwrap();

        assert_eq!(output.stdout.len(), 131_072);
        assert_eq!(output.stderr.len(), 131_072);
    }

    #[test]
    fn output_at_the_limit_succeeds() {
        let output = run_fake(
            &["stdout", &TEST_LIMIT.to_string()],
            b"",
            Duration::from_secs(5),
            TEST_LIMIT,
            &CancellationToken::new(),
        )
        .unwrap();

        assert_eq!(output.stdout.len(), TEST_LIMIT);
    }

    #[test]
    fn output_above_the_limit_terminates_with_a_typed_error() {
        let (result, pid) = run_fake_recording_pid(
            &["stdout", &(TEST_LIMIT + 1).to_string()],
            b"",
            Duration::from_secs(5),
            TEST_LIMIT,
            &CancellationToken::new(),
            "output-limit",
        );
        let error = result.unwrap_err();

        assert_eq!(
            error,
            SshRunError::OutputLimitExceeded {
                stream: OutputStream::Stdout,
                limit: TEST_LIMIT,
            }
        );
        assert_process_reaped(pid);
    }

    #[test]
    fn stderr_above_the_limit_terminates_with_a_typed_error() {
        let (result, pid) = run_fake_recording_pid(
            &["stderr", &(TEST_LIMIT + 1).to_string()],
            b"",
            Duration::from_secs(5),
            TEST_LIMIT,
            &CancellationToken::new(),
            "stderr-output-limit",
        );
        let error = result.unwrap_err();

        assert_eq!(
            error,
            SshRunError::OutputLimitExceeded {
                stream: OutputStream::Stderr,
                limit: TEST_LIMIT,
            }
        );
        assert_process_reaped(pid);
    }

    #[test]
    fn large_stderr_does_not_block_small_stdout() {
        let output = run_fake(
            &["dual", "2", "131072"],
            b"",
            Duration::from_secs(5),
            TEST_LIMIT,
            &CancellationToken::new(),
        )
        .unwrap();

        assert_eq!(output.stdout, b"xx");
        assert_eq!(output.stderr.len(), 131_072);
    }

    #[test]
    fn cancellation_during_stdin_write_terminates_and_reaps() {
        let _ = fake_ssh_path();
        let cancellation = CancellationToken::new();
        let trigger = cancellation.clone();
        let marker = pid_marker("cancel-stdin");
        let _ = std::fs::remove_file(&marker);
        let cancel_worker = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(2);
            while !marker.is_file() && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(5));
            }
            trigger.cancel();
        });

        let (result, pid) = run_fake_recording_pid(
            &["sleep", "5000"],
            &vec![b'x'; 4 * 1024 * 1024],
            Duration::from_secs(5),
            TEST_LIMIT,
            &cancellation,
            "cancel-stdin",
        );
        let error = result.unwrap_err();
        cancel_worker.join().unwrap();

        assert_eq!(error, SshRunError::Cancelled);
        assert_process_reaped(pid);
    }

    #[test]
    fn cancellation_during_output_terminates_and_reaps() {
        let _ = fake_ssh_path();
        let cancellation = CancellationToken::new();
        let trigger = cancellation.clone();
        let cancel_worker = thread::spawn(move || {
            thread::sleep(Duration::from_millis(75));
            trigger.cancel();
        });

        let (result, pid) = run_fake_recording_pid(
            &["stream", "4096", "100", "20"],
            b"",
            Duration::from_secs(5),
            TEST_LIMIT * 4,
            &cancellation,
            "cancel-output",
        );
        let error = result.unwrap_err();
        cancel_worker.join().unwrap();

        assert_eq!(error, SshRunError::Cancelled);
        assert_process_reaped(pid);
    }

    #[test]
    fn timeout_terminates_and_reaps() {
        let started = Instant::now();
        let (result, pid) = run_fake_recording_pid(
            &["sleep", "5000"],
            b"",
            Duration::from_millis(100),
            TEST_LIMIT,
            &CancellationToken::new(),
            "timeout",
        );
        let error = result.unwrap_err();

        assert!(matches!(error, SshRunError::Timeout { .. }));
        assert!(started.elapsed() < Duration::from_secs(2));
        assert_process_reaped(pid);
    }

    #[test]
    fn cancellation_before_spawn_does_not_start_the_command() {
        let cancellation = CancellationToken::new();
        cancellation.cancel();
        let marker = test_support_directory().join("not-started.marker");
        let _ = std::fs::remove_file(&marker);

        let error = run_fake(
            &["mark", marker.to_string_lossy().as_ref()],
            b"",
            Duration::from_secs(1),
            TEST_LIMIT,
            &cancellation,
        )
        .unwrap_err();

        assert_eq!(error, SshRunError::Cancelled);
        assert!(!marker.exists());
    }

    #[test]
    fn cancellation_after_process_exit_preserves_the_completed_result() {
        let _ = fake_ssh_path();
        let cancellation = CancellationToken::new();
        let trigger = cancellation.clone();
        let cancel_worker = thread::spawn(move || {
            thread::sleep(Duration::from_millis(150));
            trigger.cancel();
        });

        let (result, pid) = run_fake_recording_pid(
            &["sleep", "25"],
            b"",
            Duration::from_secs(2),
            TEST_LIMIT,
            &cancellation,
            "cancel-after-exit",
        );
        let output = result.unwrap();
        cancel_worker.join().unwrap();

        assert!(output.status.success());
        assert!(cancellation.is_cancelled());
        assert_process_reaped(pid);
    }

    #[test]
    fn parse_failure_happens_only_after_the_process_is_reaped() {
        let (result, pid) = run_fake_recording_pid(
            &["invalid-utf8", "8"],
            b"",
            Duration::from_secs(2),
            TEST_LIMIT,
            &CancellationToken::new(),
            "parse-failure",
        );
        let output = result.unwrap();

        assert!(String::from_utf8(output.stdout).is_err());
        assert_process_reaped(pid);
    }

    #[test]
    fn stdin_failure_returns_only_after_the_process_is_reaped() {
        let (result, pid) = run_fake_recording_pid(
            &["exit", "0"],
            &vec![b'x'; 4 * 1024 * 1024],
            Duration::from_secs(2),
            TEST_LIMIT,
            &CancellationToken::new(),
            "stdin-failure",
        );
        let error = result.unwrap_err();

        assert!(matches!(error, SshRunError::Input(_)));
        assert_process_reaped(pid);
    }

    #[test]
    #[ignore = "manual release-mode measurement"]
    fn measure_ssh_stream_and_cancellation_baseline() {
        let stream_started = Instant::now();
        let output = run_fake(
            &["dual", "1048576", "1048576"],
            b"",
            Duration::from_secs(5),
            2 * 1024 * 1024,
            &CancellationToken::new(),
        )
        .unwrap();
        let stream_elapsed = stream_started.elapsed();
        assert_eq!(output.stdout.len(), 1_048_576);
        assert_eq!(output.stderr.len(), 1_048_576);

        let cancellation = CancellationToken::new();
        let trigger = cancellation.clone();
        let (cancelled_tx, cancelled_rx) = std::sync::mpsc::channel();
        let cancel_worker = thread::spawn(move || {
            thread::sleep(Duration::from_millis(75));
            trigger.cancel();
            cancelled_tx.send(Instant::now()).unwrap();
        });
        let (result, pid) = run_fake_recording_pid(
            &["stream", "4096", "100", "20"],
            b"",
            Duration::from_secs(5),
            TEST_LIMIT * 4,
            &cancellation,
            "baseline-cancel",
        );
        let cancelled_at = cancelled_rx.recv().unwrap();
        let cleanup_elapsed = cancelled_at.elapsed();
        cancel_worker.join().unwrap();
        assert_eq!(result.unwrap_err(), SshRunError::Cancelled);
        assert_process_reaped(pid);

        println!(
            "M0_BASELINE ssh_stdout_bytes={} ssh_stderr_bytes={} ssh_stream_ms={:.3} ssh_cancel_cleanup_ms={:.3}",
            output.stdout.len(),
            output.stderr.len(),
            stream_elapsed.as_secs_f64() * 1_000.0,
            cleanup_elapsed.as_secs_f64() * 1_000.0,
        );
    }

    fn run_fake(
        args: &[&str],
        input: &[u8],
        timeout: Duration,
        output_limit: usize,
        cancellation: &CancellationToken,
    ) -> Result<SshOutput, SshRunError> {
        let mut command = Command::new(fake_ssh_path());
        command.args(args);
        SshRunner::new("fake-host", timeout, output_limit, cancellation).run(command, input)
    }

    fn run_fake_recording_pid(
        args: &[&str],
        input: &[u8],
        timeout: Duration,
        output_limit: usize,
        cancellation: &CancellationToken,
        label: &str,
    ) -> (Result<SshOutput, SshRunError>, u32) {
        let marker = pid_marker(label);
        let _ = std::fs::remove_file(&marker);
        let mut command = Command::new(fake_ssh_path());
        command.args(args);
        command.env("DEVHUB_FAKE_SSH_PID_FILE", &marker);
        let result =
            SshRunner::new("fake-host", timeout, output_limit, cancellation).run(command, input);
        let deadline = Instant::now() + Duration::from_secs(2);
        let pid = loop {
            if let Ok(contents) = std::fs::read_to_string(&marker) {
                if let Ok(pid) = contents.trim().parse() {
                    break pid;
                }
            }
            assert!(
                Instant::now() < deadline,
                "PID marker {} was not populated",
                marker.display()
            );
            std::thread::sleep(Duration::from_millis(10));
        };
        let _ = std::fs::remove_file(marker);
        (result, pid)
    }

    fn pid_marker(label: &str) -> PathBuf {
        test_support_directory().join(format!("{label}-{}.pid", std::process::id()))
    }

    fn assert_process_reaped(pid: u32) {
        assert!(
            !process_is_running(pid),
            "fake SSH process {pid} is still resident"
        );
    }

    #[cfg(windows)]
    fn process_is_running(pid: u32) -> bool {
        let output = Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"])
            .output()
            .unwrap();
        let expected = format!("\",\"{pid}\",");
        String::from_utf8_lossy(&output.stdout).contains(&expected)
    }

    #[cfg(unix)]
    fn process_is_running(pid: u32) -> bool {
        Command::new("sh")
            .args([
                "-c",
                "kill -0 \"$1\" 2>/dev/null",
                "devhub-test",
                &pid.to_string(),
            ])
            .status()
            .is_ok_and(|status| status.success())
    }

    fn fake_ssh_path() -> &'static Path {
        static FAKE_SSH: OnceLock<PathBuf> = OnceLock::new();
        FAKE_SSH
            .get_or_init(|| {
                let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
                let workspace = manifest.parent().and_then(Path::parent).unwrap();
                let support = workspace.join("target").join("test-support");
                std::fs::create_dir_all(&support).unwrap();
                let executable = support.join(format!(
                    "devhub-fake-ssh-{}{}",
                    std::process::id(),
                    std::env::consts::EXE_SUFFIX
                ));
                let source = manifest.join("tests").join("fixtures").join("fake_ssh.rs");
                let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
                let status = Command::new(rustc)
                    .arg("--edition=2021")
                    .arg("-O")
                    .arg(&source)
                    .arg("-o")
                    .arg(&executable)
                    .status()
                    .unwrap();
                assert!(status.success(), "failed to compile {}", source.display());
                executable
            })
            .as_path()
    }

    fn test_support_directory() -> PathBuf {
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        manifest
            .parent()
            .and_then(Path::parent)
            .unwrap()
            .join("target")
            .join("test-support")
    }
}
