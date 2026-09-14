use std::{
    collections::BTreeSet,
    fs::{File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

use anyhow::{Context as _, Result};
use oxideterm_settings::PersistedSettings;
use sysinfo::{ProcessesToUpdate, System, get_current_pid};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

const LOG_FILE_PREFIX: &str = "oxideterm-native";
const LOG_FILE_EXTENSION: &str = "log";
const MAX_LOG_FILE_BYTES: u64 = 1024 * 1024;
// Keep detailed diagnostics inside OxideTerm-owned targets. Enabling every
// dependency at debug level produced tens of thousands of SSH packet-state
// records and could itself increase CPU and disk pressure during diagnosis.
const DEBUG_LOG_FILTER: &str = "warn,oxideterm::audit=debug,oxideterm_gpui_app=debug,oxideterm_cloud_sync=debug,oxideterm_connections=debug,oxideterm_connection_monitor=debug,oxideterm_gpui_terminal=debug,oxideterm_gpui_ui=info,oxideterm_ssh=warn,oxideterm_sftp=debug,webrtc_ice=error,gpui=info,gpui_windows=info,winit=warn";
const CPU_SAMPLE_INTERVAL: Duration = Duration::from_secs(1);
const CPU_SPIKE_THRESHOLD_PERCENT: f32 = 80.0;
static NEXT_AUDIT_TRACE_ID: AtomicU64 = AtomicU64::new(1);
static LOG_CONTROLLER: OnceLock<Arc<ChunkedLogWriter>> = OnceLock::new();

struct ChunkedLogState {
    file: Option<File>,
    current_len: u64,
    next_index: u64,
}

impl ChunkedLogState {
    fn open(directory: &Path) -> io::Result<Self> {
        let mut latest_index: Option<u64> = None;
        for entry in std::fs::read_dir(directory)? {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                continue;
            }
            let Some(index) = chunk_index(&entry.file_name()) else {
                continue;
            };
            latest_index = Some(latest_index.map_or(index, |latest| latest.max(index)));
        }

        let mut state = Self {
            file: None,
            current_len: 0,
            next_index: latest_index.unwrap_or(0).saturating_add(1),
        };
        if let Some(index) = latest_index {
            let path = chunk_path(directory, index);
            let length = std::fs::metadata(&path)?.len();
            state.next_index = index;
            if length < MAX_LOG_FILE_BYTES && file_starts_with_utf8_bom(&path)? {
                state.file = Some(open_chunk(&path)?);
                state.current_len = length;
                state.next_index = index.saturating_add(1);
            } else {
                state.next_index = index.saturating_add(1);
            }
        }
        Ok(state)
    }

    fn ensure_file(&mut self, directory: &Path) -> io::Result<()> {
        if self.file.is_some() && self.current_len < MAX_LOG_FILE_BYTES {
            return Ok(());
        }
        self.file = None;
        self.current_len = 0;
        let index = self.next_index.max(1);
        self.next_index = index.saturating_add(1);
        let file = open_chunk(&chunk_path(directory, index))?;
        self.current_len = file.metadata()?.len();
        self.file = Some(file);
        Ok(())
    }

    fn write_bytes(&mut self, directory: &Path, mut bytes: &[u8]) -> io::Result<()> {
        while !bytes.is_empty() {
            self.ensure_file(directory)?;
            let remaining = (MAX_LOG_FILE_BYTES - self.current_len) as usize;
            let mut take = bytes.len().min(remaining.max(1));
            if take < bytes.len()
                && let Ok(text) = std::str::from_utf8(bytes)
            {
                while take > 0 && !text.is_char_boundary(take) {
                    take -= 1;
                }
                if take == 0 {
                    // The current chunk cannot fit one complete UTF-8 scalar.
                    // Rotate before writing so neither chunk contains broken text.
                    self.file = None;
                    self.current_len = MAX_LOG_FILE_BYTES;
                    continue;
                }
            }
            let chunk = &bytes[..take];
            self.file
                .as_mut()
                .expect("ensure_file creates a chunk")
                .write_all(chunk)?;
            self.current_len = self.current_len.saturating_add(take as u64);
            bytes = &bytes[take..];
        }
        Ok(())
    }

    fn flush(&mut self) -> io::Result<()> {
        if let Some(file) = self.file.as_mut() {
            file.flush()?;
        }
        Ok(())
    }
}

/// A tracing writer that creates files only while debug logging is enabled.
/// The mutex also serializes rotation so every chunk stays within the size cap.
struct ChunkedLogWriter {
    directory: PathBuf,
    enabled: AtomicBool,
    state: Mutex<ChunkedLogState>,
    cpu_sampler: Mutex<Option<CpuSamplerWorker>>,
}

struct ChunkedLogSink {
    writer: Arc<ChunkedLogWriter>,
}

struct CpuSamplerWorker {
    shutdown_tx: mpsc::Sender<()>,
    worker: thread::JoinHandle<()>,
}

impl Write for ChunkedLogSink {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.writer.write_enabled(bytes)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.writer.flush_enabled()
    }
}

impl ChunkedLogWriter {
    fn new(directory: PathBuf, enabled: bool) -> io::Result<Self> {
        let state = if enabled {
            std::fs::create_dir_all(&directory)?;
            ChunkedLogState::open(&directory)?
        } else {
            ChunkedLogState {
                file: None,
                current_len: 0,
                next_index: 1,
            }
        };
        Ok(Self {
            directory,
            enabled: AtomicBool::new(enabled),
            state: Mutex::new(state),
            cpu_sampler: Mutex::new(None),
        })
    }

    fn set_enabled(self: &Arc<Self>, enabled: bool) -> io::Result<()> {
        if self.enabled.load(Ordering::Acquire) == enabled {
            return Ok(());
        }
        if !enabled {
            self.enabled.store(false, Ordering::Release);
            let mut state = self
                .state
                .lock()
                .map_err(|_| io::Error::other("log writer lock poisoned"))?;
            state.file = None;
            state.current_len = 0;
            drop(state);
            self.stop_cpu_sampler();
            return Ok(());
        }

        let mut state = self
            .state
            .lock()
            .map_err(|_| io::Error::other("log writer lock poisoned"))?;
        std::fs::create_dir_all(&self.directory)?;
        state.file = None;
        state.current_len = 0;
        state.next_index = ChunkedLogState::open(&self.directory)?.next_index;
        self.enabled.store(true, Ordering::Release);
        drop(state);
        self.start_cpu_sampler()?;
        Ok(())
    }

    fn start_cpu_sampler(self: &Arc<Self>) -> io::Result<()> {
        let mut sampler = self
            .cpu_sampler
            .lock()
            .map_err(|_| io::Error::other("CPU sampler lock poisoned"))?;
        if sampler.is_some() {
            return Ok(());
        }
        let (shutdown_tx, shutdown_rx) = mpsc::channel();
        let writer = Arc::clone(self);
        let worker = thread::Builder::new()
            .name("oxideterm-cpu-audit".to_string())
            .spawn(move || run_cpu_sampler(writer, shutdown_rx))?;
        *sampler = Some(CpuSamplerWorker {
            shutdown_tx,
            worker,
        });
        Ok(())
    }

    fn stop_cpu_sampler(&self) {
        let worker = self
            .cpu_sampler
            .lock()
            .ok()
            .and_then(|mut sampler| sampler.take());
        if let Some(worker) = worker {
            let _ = worker.shutdown_tx.send(());
            let _ = worker.worker.join();
        }
    }
}

impl Write for ChunkedLogWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.write_enabled(bytes)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.flush_enabled()
    }
}

impl ChunkedLogWriter {
    fn write_enabled(&self, bytes: &[u8]) -> io::Result<usize> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| io::Error::other("log writer lock poisoned"))?;
        if !self.enabled.load(Ordering::Acquire) {
            return Ok(bytes.len());
        }
        state.write_bytes(&self.directory, bytes)?;
        Ok(bytes.len())
    }

    fn flush_enabled(&self) -> io::Result<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| io::Error::other("log writer lock poisoned"))?;
        if self.enabled.load(Ordering::Acquire) {
            state.flush()?;
        }
        Ok(())
    }

    fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Acquire)
    }
}

fn open_chunk(path: &Path) -> io::Result<File> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .read(true)
        .open(path)?;
    if file.metadata()?.len() == 0 {
        // Windows editors can otherwise interpret Chinese UTF-8 logs using the
        // system code page. A BOM is written only at the beginning of new chunks.
        file.write_all(b"\xEF\xBB\xBF")?;
        file.flush()?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(file)
}

fn file_starts_with_utf8_bom(path: &Path) -> io::Result<bool> {
    use std::io::Read;

    let mut file = File::open(path)?;
    let mut bom = [0_u8; 3];
    Ok(file.read(&mut bom)? == bom.len() && bom == *b"\xEF\xBB\xBF")
}

fn chunk_path(directory: &Path, index: u64) -> PathBuf {
    directory.join(format!("{LOG_FILE_PREFIX}-{index:06}.{LOG_FILE_EXTENSION}"))
}

fn chunk_index(file_name: &std::ffi::OsStr) -> Option<u64> {
    let file_name = file_name.to_str()?;
    let prefix = format!("{LOG_FILE_PREFIX}-");
    let index = file_name.strip_prefix(&prefix)?.strip_suffix(".log")?;
    (!index.is_empty() && index.chars().all(|character| character.is_ascii_digit()))
        .then(|| index.parse().ok())?
}

pub(crate) fn log_directory() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf))
        .map(|parent| parent.join("log"))
        .unwrap_or_else(|| PathBuf::from("log"))
}

pub(crate) fn init_file_logging(
    settings: &PersistedSettings,
    _settings_path: Option<&Path>,
) -> Result<Option<WorkerGuard>> {
    let log_dir = log_directory();
    let writer = Arc::new(
        ChunkedLogWriter::new(log_dir.clone(), settings.diagnostics.debug_logging)
            .with_context(|| format!("failed to prepare log directory at {}", log_dir.display()))?,
    );
    let (non_blocking_writer, guard) = tracing_appender::non_blocking(ChunkedLogSink {
        writer: writer.clone(),
    });
    let subscriber = tracing_subscriber::registry()
        .with(
            fmt::layer()
                .with_writer(non_blocking_writer)
                .with_ansi(false)
                .with_target(true),
        )
        .with(EnvFilter::new(DEBUG_LOG_FILTER));

    // Tests or embedding hosts may already have installed a global subscriber.
    // In that case OxideTerm should keep running and simply skip its file sink.
    if subscriber.try_init().is_err() {
        return Ok(None);
    }
    let _ = LOG_CONTROLLER.set(writer);

    if settings.diagnostics.debug_logging {
        LOG_CONTROLLER
            .get()
            .expect("logging controller was installed")
            .start_cpu_sampler()
            .context("failed to start the debug CPU sampler")?;
        tracing::info!(
            log_directory = %log_dir.display(),
            max_log_file_bytes = MAX_LOG_FILE_BYTES,
            cpu_spike_threshold_percent = CPU_SPIKE_THRESHOLD_PERCENT,
            cpu_sample_interval_secs = CPU_SAMPLE_INTERVAL.as_secs(),
            debug_logging = true,
            "detailed OxideTerm audit logging initialized"
        );
    }
    Ok(Some(guard))
}

pub(crate) fn set_debug_logging(enabled: bool) -> io::Result<()> {
    let Some(writer) = LOG_CONTROLLER.get() else {
        return Ok(());
    };
    writer.set_enabled(enabled)
}

/// Whether the detailed audit sink is currently writing to disk. Callers use
/// this to skip building expensive audit payloads while logging is disabled.
pub(crate) fn debug_logging_enabled() -> bool {
    LOG_CONTROLLER
        .get()
        .is_some_and(|writer| writer.is_enabled())
}

pub(crate) fn next_audit_trace_id() -> u64 {
    NEXT_AUDIT_TRACE_ID.fetch_add(1, Ordering::Relaxed)
}

pub(crate) fn audit_button_click(control: &str, module: &str) {
    tracing::debug!(
        target: "oxideterm::audit",
        trace_id = next_audit_trace_id(),
        stage = "ui.button",
        control,
        module,
        result = "accepted",
        "user clicked a button"
    );
}

pub(crate) fn audit_button_blocked(control: &str, module: &str, reason: &str) {
    tracing::debug!(
        target: "oxideterm::audit",
        trace_id = next_audit_trace_id(),
        stage = "ui.button",
        control,
        module,
        result = "blocked",
        reason,
        business_impact = "the requested action did not enter its feature module",
        "user interaction was blocked"
    );
}

pub(crate) fn audit_settings_change(
    settings_path: &Path,
    previous: &PersistedSettings,
    next: &PersistedSettings,
    persisted: bool,
) {
    // Serializing both settings trees is only worth paying for when the debug
    // sink is enabled; slider drags would otherwise serialize on every step.
    if !debug_logging_enabled() {
        return;
    }
    let mut changes = Vec::new();
    collect_setting_changes(
        "settings",
        &serde_json::to_value(previous),
        &serde_json::to_value(next),
        &mut changes,
    );
    if changes.is_empty() {
        return;
    }
    tracing::debug!(
        target: "oxideterm::audit",
        trace_id = next_audit_trace_id(),
        stage = "settings.persistence",
        settings_file = %settings_path.display(),
        persisted,
        changed_field_count = changes.len(),
        changed_fields = %changes.join(" | "),
        "settings changed"
    );
}

fn collect_setting_changes(
    path: &str,
    previous: &serde_json::Result<serde_json::Value>,
    next: &serde_json::Result<serde_json::Value>,
    changes: &mut Vec<String>,
) {
    let (Ok(previous), Ok(next)) = (previous, next) else {
        return;
    };
    collect_setting_changes_from_values(path, previous, next, changes);
}

fn collect_setting_changes_from_values(
    path: &str,
    previous: &serde_json::Value,
    next: &serde_json::Value,
    changes: &mut Vec<String>,
) {
    if previous == next {
        return;
    }
    if let (Some(previous), Some(next)) = (previous.as_object(), next.as_object()) {
        let keys = previous
            .keys()
            .chain(next.keys())
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
        for key in keys {
            let child_path = format!("{path}.{key}");
            match (previous.get(key), next.get(key)) {
                (Some(previous), Some(next)) => {
                    collect_setting_changes_from_values(&child_path, previous, next, changes)
                }
                (Some(previous), None) => changes.push(format!(
                    "{child_path}: {} -> <removed>",
                    display_setting_value(&child_path, previous)
                )),
                (None, Some(next)) => changes.push(format!(
                    "{child_path}: <missing> -> {}",
                    display_setting_value(&child_path, next)
                )),
                (None, None) => {}
            }
        }
        return;
    }
    changes.push(format!(
        "{path}: {} -> {}",
        display_setting_value(path, previous),
        display_setting_value(path, next)
    ));
}

fn display_setting_value(path: &str, value: &serde_json::Value) -> String {
    let lower_path = path.to_ascii_lowercase();
    if [
        "password",
        "passphrase",
        "token",
        "secret",
        "private",
        "credential",
        "keychain",
        "customenv",
        "environment",
        ".auth",
    ]
    .iter()
    .any(|needle| lower_path.contains(needle))
    {
        return "<redacted>".to_string();
    }
    let mut rendered = value.to_string();
    const MAX_VALUE_CHARS: usize = 240;
    if rendered.chars().count() > MAX_VALUE_CHARS {
        rendered = rendered.chars().take(MAX_VALUE_CHARS).collect::<String>();
        rendered.push_str("...");
    }
    rendered
}

fn run_cpu_sampler(writer: Arc<ChunkedLogWriter>, shutdown_rx: mpsc::Receiver<()>) {
    let Ok(pid) = get_current_pid() else {
        tracing::warn!(
            target: "oxideterm::audit",
            stage = "performance.cpu",
            result = "unavailable",
            reason = "the current process identifier could not be resolved",
            business_impact = "CPU spike diagnostics are unavailable for this run",
            "debug CPU sampler could not start"
        );
        return;
    };

    let mut system = System::new_all();
    let logical_processor_count = system.cpus().len().max(1) as f32;
    let mut active_spike_trace_id = None;
    let mut process_unavailable_reported = false;
    loop {
        match shutdown_rx.recv_timeout(CPU_SAMPLE_INTERVAL) {
            Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => return,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        if !writer.enabled.load(Ordering::Acquire) {
            return;
        }

        system.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
        let Some(process) = system.process(pid) else {
            if !process_unavailable_reported {
                tracing::warn!(
                    target: "oxideterm::audit",
                    stage = "performance.cpu",
                    result = "unavailable",
                    reason = "the current process was absent from the operating-system sample",
                    business_impact = "CPU spike diagnostics paused until the process is observable again",
                    "debug CPU sampler could not read OxideTerm process metrics"
                );
                process_unavailable_reported = true;
            }
            continue;
        };
        if process_unavailable_reported {
            tracing::info!(
                target: "oxideterm::audit",
                stage = "performance.cpu",
                result = "available",
                "debug CPU sampler resumed process metric collection"
            );
            process_unavailable_reported = false;
        }

        let cpu_percent =
            normalized_process_cpu_percent(process.cpu_usage(), logical_processor_count);
        let memory_bytes = process.memory();
        if cpu_percent >= CPU_SPIKE_THRESHOLD_PERCENT {
            if active_spike_trace_id.is_none() {
                let trace_id = next_audit_trace_id();
                active_spike_trace_id = Some(trace_id);
                tracing::warn!(
                    target: "oxideterm::audit",
                    trace_id,
                    stage = "performance.cpu",
                    result = "spike_detected",
                    process_cpu_percent = cpu_percent,
                    process_memory_bytes = memory_bytes,
                    logical_processor_count,
                    threshold_percent = CPU_SPIKE_THRESHOLD_PERCENT,
                    sample_interval_secs = CPU_SAMPLE_INTERVAL.as_secs(),
                    business_impact = "OxideTerm may be causing visible interface slowdown while this spike persists",
                    "OxideTerm process CPU crossed the diagnostic threshold"
                );
            }
            continue;
        }

        if let Some(trace_id) = active_spike_trace_id.take() {
            tracing::info!(
                target: "oxideterm::audit",
                trace_id,
                stage = "performance.cpu",
                result = "recovered",
                process_cpu_percent = cpu_percent,
                process_memory_bytes = memory_bytes,
                logical_processor_count,
                threshold_percent = CPU_SPIKE_THRESHOLD_PERCENT,
                business_impact = "OxideTerm process CPU returned below the diagnostic threshold",
                "OxideTerm process CPU spike recovered"
            );
        }
    }
}

fn normalized_process_cpu_percent(raw_cpu_percent: f32, logical_processor_count: f32) -> f32 {
    raw_cpu_percent / logical_processor_count.max(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new(test_name: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "oxideterm-logging-{test_name}-{}",
                uuid::Uuid::new_v4()
            ));
            std::fs::create_dir_all(&path).expect("create test directory");
            Self(path)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn chunked_writer_rotates_at_one_megabyte() {
        let directory = TestDirectory::new("chunked-file");
        let writer = ChunkedLogWriter::new(directory.0.clone(), true).expect("open log writer");
        let mut writer = writer;
        let first = vec![b'a'; MAX_LOG_FILE_BYTES as usize];
        writer.write_all(&first).expect("write first chunk");
        writer
            .write_all(b"newest-entry\n")
            .expect("write second chunk");
        writer.flush().expect("flush chunks");

        let mut chunks = std::fs::read_dir(&directory.0)
            .expect("read chunk directory")
            .map(|entry| entry.expect("read chunk entry").path())
            .collect::<Vec<_>>();
        chunks.sort();
        assert_eq!(chunks.len(), 2);
        assert!(chunks.iter().all(|path| {
            std::fs::metadata(path).expect("read chunk metadata").len() <= MAX_LOG_FILE_BYTES
        }));
        assert!(
            std::fs::read_to_string(&chunks[1])
                .expect("read newest chunk")
                .contains("newest-entry")
        );
    }

    #[test]
    fn new_log_chunk_is_bom_marked_utf8_and_preserves_chinese_text() {
        let directory = TestDirectory::new("utf8-bom");
        let writer = ChunkedLogWriter::new(directory.0.clone(), true).unwrap();
        writer.write_enabled("中文调试日志\n".as_bytes()).unwrap();
        writer.flush_enabled().unwrap();

        let path = std::fs::read_dir(&directory.0)
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let bytes = std::fs::read(path).unwrap();
        assert!(bytes.starts_with(b"\xEF\xBB\xBF"));
        assert_eq!(
            std::str::from_utf8(&bytes[3..]).unwrap(),
            "中文调试日志\n"
        );
    }

    #[test]
    fn legacy_log_without_bom_is_not_appended() {
        let directory = TestDirectory::new("legacy-without-bom");
        let legacy = chunk_path(&directory.0, 1);
        std::fs::write(&legacy, b"legacy log\n").unwrap();
        let writer = ChunkedLogWriter::new(directory.0.clone(), true).unwrap();
        writer.write_enabled("新中文日志\n".as_bytes()).unwrap();
        writer.flush_enabled().unwrap();

        assert_eq!(std::fs::read(&legacy).unwrap(), b"legacy log\n");
        let new_chunk = chunk_path(&directory.0, 2);
        let bytes = std::fs::read(new_chunk).unwrap();
        assert!(bytes.starts_with(b"\xEF\xBB\xBF"));
        assert_eq!(std::str::from_utf8(&bytes[3..]).unwrap(), "新中文日志\n");
    }

    #[test]
    fn log_rotation_never_splits_a_chinese_utf8_character() {
        let directory = TestDirectory::new("utf8-rotation");
        let writer = ChunkedLogWriter::new(directory.0.clone(), true).unwrap();
        writer
            .write_enabled(&vec![b'a'; MAX_LOG_FILE_BYTES as usize - 4])
            .unwrap();
        writer.write_enabled("中\n".as_bytes()).unwrap();
        writer.flush_enabled().unwrap();

        let mut paths = std::fs::read_dir(&directory.0)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect::<Vec<_>>();
        paths.sort();
        assert_eq!(paths.len(), 2);
        for path in paths {
            let bytes = std::fs::read(path).unwrap();
            assert!(bytes.starts_with(b"\xEF\xBB\xBF"));
            assert!(std::str::from_utf8(&bytes[3..]).is_ok());
        }
    }

    #[test]
    fn disabled_writer_does_not_create_or_write_a_log_file() {
        let directory = TestDirectory::new("disabled");
        let writer =
            ChunkedLogWriter::new(directory.0.clone(), false).expect("open disabled writer");
        let mut writer = writer;
        writer
            .write_all(b"must not be persisted")
            .expect("discard disabled log");
        writer.flush().expect("flush disabled log");
        assert_eq!(std::fs::read_dir(&directory.0).unwrap().count(), 0);
    }

    #[test]
    fn chunk_index_accepts_only_native_chunk_names() {
        assert_eq!(
            chunk_index(std::ffi::OsStr::new("oxideterm-native-000012.log")),
            Some(12)
        );
        assert_eq!(
            chunk_index(std::ffi::OsStr::new("oxideterm-native.backup")),
            None
        );
        assert_eq!(chunk_index(std::ffi::OsStr::new("other-000012.log")), None);
    }

    #[test]
    fn setting_values_redact_sensitive_paths() {
        assert_eq!(
            display_setting_value("snapshot.connection.password", &serde_json::json!("secret")),
            "<redacted>"
        );
        assert_eq!(
            display_setting_value("snapshot.terminal.fontSize", &serde_json::json!(14)),
            "14"
        );
    }

    #[test]
    fn audit_trace_ids_are_monotonic() {
        let first = next_audit_trace_id();
        let second = next_audit_trace_id();
        assert!(second > first);
    }

    #[test]
    fn session_debug_audit_reaches_the_log_file_with_application_filter() {
        let directory = TestDirectory::new("session-audit-filter");
        let writer = Arc::new(ChunkedLogWriter::new(directory.0.clone(), true).unwrap());
        let (sink, guard) = tracing_appender::non_blocking(ChunkedLogSink { writer });
        let subscriber = tracing_subscriber::registry()
            .with(fmt::layer().with_writer(sink).with_ansi(false))
            .with(EnvFilter::new(DEBUG_LOG_FILTER));
        tracing::subscriber::with_default(subscriber, || {
            tracing::debug!(target: "oxideterm::audit", trace_id = 42,
                stage = "session.form.control_change", "audit-file-regression-marker");
        });
        drop(guard);
        let logs = std::fs::read_dir(&directory.0).unwrap()
            .map(|entry| std::fs::read_to_string(entry.unwrap().path()).unwrap())
            .collect::<String>();
        assert!(logs.contains("audit-file-regression-marker"));
        assert!(logs.contains("trace_id=42"));
    }

    #[test]
    fn process_cpu_is_normalized_to_total_machine_capacity() {
        assert_eq!(normalized_process_cpu_percent(320.0, 4.0), 80.0);
        assert_eq!(normalized_process_cpu_percent(80.0, 0.0), 80.0);
    }
}
