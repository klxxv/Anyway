//! Framed stdio transport and process supervisor for isolated Python workers.
//!
//! The transport is deliberately below the Tauri/kernel-native boundary:
//! workers receive a Host-created session envelope, never a principal, and
//! cannot call `kernel_native::kernel_bus_call` directly. The Host owns the
//! process, operation allowlist, deadlines, cancellation and credentials.

use base64::Engine;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    io::{self, Read, Write},
    path::PathBuf,
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    thread,
    time::{Duration, Instant},
};

pub const WORKER_RPC_API_VERSION: &str = "researchcanvas.dev/worker-rpc/v1";
pub const WORKER_TRANSPORT_ID: &str = "stdio-framed-json-v1";
pub const MAX_FRAME_BYTES: usize = 1024 * 1024;
pub const MAX_INLINE_BYTES: usize = 64 * 1024;
pub const MAX_BLOB_READ_CHUNK_BYTES: usize = 256 * 1024;
pub const MAX_BLOB_READ_RESULT_BYTES: usize = 384 * 1024;
const MAX_BLOB_READ_BASE64_BYTES: usize = ((MAX_BLOB_READ_CHUNK_BYTES + 2) / 3) * 4;
pub const MAX_ERROR_MESSAGE_BYTES: usize = 8 * 1024;
pub const MAX_EVENTS_PER_REQUEST: usize = 64;
pub const MAX_HOST_CALLS_PER_REQUEST: usize = 128;

#[derive(Debug)]
pub enum WorkerError {
    Io(String),
    UnexpectedEof,
    FrameTooLarge { size: usize, limit: usize },
    InvalidUtf8,
    InvalidJson(String),
    Protocol(String),
    OperationNotAllowed(String),
    InlinePayloadTooLarge { size: usize, limit: usize },
    Timeout(String),
    Cancelled(String),
    ProcessExited(Option<i32>),
    Remote { code: String, message: String },
}

impl std::fmt::Display for WorkerError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(message) => write!(formatter, "worker io: {message}"),
            Self::UnexpectedEof => write!(formatter, "worker stream ended unexpectedly"),
            Self::FrameTooLarge { size, limit } => {
                write!(formatter, "worker frame is {size} bytes; limit is {limit}")
            }
            Self::InvalidUtf8 => write!(formatter, "worker frame is not valid UTF-8"),
            Self::InvalidJson(message) => {
                write!(formatter, "worker frame has invalid JSON: {message}")
            }
            Self::Protocol(message) => write!(formatter, "worker protocol: {message}"),
            Self::OperationNotAllowed(operation) => {
                write!(formatter, "worker operation is not allowed: {operation}")
            }
            Self::InlinePayloadTooLarge { size, limit } => {
                write!(
                    formatter,
                    "inline worker payload is {size} bytes; limit is {limit}; use BlobRef"
                )
            }
            Self::Timeout(message) => write!(formatter, "worker timeout: {message}"),
            Self::Cancelled(message) => write!(formatter, "worker cancelled: {message}"),
            Self::ProcessExited(code) => write!(formatter, "worker process exited: {code:?}"),
            Self::Remote { code, message } => {
                write!(formatter, "worker remote error {code}: {message}")
            }
        }
    }
}

impl std::error::Error for WorkerError {}

impl From<io::Error> for WorkerError {
    fn from(error: io::Error) -> Self {
        Self::Io(error.to_string())
    }
}

/// Encode one JSON object as a 4-byte big-endian length-prefixed frame.
pub fn encode_frame(value: &Value) -> Result<Vec<u8>, WorkerError> {
    let payload =
        serde_json::to_vec(value).map_err(|error| WorkerError::InvalidJson(error.to_string()))?;
    if payload.len() > MAX_FRAME_BYTES {
        return Err(WorkerError::FrameTooLarge {
            size: payload.len(),
            limit: MAX_FRAME_BYTES,
        });
    }
    let length = u32::try_from(payload.len()).map_err(|_| WorkerError::FrameTooLarge {
        size: payload.len(),
        limit: MAX_FRAME_BYTES,
    })?;
    let mut frame = Vec::with_capacity(4 + payload.len());
    frame.extend_from_slice(&length.to_be_bytes());
    frame.extend_from_slice(&payload);
    Ok(frame)
}

/// Decode one complete framed message. Partial input is rejected so callers
/// cannot accidentally treat a truncated JSON document as a valid message.
pub fn decode_frame(frame: &[u8]) -> Result<Value, WorkerError> {
    if frame.len() < 4 {
        return Err(WorkerError::UnexpectedEof);
    }
    let size = u32::from_be_bytes([frame[0], frame[1], frame[2], frame[3]]) as usize;
    if size > MAX_FRAME_BYTES {
        return Err(WorkerError::FrameTooLarge {
            size,
            limit: MAX_FRAME_BYTES,
        });
    }
    if frame.len() != size + 4 {
        return Err(WorkerError::Protocol(format!(
            "frame length header says {size}, received {}",
            frame.len().saturating_sub(4)
        )));
    }
    let text = std::str::from_utf8(&frame[4..]).map_err(|_| WorkerError::InvalidUtf8)?;
    serde_json::from_str(text).map_err(|error| WorkerError::InvalidJson(error.to_string()))
}

pub fn read_frame<R: Read>(reader: &mut R) -> Result<Value, WorkerError> {
    let mut header = [0_u8; 4];
    reader.read_exact(&mut header).map_err(|error| {
        if error.kind() == io::ErrorKind::UnexpectedEof {
            WorkerError::UnexpectedEof
        } else {
            WorkerError::Io(error.to_string())
        }
    })?;
    let size = u32::from_be_bytes(header) as usize;
    if size > MAX_FRAME_BYTES {
        return Err(WorkerError::FrameTooLarge {
            size,
            limit: MAX_FRAME_BYTES,
        });
    }
    let mut payload = vec![0_u8; size];
    reader.read_exact(&mut payload).map_err(|error| {
        if error.kind() == io::ErrorKind::UnexpectedEof {
            WorkerError::UnexpectedEof
        } else {
            WorkerError::Io(error.to_string())
        }
    })?;
    let mut frame = header.to_vec();
    frame.extend_from_slice(&payload);
    decode_frame(&frame)
}

pub fn write_frame<W: Write>(writer: &mut W, value: &Value) -> Result<(), WorkerError> {
    writer.write_all(&encode_frame(value)?)?;
    writer.flush()?;
    Ok(())
}

fn validate_no_principal(value: &Value) -> Result<(), WorkerError> {
    match value {
        Value::Object(map) => {
            if map.contains_key("principal") {
                return Err(WorkerError::Protocol(
                    "principal is Host-bound and forbidden in worker payloads".to_string(),
                ));
            }
            for child in map.values() {
                validate_no_principal(child)?;
            }
        }
        Value::Array(values) => {
            for child in values {
                validate_no_principal(child)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn validate_no_authority_escape(value: &Value) -> Result<(), WorkerError> {
    match value {
        Value::Object(map) => {
            if map.keys().any(|key| {
                matches!(
                    key.as_str(),
                    "principal" | "lease" | "leaseId" | "capabilityLeaseIds"
                )
            }) {
                return Err(WorkerError::Protocol(
                    "worker cannot provide principal or capability leases".to_string(),
                ));
            }
            for child in map.values() {
                validate_no_authority_escape(child)?;
            }
        }
        Value::Array(values) => {
            for child in values {
                validate_no_authority_escape(child)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn validate_inline_payload(payload: &Value) -> Result<(), WorkerError> {
    validate_no_authority_escape(payload)?;
    validate_blob_ref_envelopes(payload)?;
    let size = serde_json::to_vec(payload)
        .map_err(|error| WorkerError::InvalidJson(error.to_string()))?
        .len();
    let is_blob_ref = payload
        .as_object()
        .is_some_and(|object| object.len() == 1 && object.contains_key("blobRef"));
    if is_blob_ref {
        validate_blob_ref(payload.get("blobRef").expect("blobRef exists"))?;
        return Ok(());
    }
    if size <= MAX_INLINE_BYTES {
        return Ok(());
    }
    Err(WorkerError::InlinePayloadTooLarge {
        size,
        limit: MAX_INLINE_BYTES,
    })
}

fn validate_host_result(operation: &str, result: &Value) -> Result<(), WorkerError> {
    if operation != "blob.read" {
        return validate_inline_payload(result);
    }
    validate_no_authority_escape(result)?;
    validate_blob_ref_envelopes(result)?;
    let object = result
        .as_object()
        .ok_or_else(|| WorkerError::Protocol("blob.read result must be an object".to_string()))?;
    let content_base64 = object
        .get("contentBase64")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            WorkerError::Protocol("blob.read result must contain contentBase64".to_string())
        })?;
    let size = serde_json::to_vec(result)
        .map_err(|error| WorkerError::InvalidJson(error.to_string()))?
        .len();
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(content_base64)
        .map_err(|_| WorkerError::Protocol("blob.read contentBase64 is invalid".to_string()))?;
    if content_base64.len() > MAX_BLOB_READ_BASE64_BYTES
        || decoded.len() > MAX_BLOB_READ_CHUNK_BYTES
        || size > MAX_BLOB_READ_RESULT_BYTES
    {
        return Err(WorkerError::InlinePayloadTooLarge {
            size,
            limit: MAX_BLOB_READ_RESULT_BYTES,
        });
    }
    Ok(())
}

fn validate_blob_ref_envelopes(value: &Value) -> Result<(), WorkerError> {
    match value {
        Value::Object(object) => {
            if object.contains_key("blobRef") {
                validate_blob_ref(object.get("blobRef").expect("blobRef exists"))?;
            }
            for (key, child) in object {
                if key == "blobRef" {
                    continue;
                }
                validate_blob_ref_envelopes(child)?;
            }
        }
        Value::Array(values) => {
            for child in values {
                validate_blob_ref_envelopes(child)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn validate_blob_ref(value: &Value) -> Result<(), WorkerError> {
    let object = value
        .as_object()
        .ok_or_else(|| WorkerError::Protocol("BlobRef must be an object".to_string()))?;
    let expected = [
        "algorithm",
        "digest",
        "size",
        "mediaType",
        "scope",
        "owner",
        "retentionClass",
    ];
    if object.len() != expected.len() || expected.iter().any(|key| !object.contains_key(*key)) {
        return Err(WorkerError::Protocol(
            "BlobRef must contain exactly algorithm, digest, size, mediaType, scope, owner and retentionClass".to_string(),
        ));
    }
    let algorithm = object
        .get("algorithm")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let digest = object
        .get("digest")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let media_type = object
        .get("mediaType")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let scope = object
        .get("scope")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let owner = object
        .get("owner")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let retention = object
        .get("retentionClass")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let size = object
        .get("size")
        .and_then(Value::as_u64)
        .unwrap_or(u64::MAX);
    if algorithm != "sha256" {
        return Err(WorkerError::Protocol(
            "BlobRef.algorithm is invalid".to_string(),
        ));
    }
    if size > (1_u64 << 40) {
        return Err(WorkerError::Protocol(
            "BlobRef.size is outside the bounded range".to_string(),
        ));
    }
    if digest.len() != 64
        || !digest
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(WorkerError::Protocol(
            "BlobRef.digest is invalid".to_string(),
        ));
    }
    if !valid_blob_scope(scope, owner) {
        return Err(WorkerError::Protocol(
            "BlobRef.scope is invalid".to_string(),
        ));
    }
    for (name, value) in [("mediaType", media_type), ("owner", owner)] {
        if value.is_empty() || value.len() > 256 || value.bytes().any(|byte| byte < 0x20) {
            return Err(WorkerError::Protocol(format!("BlobRef.{name} is invalid")));
        }
    }
    if !matches!(retention, "request" | "session" | "plugin" | "persistent") {
        return Err(WorkerError::Protocol(
            "BlobRef.retentionClass is invalid".to_string(),
        ));
    }
    Ok(())
}

fn valid_blob_scope(scope: &str, owner: &str) -> bool {
    if scope == "shared" {
        return true;
    }
    let Some((kind, subject)) = scope.split_once(':') else {
        return false;
    };
    if !matches!(kind, "private" | "workspace")
        || subject.is_empty()
        || subject.len() > 256
        || subject.chars().any(char::is_control)
    {
        return false;
    }
    kind != "private" || subject == owner
}

fn validate_error_message(message: &str) -> Result<(), WorkerError> {
    if message.as_bytes().len() > MAX_ERROR_MESSAGE_BYTES {
        return Err(WorkerError::Protocol(
            "worker error message exceeds the bounded limit".to_string(),
        ));
    }
    Ok(())
}

fn validate_incoming_message(message: &Value) -> Result<(), WorkerError> {
    validate_no_authority_escape(message)?;
    match message.get("type").and_then(Value::as_str) {
        Some("response") => {
            if message.get("ok").and_then(Value::as_bool).unwrap_or(false) {
                validate_inline_payload(message.get("result").unwrap_or(&Value::Null))?;
            } else if let Some(error) = message.get("error") {
                let text = error
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                validate_error_message(text)?;
            }
        }
        Some("event") => {
            let payload = message
                .get("payload")
                .or_else(|| message.get("event"))
                .unwrap_or(&Value::Null);
            validate_inline_payload(payload)?;
        }
        Some("hostRequest") => {
            validate_no_authority_escape(message)?;
            validate_inline_payload(message.get("payload").unwrap_or(&Value::Null))?;
        }
        _ => {}
    }
    Ok(())
}

#[derive(Clone, Default)]
pub struct SecretEnv(BTreeMap<String, String>);

impl SecretEnv {
    pub fn insert(&mut self, name: String, value: String) {
        self.0.insert(name, value);
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.0
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str()))
    }
}

impl std::fmt::Debug for SecretEnv {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let configured = self
            .0
            .keys()
            .map(|name| (name.as_str(), true))
            .collect::<Vec<_>>();
        formatter
            .debug_struct("SecretEnv")
            .field("configured", &configured)
            .finish()
    }
}

#[derive(Clone, Debug)]
pub struct WorkerSessionConfig {
    pub executable: PathBuf,
    pub args: Vec<OsString>,
    pub working_directory: Option<PathBuf>,
    pub language: String,
    pub transport: String,
    pub plugin_id: String,
    pub plugin_version: String,
    pub worker_id: String,
    pub session_id: String,
    pub fingerprint: String,
    pub allowed_operations: BTreeSet<String>,
    pub handshake_timeout: Duration,
    pub cancel_grace_period: Duration,
    pub environment: BTreeMap<String, String>,
    pub secret_environment: SecretEnv,
}

impl WorkerSessionConfig {
    pub fn stdio(
        executable: impl Into<PathBuf>,
        args: Vec<OsString>,
        working_directory: Option<PathBuf>,
        language: impl Into<String>,
        plugin_id: impl Into<String>,
        plugin_version: impl Into<String>,
        worker_id: impl Into<String>,
        session_id: impl Into<String>,
        fingerprint: impl Into<String>,
        allowed_operations: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            executable: executable.into(),
            args,
            working_directory,
            language: language.into(),
            transport: WORKER_TRANSPORT_ID.to_string(),
            plugin_id: plugin_id.into(),
            plugin_version: plugin_version.into(),
            worker_id: worker_id.into(),
            session_id: session_id.into(),
            fingerprint: fingerprint.into(),
            allowed_operations: allowed_operations.into_iter().map(Into::into).collect(),
            handshake_timeout: Duration::from_secs(5),
            cancel_grace_period: Duration::from_millis(250),
            environment: BTreeMap::new(),
            secret_environment: SecretEnv::default(),
        }
    }

    pub fn python(
        executable: impl Into<PathBuf>,
        args: Vec<OsString>,
        working_directory: Option<PathBuf>,
        plugin_id: impl Into<String>,
        plugin_version: impl Into<String>,
        worker_id: impl Into<String>,
        allowed_operations: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        let worker_id = worker_id.into();
        Self::stdio(
            executable,
            args,
            working_directory,
            "python",
            plugin_id,
            plugin_version,
            worker_id.clone(),
            worker_id,
            "legacy-python-worker",
            allowed_operations,
        )
    }
}

pub struct PythonWorkerSession {
    child: Child,
    stdin: ChildStdin,
    messages: Receiver<Result<Value, WorkerError>>,
    config: WorkerSessionConfig,
    request_sequence: u64,
    closed: bool,
}

pub type StdioFramedWorkerSession = PythonWorkerSession;

fn terminate_child(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

impl PythonWorkerSession {
    pub fn spawn(config: WorkerSessionConfig) -> Result<Self, WorkerError> {
        if config.allowed_operations.is_empty() {
            return Err(WorkerError::Protocol(
                "worker operation allowlist is empty".to_string(),
            ));
        }

        // No shell, no inherited credentials, and no ambient user environment.
        // The worker receives only transport metadata that is not secret; the
        // authoritative plugin identity remains in this Host-side session.
        let mut command = Command::new(&config.executable);
        command
            .args(&config.args)
            .env_clear()
            .env("ANYWAY_WORKER_TRANSPORT", WORKER_TRANSPORT_ID)
            .env("ANYWAY_WORKER_LANGUAGE", &config.language)
            .env("ANYWAY_PLUGIN_ID", &config.plugin_id)
            .env("ANYWAY_PLUGIN_VERSION", &config.plugin_version)
            .env("ANYWAY_PLUGIN_WORKER_ID", &config.worker_id)
            .env("ANYWAY_PLUGIN_WORKER_SESSION_ID", &config.session_id)
            .env("ANYWAY_PLUGIN_WORKER_FINGERPRINT", &config.fingerprint)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if config.language == "python" {
            command
                .env("PYTHONUNBUFFERED", "1")
                .env("PYTHONIOENCODING", "utf-8");
        }
        for (name, value) in &config.environment {
            command.env(name, value);
        }
        for (name, value) in config.secret_environment.iter() {
            command.env(name, value);
        }
        if let Some(directory) = &config.working_directory {
            command.current_dir(directory);
        }
        let mut child = command
            .spawn()
            .map_err(|error| WorkerError::Io(format!("cannot start Python worker: {error}")))?;
        let stdin = match child.stdin.take() {
            Some(stdin) => stdin,
            None => {
                terminate_child(&mut child);
                return Err(WorkerError::Protocol(
                    "worker stdin was not piped".to_string(),
                ));
            }
        };
        let mut stdout = match child.stdout.take() {
            Some(stdout) => stdout,
            None => {
                terminate_child(&mut child);
                return Err(WorkerError::Protocol(
                    "worker stdout was not piped".to_string(),
                ));
            }
        };
        if let Some(stderr) = child.stderr.take() {
            if let Err(error) = thread::Builder::new()
                .name("anyway-python-worker-stderr".to_string())
                .spawn(move || {
                    use std::io::BufRead;
                    for line in io::BufReader::new(stderr).lines().map_while(Result::ok) {
                        eprintln!("[python-worker] {line}");
                    }
                })
            {
                terminate_child(&mut child);
                return Err(WorkerError::Io(error.to_string()));
            }
        }

        let (sender, messages) = mpsc::channel();
        if let Err(error) = thread::Builder::new()
            .name("anyway-python-worker-stdout".to_string())
            .spawn(move || loop {
                match read_frame(&mut stdout) {
                    Ok(message) => {
                        if sender.send(Ok(message)).is_err() {
                            break;
                        }
                    }
                    Err(error) => {
                        let _ = sender.send(Err(error));
                        break;
                    }
                }
            })
        {
            terminate_child(&mut child);
            return Err(WorkerError::Io(error.to_string()));
        }

        let mut session = Self {
            child,
            stdin,
            messages,
            config,
            request_sequence: 0,
            closed: false,
        };
        session.send(&json!({
            "type": "hello",
            "apiVersion": WORKER_RPC_API_VERSION,
            "pluginId": session.config.plugin_id.clone(),
            "pluginVersion": session.config.plugin_version.clone(),
            "workerId": session.config.worker_id.clone(),
            "sessionId": session.config.session_id.clone(),
            "fingerprint": session.config.fingerprint.clone(),
            "transport": session.config.transport.clone(),
            "allowedOperations": session.config.allowed_operations.iter().cloned().collect::<Vec<_>>(),
        }))?;
        let ack = session.receive(session.config.handshake_timeout)?;
        validate_hello_ack(&ack, &session.config)?;
        Ok(session)
    }

    fn send(&mut self, message: &Value) -> Result<(), WorkerError> {
        write_frame(&mut self.stdin, message)
    }

    fn receive(&mut self, timeout: Duration) -> Result<Value, WorkerError> {
        match self.messages.recv_timeout(timeout) {
            Ok(Ok(message)) => Ok(message),
            Ok(Err(error)) => {
                if matches!(error, WorkerError::UnexpectedEof) {
                    let status = self.child.try_wait().ok().flatten();
                    return Err(WorkerError::ProcessExited(
                        status.and_then(|value| value.code()),
                    ));
                }
                if let Ok(Some(status)) = self.child.try_wait() {
                    Err(WorkerError::ProcessExited(status.code()))
                } else {
                    Err(error)
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => Err(WorkerError::Timeout(
                "response deadline elapsed".to_string(),
            )),
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(WorkerError::ProcessExited(None)),
        }
    }

    pub fn request(
        &mut self,
        operation: &str,
        payload: Value,
        timeout: Duration,
    ) -> Result<Value, WorkerError> {
        self.request_with_host(operation, payload, timeout, |operation, _, _| {
            Err(WorkerError::OperationNotAllowed(operation.to_string()))
        })
    }

    pub fn request_with_host<F>(
        &mut self,
        operation: &str,
        payload: Value,
        timeout: Duration,
        host_call: F,
    ) -> Result<Value, WorkerError>
    where
        F: FnMut(&str, Value, Duration) -> Result<Value, WorkerError>,
    {
        self.request_with_host_cancel(operation, payload, timeout, None, host_call)
    }

    pub fn request_with_host_cancel<F>(
        &mut self,
        operation: &str,
        payload: Value,
        timeout: Duration,
        cancel: Option<&AtomicBool>,
        host_call: F,
    ) -> Result<Value, WorkerError>
    where
        F: FnMut(&str, Value, Duration) -> Result<Value, WorkerError>,
    {
        self.request_with_host_cancel_id(operation, payload, timeout, cancel, None, host_call)
    }

    pub fn request_with_host_cancel_id<F>(
        &mut self,
        operation: &str,
        payload: Value,
        timeout: Duration,
        cancel: Option<&AtomicBool>,
        request_id: Option<&str>,
        mut host_call: F,
    ) -> Result<Value, WorkerError>
    where
        F: FnMut(&str, Value, Duration) -> Result<Value, WorkerError>,
    {
        if !self.config.allowed_operations.contains(operation) {
            return Err(WorkerError::OperationNotAllowed(operation.to_string()));
        }
        validate_inline_payload(&payload)?;
        let request_id = match request_id {
            Some(value)
                if !value.is_empty()
                    && value.len() <= 256
                    && !value.chars().any(char::is_control)
                    && !value.chars().any(char::is_whitespace) =>
            {
                value.to_string()
            }
            Some(_) => {
                return Err(WorkerError::Protocol(
                    "worker request id is invalid".to_string(),
                ))
            }
            None => {
                self.request_sequence += 1;
                format!("{}-{}", self.config.worker_id, self.request_sequence)
            }
        };
        let deadline = Instant::now() + timeout;
        self.send(&json!({
            "type": "request",
            "apiVersion": WORKER_RPC_API_VERSION,
            "requestId": request_id,
            "operation": operation,
            "payload": payload,
            "deadlineMs": timeout.as_millis().min(u64::MAX as u128) as u64,
        }))?;

        let result = match self.receive_until(deadline, cancel) {
            Ok(message) => {
                self.response_for(&request_id, message, deadline, cancel, &mut host_call)
            }
            Err(error) => Err(error),
        };
        match result {
            Ok(value) => Ok(value),
            Err(error @ (WorkerError::Timeout(_) | WorkerError::Cancelled(_))) => {
                let _ = self.send(&json!({
                    "type": "cancel",
                    "apiVersion": WORKER_RPC_API_VERSION,
                    "requestId": request_id,
                }));
                self.terminate_after_cancel_grace()?;
                match error {
                    WorkerError::Timeout(_) => Err(WorkerError::Timeout(format!(
                        "operation {operation} exceeded deadline"
                    ))),
                    WorkerError::Cancelled(_) => Err(WorkerError::Cancelled(format!(
                        "operation {operation} was cancelled by Host request"
                    ))),
                    _ => unreachable!(),
                }
            }
            Err(error) => Err(error),
        }
    }

    fn terminate_after_cancel_grace(&mut self) -> Result<(), WorkerError> {
        let deadline = Instant::now() + self.config.cancel_grace_period;
        while Instant::now() < deadline {
            if let Ok(Some(_)) = self.child.try_wait() {
                self.closed = true;
                return Ok(());
            }
            thread::sleep(
                deadline
                    .saturating_duration_since(Instant::now())
                    .min(Duration::from_millis(5)),
            );
        }
        match self.child.kill() {
            Ok(()) => {
                let _ = self.child.wait();
                self.closed = true;
                Ok(())
            }
            Err(error) => {
                if let Ok(Some(_)) = self.child.try_wait() {
                    self.closed = true;
                    Ok(())
                } else {
                    Err(WorkerError::from(error))
                }
            }
        }
    }

    fn response_for<F>(
        &mut self,
        request_id: &str,
        first_message: Value,
        deadline: Instant,
        cancel: Option<&AtomicBool>,
        host_call: &mut F,
    ) -> Result<Value, WorkerError>
    where
        F: FnMut(&str, Value, Duration) -> Result<Value, WorkerError>,
    {
        let mut message = first_message;
        let mut event_count = 0;
        let mut host_call_count = 0;
        loop {
            validate_incoming_message(&message)?;
            let message_type = message
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if message_type == "event" {
                event_count += 1;
                if event_count > MAX_EVENTS_PER_REQUEST {
                    return Err(WorkerError::Protocol(
                        "worker event limit exceeded".to_string(),
                    ));
                }
                if message.get("requestId").and_then(Value::as_str) != Some(request_id) {
                    return Err(WorkerError::Protocol(
                        "worker event correlation mismatch".to_string(),
                    ));
                }
                message = self.receive_until(deadline, cancel)?;
                continue;
            }
            if message_type == "hostRequest" {
                host_call_count += 1;
                if host_call_count > MAX_HOST_CALLS_PER_REQUEST {
                    return Err(WorkerError::Protocol(
                        "worker Host Bus call limit exceeded".to_string(),
                    ));
                }
                self.handle_host_request(request_id, &message, deadline, host_call)?;
                message = self.receive_until(deadline, cancel)?;
                continue;
            }
            if message_type != "response" {
                return Err(WorkerError::Protocol(format!(
                    "expected response, got {message_type}"
                )));
            }
            let received_id = message
                .get("requestId")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if received_id != request_id {
                return Err(WorkerError::Protocol(format!(
                    "response correlation mismatch: expected {request_id}, got {received_id}"
                )));
            }
            if message.get("ok").and_then(Value::as_bool).unwrap_or(false) {
                return Ok(message.get("result").cloned().unwrap_or(Value::Null));
            }
            let error = message.get("error").cloned().unwrap_or(Value::Null);
            let code = error
                .get("code")
                .and_then(Value::as_str)
                .unwrap_or("REMOTE_ERROR");
            let error_message = error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("worker request failed");
            validate_error_message(error_message)?;
            return Err(WorkerError::Remote {
                code: code.to_string(),
                message: error_message.to_string(),
            });
        }
    }

    fn receive_until(
        &mut self,
        deadline: Instant,
        cancel: Option<&AtomicBool>,
    ) -> Result<Value, WorkerError> {
        loop {
            if cancel.is_some_and(|flag| flag.load(Ordering::Acquire)) {
                return Err(WorkerError::Cancelled(
                    "Host request cancellation was signalled".to_string(),
                ));
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(WorkerError::Timeout(
                    "response deadline elapsed".to_string(),
                ));
            }
            let wait = remaining.min(Duration::from_millis(25));
            match self.messages.recv_timeout(wait) {
                Ok(Ok(message)) => return Ok(message),
                Ok(Err(error)) => {
                    if matches!(error, WorkerError::UnexpectedEof) {
                        let status = self.child.try_wait().ok().flatten();
                        return Err(WorkerError::ProcessExited(
                            status.and_then(|value| value.code()),
                        ));
                    }
                    if let Ok(Some(status)) = self.child.try_wait() {
                        return Err(WorkerError::ProcessExited(status.code()));
                    }
                    return Err(error);
                }
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(WorkerError::ProcessExited(None))
                }
            }
        }
    }

    fn handle_host_request<F>(
        &mut self,
        parent_request_id: &str,
        message: &Value,
        deadline: Instant,
        host_call: &mut F,
    ) -> Result<(), WorkerError>
    where
        F: FnMut(&str, Value, Duration) -> Result<Value, WorkerError>,
    {
        if message.get("apiVersion").and_then(Value::as_str) != Some(WORKER_RPC_API_VERSION) {
            return Err(WorkerError::Protocol(
                "worker Host Bus protocol version mismatch".to_string(),
            ));
        }
        if message.get("parentRequestId").and_then(Value::as_str) != Some(parent_request_id) {
            return Err(WorkerError::Protocol(
                "worker Host Bus parent correlation mismatch".to_string(),
            ));
        }
        let host_request_id = message
            .get("hostRequestId")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty() && value.len() <= 256)
            .ok_or_else(|| {
                WorkerError::Protocol("worker Host Bus request id is invalid".to_string())
            })?;
        let operation = message
            .get("operation")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty() && value.len() <= 160)
            .ok_or_else(|| {
                WorkerError::Protocol("worker Host Bus operation is invalid".to_string())
            })?;
        let requested_ms = message
            .get("deadlineMs")
            .and_then(Value::as_u64)
            .filter(|value| *value > 0)
            .ok_or_else(|| {
                WorkerError::Protocol("worker Host Bus deadline is invalid".to_string())
            })?;
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(WorkerError::Timeout(
                "worker Host Bus deadline elapsed".to_string(),
            ));
        }
        let host_budget = remaining.min(Duration::from_millis(requested_ms));
        let payload = message.get("payload").cloned().unwrap_or(Value::Null);
        let response = match host_call(operation, payload, host_budget) {
            Ok(result) => {
                validate_host_result(operation, &result)?;
                json!({
                    "type": "hostResponse",
                    "apiVersion": WORKER_RPC_API_VERSION,
                    "parentRequestId": parent_request_id,
                    "hostRequestId": host_request_id,
                    "ok": true,
                    "result": result,
                })
            }
            Err(error) => {
                let mut message = error.to_string();
                while message.as_bytes().len() > MAX_ERROR_MESSAGE_BYTES {
                    message.pop();
                }
                json!({
                    "type": "hostResponse",
                    "apiVersion": WORKER_RPC_API_VERSION,
                    "parentRequestId": parent_request_id,
                    "hostRequestId": host_request_id,
                    "ok": false,
                    "error": { "code": "HOST_CALL_FAILED", "message": message },
                })
            }
        };
        self.send(&response)
    }

    pub fn cancel(&mut self, request_id: &str) -> Result<(), WorkerError> {
        self.send(&json!({
            "type": "cancel",
            "apiVersion": WORKER_RPC_API_VERSION,
            "requestId": request_id,
        }))
    }

    pub fn shutdown(&mut self) -> Result<(), WorkerError> {
        if self.closed {
            return Ok(());
        }
        self.send(&json!({
            "type": "shutdown",
            "apiVersion": WORKER_RPC_API_VERSION,
        }))?;
        let deadline = Instant::now() + self.config.cancel_grace_period;
        while Instant::now() < deadline {
            if let Ok(Some(_)) = self.child.try_wait() {
                self.closed = true;
                return Ok(());
            }
            thread::sleep(Duration::from_millis(5));
        }
        self.kill()
    }

    pub fn kill(&mut self) -> Result<(), WorkerError> {
        if self.closed {
            return Ok(());
        }
        self.child.kill().map_err(WorkerError::from)?;
        let _ = self.child.wait();
        self.closed = true;
        Ok(())
    }
}

impl Drop for PythonWorkerSession {
    fn drop(&mut self) {
        let _ = self.kill();
    }
}

pub fn validate_hello_ack(ack: &Value, config: &WorkerSessionConfig) -> Result<(), WorkerError> {
    validate_no_principal(ack)?;
    if ack.get("type").and_then(Value::as_str) != Some("helloAck") {
        return Err(WorkerError::Protocol("expected helloAck".to_string()));
    }
    if ack.get("apiVersion").and_then(Value::as_str) != Some(WORKER_RPC_API_VERSION) {
        return Err(WorkerError::Protocol(
            "worker protocol version mismatch".to_string(),
        ));
    }
    if ack.get("workerId").and_then(Value::as_str) != Some(config.worker_id.as_str()) {
        return Err(WorkerError::Protocol(
            "worker identity mismatch".to_string(),
        ));
    }
    if ack
        .get("sessionId")
        .and_then(Value::as_str)
        .is_some_and(|session_id| session_id != config.session_id.as_str())
    {
        return Err(WorkerError::Protocol(
            "worker session identity mismatch".to_string(),
        ));
    }
    if ack.get("principal").is_some() {
        return Err(WorkerError::Protocol(
            "worker must not negotiate a principal".to_string(),
        ));
    }
    let operations = ack
        .get("operations")
        .and_then(Value::as_array)
        .ok_or_else(|| WorkerError::Protocol("helloAck.operations is required".to_string()))?;
    for operation in operations {
        let name = operation.as_str().ok_or_else(|| {
            WorkerError::Protocol("helloAck operation must be a string".to_string())
        })?;
        if !config.allowed_operations.contains(name) {
            return Err(WorkerError::Protocol(format!(
                "worker negotiated disallowed operation {name}"
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{io::Cursor, process::Command};

    #[test]
    fn frame_codec_handles_partial_reads_and_multiple_frames() {
        let first = encode_frame(&json!({"type":"event","value":1})).unwrap();
        let second = encode_frame(&json!({"type":"response","value":2})).unwrap();
        let mut bytes = first;
        bytes.extend(second);
        let mut reader = Cursor::new(bytes);
        assert_eq!(read_frame(&mut reader).unwrap()["value"], 1);
        assert_eq!(read_frame(&mut reader).unwrap()["value"], 2);
    }

    #[test]
    fn secret_environment_debug_never_contains_plaintext() {
        let mut secrets = SecretEnv::default();
        secrets.insert(
            "ANYWAY_PLUGIN_SECRET_PROVIDER_API_KEY".to_string(),
            "never-print-this-secret-value".to_string(),
        );
        let debug = format!("{secrets:?}");
        assert!(debug.contains("ANYWAY_PLUGIN_SECRET_PROVIDER_API_KEY"));
        assert!(!debug.contains("never-print-this-secret-value"));
    }

    #[test]
    fn frame_codec_rejects_oversize_utf8_and_json() {
        let oversized = json!("x".repeat(MAX_FRAME_BYTES));
        assert!(matches!(
            encode_frame(&oversized),
            Err(WorkerError::FrameTooLarge { .. })
        ));
        assert!(matches!(
            decode_frame(&[0, 0, 0, 1, 0xff]),
            Err(WorkerError::InvalidUtf8)
        ));
        assert!(matches!(
            decode_frame(&[0, 0, 0, 1, b'{']),
            Err(WorkerError::InvalidJson(_))
        ));
    }

    #[test]
    fn oversized_inline_payload_requires_a_valid_blobref() {
        let large = json!({"text": "x".repeat(MAX_INLINE_BYTES + 1)});
        assert!(matches!(
            validate_inline_payload(&large),
            Err(WorkerError::InlinePayloadTooLarge { .. })
        ));
        let blob = json!({"blobRef": {"algorithm":"sha256","digest":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","size":1024,"mediaType":"application/pdf","scope":"private:plugin.a","owner":"plugin.a","retentionClass":"session"}});
        assert!(validate_inline_payload(&blob).is_ok());
        let invalid_blob = json!({"blobRef": {"algorithm":"sha256","digest":"bad","size":1024,"mediaType":"application/pdf","scope":"private:plugin.a","owner":"plugin.a","retentionClass":"session"}});
        assert!(matches!(
            validate_inline_payload(&invalid_blob),
            Err(WorkerError::Protocol(_))
        ));
        let extra_field = json!({"blobRef": {"algorithm":"sha256","digest":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","size":1024,"mediaType":"application/pdf","scope":"private:plugin.a","owner":"plugin.a","retentionClass":"session","path":"secret"}});
        assert!(matches!(
            validate_inline_payload(&extra_field),
            Err(WorkerError::Protocol(_))
        ));
        let principal_field = json!({"blobRef": {"algorithm":"sha256","digest":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","size":1024,"mediaType":"application/pdf","scope":"private:plugin.a","owner":"plugin.a","retentionClass":"session","principal":"spoof"}});
        assert!(matches!(
            validate_inline_payload(&principal_field),
            Err(WorkerError::Protocol(_))
        ));
        let owner_mismatch = json!({"blobRef": {"algorithm":"sha256","digest":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","size":1024,"mediaType":"application/pdf","scope":"private:plugin.a","owner":"plugin.b","retentionClass":"session"}});
        assert!(matches!(
            validate_inline_payload(&owner_mismatch),
            Err(WorkerError::Protocol(_))
        ));

        let file_payload = json!({
            "file": {
                "label": "paper.pdf",
                "blobRef": {
                    "algorithm": "sha256",
                    "digest": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                    "size": 1024,
                    "mediaType": "application/pdf",
                    "scope": "private:plugin.a",
                    "owner": "plugin.a",
                    "retentionClass": "session"
                }
            }
        });
        validate_inline_payload(&file_payload)
            .expect("BlobRef may appear beside bounded business metadata");
    }

    #[test]
    fn host_blobref_cross_language_fixture_is_valid() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../../plugins/sdk/fixtures/host-blob-ref-v1.json"
        ))
        .expect("shared BlobRef fixture");
        validate_blob_ref(&fixture).expect("Host and Python share one BlobRef wire shape");
    }

    #[test]
    fn reverse_host_requests_cannot_submit_identity_or_leases() {
        for payload in [
            json!({"principal":"plugin.attacker"}),
            json!({"leaseId":"forged"}),
            json!({"capabilityLeaseIds":["forged"]}),
        ] {
            let message = json!({
                "type": "hostRequest",
                "apiVersion": WORKER_RPC_API_VERSION,
                "parentRequestId": "parent-1",
                "hostRequestId": "host-1",
                "operation": "blob.read",
                "payload": payload,
                "deadlineMs": 1000
            });
            assert!(
                matches!(
                    validate_incoming_message(&message),
                    Err(WorkerError::Protocol(_))
                ),
                "worker authority escape must be rejected: {message}"
            );
        }
    }

    #[test]
    fn incoming_result_event_and_error_are_bounded() {
        let result = json!({
            "type": "response",
            "ok": true,
            "result": {"text": "x".repeat(MAX_INLINE_BYTES + 1)}
        });
        assert!(matches!(
            validate_incoming_message(&result),
            Err(WorkerError::InlinePayloadTooLarge { .. })
        ));
        let event = json!({
            "type": "event",
            "payload": {"text": "x".repeat(MAX_INLINE_BYTES + 1)}
        });
        assert!(matches!(
            validate_incoming_message(&event),
            Err(WorkerError::InlinePayloadTooLarge { .. })
        ));
        let error = json!({
            "type": "response",
            "ok": false,
            "error": {"code": "REMOTE", "message": "x".repeat(MAX_ERROR_MESSAGE_BYTES + 1)}
        });
        assert!(matches!(
            validate_incoming_message(&error),
            Err(WorkerError::Protocol(_))
        ));
    }

    #[test]
    fn blob_read_host_results_have_an_operation_scoped_binary_budget() {
        let result = json!({
            "digest": "a".repeat(64),
            "size": MAX_BLOB_READ_CHUNK_BYTES,
            "mediaType": "application/pdf",
            "offset": 0,
            "nextOffset": MAX_BLOB_READ_CHUNK_BYTES,
            "eof": true,
            "contentBase64": base64::engine::general_purpose::STANDARD.encode(vec![0_u8; MAX_BLOB_READ_CHUNK_BYTES]),
        });
        let encoded_size = serde_json::to_vec(&result)
            .expect("serialize Blob result")
            .len();
        assert!(encoded_size > MAX_INLINE_BYTES);
        assert!(encoded_size < MAX_BLOB_READ_RESULT_BYTES);
        validate_host_result("blob.read", &result)
            .expect("bounded blob.read result uses its operation-scoped budget");
        assert!(matches!(
            validate_host_result("event.publish", &result),
            Err(WorkerError::InlinePayloadTooLarge { .. })
        ));

        let oversized = json!({
            "contentBase64": base64::engine::general_purpose::STANDARD.encode(vec![0_u8; MAX_BLOB_READ_CHUNK_BYTES + 1]),
        });
        assert!(matches!(
            validate_host_result("blob.read", &oversized),
            Err(WorkerError::InlinePayloadTooLarge { .. })
        ));
    }

    #[test]
    fn hello_ack_rejects_wrong_version_and_disallowed_operation() {
        let config = WorkerSessionConfig::python(
            "python",
            Vec::new(),
            None,
            "myc.pdf-canvas-agent",
            "0.4.0",
            "worker.test",
            ["ping"],
        );
        let wrong_version = json!({"type":"helloAck","apiVersion":"wrong","workerId":"worker.test","operations":["ping"]});
        assert!(validate_hello_ack(&wrong_version, &config).is_err());
        let disallowed = json!({"type":"helloAck","apiVersion":WORKER_RPC_API_VERSION,"workerId":"worker.test","operations":["kernel.bus"]});
        assert!(validate_hello_ack(&disallowed, &config).is_err());
    }

    fn python_available() -> bool {
        Command::new("python").arg("--version").status().is_ok()
    }

    fn python_config(worker_id: &str, operations: &[&str]) -> WorkerSessionConfig {
        let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("repository")
            .to_path_buf();
        WorkerSessionConfig::python(
            "python",
            vec![repository
                .join("my-plugins/anPdfsolver/src/anpdfsolver/worker.py")
                .into_os_string()],
            Some(repository),
            "myc.pdf-canvas-agent",
            "0.4.0",
            worker_id,
            operations.iter().copied(),
        )
    }

    #[test]
    fn python_worker_handshakes_pings_and_keeps_secrets_out_of_environment() {
        if !python_available() {
            eprintln!("python executable unavailable; skipping integration test");
            return;
        }
        let mut session = PythonWorkerSession::spawn(python_config(
            "worker.integration",
            &["ping", "health", "echo-small", "heartbeat"],
        ))
        .expect("Python worker handshake");
        assert_eq!(
            session
                .request("ping", json!({"value":"ok"}), Duration::from_secs(2))
                .expect("ping")["pong"],
            "ok"
        );
        let health = session
            .request("health", json!({}), Duration::from_secs(2))
            .expect("health");
        let secret_present = health
            .get("providerSecretPresent")
            .or_else(|| health.get("secretPresent"))
            .and_then(Value::as_bool);
        assert_eq!(secret_present, Some(false));
        assert!(matches!(
            session.request("kernel.bus", json!({}), Duration::from_secs(1)),
            Err(WorkerError::OperationNotAllowed(_))
        ));
        assert!(matches!(
            session.request(
                "echo-small",
                json!({"principal":"spoof"}),
                Duration::from_secs(1)
            ),
            Err(WorkerError::Protocol(_))
        ));
        session.shutdown().expect("graceful shutdown");
    }

    #[test]
    fn python_worker_timeout_kills_a_stuck_process_and_crash_is_mapped() {
        if !python_available() {
            eprintln!("python executable unavailable; skipping integration test");
            return;
        }
        let script = r#"
import json, struct, sys, time
def read():
    header = sys.stdin.buffer.read(4)
    if not header:
        sys.exit(0)
    size = struct.unpack('>I', header)[0]
    return json.loads(sys.stdin.buffer.read(size))
def send(value):
    payload = json.dumps(value, separators=(',', ':')).encode()
    sys.stdout.buffer.write(struct.pack('>I', len(payload)) + payload)
    sys.stdout.buffer.flush()
hello = read()
send({'type':'helloAck','apiVersion':'researchcanvas.dev/worker-rpc/v1','workerId':hello['workerId'],'operations':['sleep','crash']})
while True:
    request = read()
    if request.get('type') == 'shutdown':
        sys.exit(0)
    if request.get('type') != 'request':
        continue
    if request.get('operation') == 'sleep':
        time.sleep(float(request.get('payload', {}).get('delayMs', 0)) / 1000.0)
        send({'type':'response','apiVersion':'researchcanvas.dev/worker-rpc/v1','requestId':request['requestId'],'ok':True,'result':{'slept':True}})
    elif request.get('operation') == 'crash':
        sys.exit(37)
"#;
        let mut session = PythonWorkerSession::spawn(WorkerSessionConfig::python(
            "python",
            vec!["-c".into(), script.into()],
            None,
            "myc.pdf-canvas-agent",
            "0.4.0",
            "worker.timeout",
            ["health", "sleep", "crash"],
        ))
        .expect("Python worker handshake");
        session.config.cancel_grace_period = Duration::from_millis(25);
        assert!(matches!(
            session.request("sleep", json!({"delayMs":500}), Duration::from_millis(20)),
            Err(WorkerError::Timeout(_))
        ));

        let mut crashed = PythonWorkerSession::spawn(WorkerSessionConfig::python(
            "python",
            vec!["-c".into(), script.into()],
            None,
            "myc.pdf-canvas-agent",
            "0.4.0",
            "worker.crash",
            ["sleep", "crash"],
        ))
        .expect("crash worker handshake");
        assert!(matches!(
            crashed.request("crash", json!({}), Duration::from_secs(1)),
            Err(WorkerError::ProcessExited(_))
        ));
    }

    #[test]
    fn python_worker_event_flood_cannot_extend_the_absolute_deadline() {
        if !python_available() {
            eprintln!("python executable unavailable; skipping integration test");
            return;
        }
        let script = r#"
import json, struct, sys, time
def read():
    header = sys.stdin.buffer.read(4)
    size = struct.unpack('>I', header)[0]
    return json.loads(sys.stdin.buffer.read(size))
def send(value):
    payload = json.dumps(value, separators=(',', ':')).encode()
    sys.stdout.buffer.write(struct.pack('>I', len(payload)) + payload)
    sys.stdout.buffer.flush()
hello = read()
send({'type':'helloAck','apiVersion':'researchcanvas.dev/worker-rpc/v1','workerId':hello['workerId'],'operations':['ping']})
read()
for index in range(1000):
    send({'type':'event','payload':{'index':index}})
    time.sleep(0.002)
"#;
        let config = WorkerSessionConfig::python(
            "python",
            vec!["-c".into(), script.into()],
            None,
            "myc.pdf-canvas-agent",
            "0.4.0",
            "worker.event-flood",
            ["ping"],
        );
        let mut session = PythonWorkerSession::spawn(config).expect("event flood worker handshake");
        assert!(matches!(
            session.request("ping", json!({}), Duration::from_millis(50)),
            Err(WorkerError::Timeout(_)) | Err(WorkerError::Protocol(_))
        ));
        session.kill().expect("event flood worker cleanup");
    }

    #[test]
    fn python_worker_reverse_host_request_is_correlated_and_reentrant() {
        if !python_available() {
            eprintln!("python executable unavailable; skipping integration test");
            return;
        }
        let script = r#"
import json, struct, sys
def read():
    header = sys.stdin.buffer.read(4)
    size = struct.unpack('>I', header)[0]
    return json.loads(sys.stdin.buffer.read(size))
def send(value):
    payload = json.dumps(value, separators=(',', ':')).encode()
    sys.stdout.buffer.write(struct.pack('>I', len(payload)) + payload)
    sys.stdout.buffer.flush()
hello = read()
send({'type':'helloAck','apiVersion':'researchcanvas.dev/worker-rpc/v1','workerId':hello['workerId'],'operations':['surface.action']})
request = read()
send({'type':'hostRequest','apiVersion':'researchcanvas.dev/worker-rpc/v1','parentRequestId':request['requestId'],'hostRequestId':'nested-1','operation':'event.publish','payload':{'topic':'worker.test','payload':{'status':'ok'}},'deadlineMs':500})
host_response = read()
send({'type':'response','apiVersion':'researchcanvas.dev/worker-rpc/v1','requestId':request['requestId'],'ok':True,'result':{'model':{},'state':{},'accepted':host_response.get('ok') is True}})
"#;
        let config = WorkerSessionConfig::python(
            "python",
            vec!["-c".into(), script.into()],
            None,
            "plugin.test",
            "1.0.0",
            "worker.reverse-rpc",
            ["surface.action"],
        );
        let mut session = PythonWorkerSession::spawn(config).expect("reverse RPC handshake");
        let mut calls = 0;
        let result = session
            .request_with_host(
                "surface.action",
                json!({"action":{"actionId":"test"}}),
                Duration::from_secs(2),
                |operation, payload, remaining| {
                    calls += 1;
                    assert_eq!(operation, "event.publish");
                    assert_eq!(payload["topic"], "worker.test");
                    assert!(!remaining.is_zero());
                    Ok(json!({"delivered": 1}))
                },
            )
            .expect("nested Host Bus result");
        assert_eq!(calls, 1);
        assert_eq!(result["accepted"], true);
        session.shutdown().expect("reverse RPC shutdown");
    }

    #[test]
    fn python_worker_accepts_a_bounded_large_blob_read_host_response() {
        if !python_available() {
            eprintln!("python executable unavailable; skipping integration test");
            return;
        }
        let script = r#"
import json, struct, sys
def read():
    header = sys.stdin.buffer.read(4)
    size = struct.unpack('>I', header)[0]
    return json.loads(sys.stdin.buffer.read(size))
def send(value):
    payload = json.dumps(value, separators=(',', ':')).encode()
    sys.stdout.buffer.write(struct.pack('>I', len(payload)) + payload)
    sys.stdout.buffer.flush()
hello = read()
send({'type':'helloAck','apiVersion':'researchcanvas.dev/worker-rpc/v1','workerId':hello['workerId'],'operations':['surface.action']})
request = read()
send({'type':'hostRequest','apiVersion':'researchcanvas.dev/worker-rpc/v1','parentRequestId':request['requestId'],'hostRequestId':'blob-read-1','operation':'blob.read','payload':{'offset':0,'maxBytes':262144},'deadlineMs':1000})
host_response = read()
content = host_response['result']['contentBase64']
send({'type':'response','apiVersion':'researchcanvas.dev/worker-rpc/v1','requestId':request['requestId'],'ok':True,'result':{'accepted':host_response.get('ok') is True,'encodedBytes':len(content)}})
"#;
        let config = WorkerSessionConfig::python(
            "python",
            vec!["-c".into(), script.into()],
            None,
            "plugin.test",
            "1.0.0",
            "worker.blob-read",
            ["surface.action"],
        );
        let mut session = PythonWorkerSession::spawn(config).expect("Blob read worker handshake");
        let result = session
            .request_with_host(
                "surface.action",
                json!({}),
                Duration::from_secs(2),
                |operation, _, _| {
                    assert_eq!(operation, "blob.read");
                    Ok(json!({
                        "digest": "a".repeat(64),
                        "size": MAX_BLOB_READ_CHUNK_BYTES,
                        "mediaType": "application/pdf",
                        "offset": 0,
                        "nextOffset": MAX_BLOB_READ_CHUNK_BYTES,
                        "eof": true,
                        "contentBase64": base64::engine::general_purpose::STANDARD.encode(vec![0_u8; MAX_BLOB_READ_CHUNK_BYTES]),
                    }))
                },
            )
            .expect("bounded large blob.read response crosses the real worker transport");
        assert_eq!(result["accepted"], true);
        assert_eq!(result["encodedBytes"], MAX_BLOB_READ_BASE64_BYTES);
    }
}
