use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

macro_rules! no_window {
    ($cmd:expr) => {
        #[cfg(target_os = "windows")]
        {
            #[allow(unused_imports)]
            use std::os::windows::process::CommandExt as _;
            $cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }
    };
}

use bollard::container::{
    ListContainersOptions, LogOutput, LogsOptions, RemoveContainerOptions,
    RestartContainerOptions, StartContainerOptions, StopContainerOptions,
};
use bollard::image::{ListImagesOptions, RemoveImageOptions};
use bollard::system::EventsOptions;
use bollard::Docker;
use futures_util::StreamExt;
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use tokio::time::{timeout, Duration};

use crate::error::AppError;

// ── State ─────────────────────────────────────────────────────────────────────

pub struct DockerStreamState {
    pub streams: Mutex<HashMap<String, Arc<AtomicBool>>>,
}

impl DockerStreamState {
    pub fn new() -> Self {
        Self { streams: Mutex::new(HashMap::new()) }
    }
}

pub struct DockerEventState {
    pub flag: Mutex<Option<Arc<AtomicBool>>>,
}

impl DockerEventState {
    pub fn new() -> Self {
        Self { flag: Mutex::new(None) }
    }
}

// ── Serializable types ────────────────────────────────────────────────────────

#[derive(Serialize, Clone)]
pub struct ContainerInfo {
    pub id: String,
    pub short_id: String,
    pub names: Vec<String>,
    pub image: String,
    pub status: String,
    pub state: String,
    pub created: i64,
    pub compose_project: Option<String>,
    pub compose_service: Option<String>,
}

#[derive(Serialize, Clone)]
pub struct ImageInfo {
    pub id: String,
    pub short_id: String,
    pub repo_tags: Vec<String>,
    pub size: i64,
    pub created: i64,
}

#[derive(Serialize, Clone)]
pub struct ComposeFile {
    pub rel_path: String,
    pub name: String,
}

#[derive(Serialize, Clone)]
pub struct DockerLogLine {
    pub timestamp: Option<String>,
    pub message: String,
    pub stream: String,
}

#[derive(Serialize, Clone)]
pub struct ExecResult {
    pub stdout: String,
    pub stderr: String,
}

#[derive(Serialize, Clone)]
struct DockerComposeLinePayload {
    pub line: String,
    pub stream: String,
    pub done: bool,
}

// ── Shared Docker client ──────────────────────────────────────────────────────
// One instance for the lifetime of the process. Clone is cheap (Arc internally)
// and shares the underlying hyper connection pool, so connections are reused
// across all commands instead of being re-established on every call.

static DOCKER: OnceLock<Option<Docker>> = OnceLock::new();

fn docker() -> Option<&'static Docker> {
    DOCKER
        .get_or_init(|| {
            Docker::connect_with_local_defaults()
                .map_err(|e| eprintln!("[docker] client init failed: {e}"))
                .ok()
        })
        .as_ref()
}

fn docker_client() -> Result<Docker, AppError> {
    docker()
        .cloned()
        .ok_or_else(|| AppError::Other("Docker is not available on this machine".into()))
}

/// Spawn a background task that pings Docker at app startup.
/// Best-effort only — runs entirely off the main thread so a missing daemon
/// cannot block or crash launch.
pub fn prewarm_docker() {
    tauri::async_runtime::spawn(async move {
        if let Ok(docker) = docker_client() {
            let _ = timeout(Duration::from_secs(3), docker.ping()).await;
        }
    });
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn parse_log_output(output: LogOutput) -> Option<DockerLogLine> {
    let (stream, bytes) = match output {
        LogOutput::StdOut { message } => ("stdout", message),
        LogOutput::StdErr { message } => ("stderr", message),
        // Console = TTY containers (no multiplexing header) — treat as stdout
        LogOutput::Console { message } => ("stdout", message),
        LogOutput::StdIn { .. } => return None,
    };
    let raw = String::from_utf8_lossy(&bytes);
    let raw = raw.trim_end_matches('\n');
    if raw.is_empty() {
        return None;
    }

    // When timestamps=true Docker prepends: "2024-01-15T10:30:00.123Z message"
    let (timestamp, message) = if let Some(space) = raw.find(' ') {
        let prefix = &raw[..space];
        if prefix.contains('T') && (prefix.ends_with('Z') || prefix.contains('+')) {
            (Some(prefix.to_string()), raw[space + 1..].to_string())
        } else {
            (None, raw.to_string())
        }
    } else {
        (None, raw.to_string())
    };

    Some(DockerLogLine { timestamp, message, stream: stream.to_string() })
}

fn scan_compose_files(root: &Path, dir: &Path, out: &mut Vec<ComposeFile>, depth: usize) {
    if depth > 3 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if path.is_dir() {
            if matches!(
                name_str.as_ref(),
                "node_modules" | "target" | "dist" | ".git" | ".svelte-kit" | ".next" | "venv"
            ) {
                continue;
            }
            scan_compose_files(root, &path, out, depth + 1);
        } else if path.is_file() && is_yaml_file(&name_str) && is_compose_file(&path) {
            if let Ok(rel) = path.strip_prefix(root) {
                out.push(ComposeFile {
                    rel_path: rel.to_string_lossy().replace('\\', "/"),
                    name: name_str.to_string(),
                });
            }
        }
    }
}

fn is_yaml_file(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.ends_with(".yml") || lower.ends_with(".yaml")
}

// Detect compose files by content: any YAML with a top-level `services:` key.
// Only scan the first 50 lines — `services:` is always near the top.
fn is_compose_file(path: &Path) -> bool {
    let Ok(content) = std::fs::read_to_string(path) else { return false };
    content.lines().take(50).any(|line| line.starts_with("services:"))
}

fn validate_path_in_project(project_path: &str, rel_path: &str) -> Result<PathBuf, AppError> {
    let root = PathBuf::from(project_path);
    let full = root.join(rel_path);
    let canonical_root = root.canonicalize()?;
    let canonical_full = full
        .canonicalize()
        .map_err(|_| AppError::NotFound(rel_path.to_string()))?;
    if !canonical_full.starts_with(&canonical_root) {
        return Err(AppError::InvalidPath(rel_path.to_string()));
    }
    Ok(canonical_full)
}

// ── Commands ──────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn docker_list_containers() -> Result<Vec<ContainerInfo>, AppError> {
    let docker = docker_client()?;
    let opts = ListContainersOptions::<String> { all: true, ..Default::default() };
    let list = timeout(Duration::from_secs(5), docker.list_containers(Some(opts)))
        .await
        .map_err(|_| AppError::Other("Docker not responding (timeout)".into()))?
        .map_err(|e| AppError::Other(e.to_string()))?;

    Ok(list
        .into_iter()
        .map(|c| {
            let labels = c.labels.unwrap_or_default();
            let id = c.id.clone().unwrap_or_default();
            let short_id: String = id.chars().take(12).collect();
            ContainerInfo {
                id,
                short_id,
                names: c
                    .names
                    .unwrap_or_default()
                    .into_iter()
                    .map(|n| n.trim_start_matches('/').to_string())
                    .collect(),
                image: c.image.unwrap_or_default(),
                status: c.status.unwrap_or_default(),
                state: c.state.unwrap_or_default(),
                created: c.created.unwrap_or(0),
                compose_project: labels.get("com.docker.compose.project").cloned(),
                compose_service: labels.get("com.docker.compose.service").cloned(),
            }
        })
        .collect())
}

#[tauri::command]
pub async fn docker_list_images() -> Result<Vec<ImageInfo>, AppError> {
    let docker = docker_client()?;
    let list = timeout(
        Duration::from_secs(5),
        docker.list_images(Some(ListImagesOptions::<String> { all: false, ..Default::default() })),
    )
    .await
    .map_err(|_| AppError::Other("Docker not responding (timeout)".into()))?
    .map_err(|e| AppError::Other(e.to_string()))?;

    Ok(list
        .into_iter()
        .map(|img| {
            let id = img.id.clone();
            let short_id: String = id
                .strip_prefix("sha256:")
                .unwrap_or(&id)
                .chars()
                .take(12)
                .collect();
            ImageInfo {
                id: img.id,
                short_id,
                repo_tags: img.repo_tags,
                size: img.size,
                created: img.created,
            }
        })
        .collect())
}

#[tauri::command]
pub fn docker_list_compose_files(project_path: String) -> Result<Vec<ComposeFile>, AppError> {
    let root = PathBuf::from(&project_path);
    let mut files = Vec::new();
    scan_compose_files(&root, &root, &mut files, 0);
    Ok(files)
}

#[tauri::command]
pub async fn docker_container_start(container_id: String) -> Result<(), AppError> {
    let docker = docker_client()?;
    docker
        .start_container(&container_id, None::<StartContainerOptions<String>>)
        .await
        .map_err(|e| AppError::Other(e.to_string()))
}

#[tauri::command]
pub async fn docker_container_stop(container_id: String) -> Result<(), AppError> {
    let docker = docker_client()?;
    docker
        .stop_container(&container_id, None::<StopContainerOptions>)
        .await
        .map_err(|e| AppError::Other(e.to_string()))
}

#[tauri::command]
pub async fn docker_container_restart(container_id: String) -> Result<(), AppError> {
    let docker = docker_client()?;
    docker
        .restart_container(&container_id, None::<RestartContainerOptions>)
        .await
        .map_err(|e| AppError::Other(e.to_string()))
}

#[tauri::command]
pub async fn docker_container_remove(container_id: String, force: bool) -> Result<(), AppError> {
    let docker = docker_client()?;
    docker
        .remove_container(
            &container_id,
            Some(RemoveContainerOptions { force, v: false, link: false }),
        )
        .await
        .map_err(|e| AppError::Other(e.to_string()))
}

#[tauri::command]
pub async fn docker_image_remove(image_id: String, force: bool) -> Result<(), AppError> {
    let docker = docker_client()?;
    docker
        .remove_image(&image_id, Some(RemoveImageOptions { force, noprune: false }), None)
        .await
        .map_err(|e| AppError::Other(e.to_string()))?;
    Ok(())
}

#[tauri::command]
pub async fn docker_start_log_stream(
    app: AppHandle,
    state: State<'_, DockerStreamState>,
    container_id: String,
    tail: u64,
) -> Result<(), AppError> {
    let flag = Arc::new(AtomicBool::new(true));
    {
        let mut streams = state
            .streams
            .lock()
            .map_err(|_| AppError::Other("docker stream lock poisoned".into()))?;
        if let Some(old) = streams.get(&container_id) {
            old.store(false, Ordering::SeqCst);
        }
        streams.insert(container_id.clone(), Arc::clone(&flag));
    }

    let cid = container_id.clone();
    let app_clone = app.clone();

    tauri::async_runtime::spawn(async move {
        let docker = match docker_client() {
            Ok(d) => d,
            Err(e) => {
                let _ = app_clone.emit(
                    &format!("docker:log:{}", cid),
                    DockerLogLine {
                        timestamp: None,
                        message: format!("[error: {}]", e),
                        stream: "stderr".into(),
                    },
                );
                return;
            }
        };

        let opts = LogsOptions::<String> {
            follow: true,
            stdout: true,
            stderr: true,
            timestamps: true,
            tail: if tail == 0 { "all".to_string() } else { tail.to_string() },
            ..Default::default()
        };

        eprintln!("[log_stream:{}] starting (tail={})", cid.get(..12).unwrap_or(&cid), tail);
        let mut stream = docker.logs(&cid, Some(opts));
        let mut emitted = 0u64;

        while let Some(result) = stream.next().await {
            if !flag.load(Ordering::SeqCst) {
                break;
            }
            match result {
                Ok(output) => {
                    if let Some(line) = parse_log_output(output) {
                        emitted += 1;
                        let _ = app_clone.emit(&format!("docker:log:{}", cid), line);
                    }
                }
                Err(e) => {
                    eprintln!("[log_stream:{}] stream error: {e}", cid.get(..12).unwrap_or(&cid));
                    let _ = app_clone.emit(
                        &format!("docker:log:{}", cid),
                        DockerLogLine {
                            timestamp: None,
                            message: format!("[stream error: {}]", e),
                            stream: "stderr".into(),
                        },
                    );
                    break;
                }
            }
        }

        eprintln!("[log_stream:{}] ended, emitted={emitted}", cid.get(..12).unwrap_or(&cid));
        let _ = app_clone.emit(&format!("docker:log-end:{}", cid), ());
    });

    Ok(())
}

#[tauri::command]
pub fn docker_stop_log_stream(
    state: State<'_, DockerStreamState>,
    container_id: String,
) -> Result<(), AppError> {
    if let Ok(mut streams) = state.streams.lock() {
        if let Some(flag) = streams.remove(&container_id) {
            flag.store(false, Ordering::SeqCst);
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn docker_compose_up(
    project_path: String,
    compose_rel_path: String,
) -> Result<(), AppError> {
    run_compose(project_path, compose_rel_path, &["up", "--detach"]).await
}

#[tauri::command]
pub async fn docker_compose_down(
    project_path: String,
    compose_rel_path: String,
) -> Result<(), AppError> {
    run_compose(project_path, compose_rel_path, &["down"]).await
}

// ── Engine status / start / stop ──────────────────────────────────────────────

/// Returns true if the Docker daemon is reachable and answering API requests.
/// Uses bollard's GET /_ping — a real API roundtrip, not just a socket open.
/// 2-second timeout so it fails fast when Docker is starting up or in a bad state.
#[tauri::command]
pub async fn docker_ping() -> bool {
    let Ok(docker) = docker_client() else {
        return false;
    };
    match timeout(Duration::from_secs(2), docker.ping()).await {
        Ok(Ok(_)) => true,
        _ => false,
    }
}

/// Launch the Docker engine / Docker Desktop.
#[tauri::command]
pub fn docker_start_engine() -> Result<(), AppError> {
    #[cfg(target_os = "windows")]
    return start_engine_windows();

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .args(["-a", "Docker"])
            .spawn()
            .map_err(|e| AppError::Other(format!("Failed to open Docker Desktop: {e}")))?;
        return Ok(());
    }

    #[cfg(target_os = "linux")]
    {
        if std::process::Command::new("systemctl")
            .args(["start", "docker"])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
        {
            return Ok(());
        }
        std::process::Command::new("service")
            .args(["docker", "start"])
            .spawn()
            .map_err(|e| AppError::Other(format!("Failed to start Docker: {e}")))?;
        return Ok(());
    }
}

#[cfg(target_os = "windows")]
fn find_docker_desktop() -> Option<std::path::PathBuf> {
    let localappdata = std::env::var("LOCALAPPDATA").unwrap_or_default();
    let programfiles = std::env::var("PROGRAMFILES").unwrap_or_default();
    let programw6432 = std::env::var("PROGRAMW6432").unwrap_or_default();

    eprintln!("[find_docker_desktop] LOCALAPPDATA={localappdata}");
    eprintln!("[find_docker_desktop] PROGRAMFILES={programfiles}");
    eprintln!("[find_docker_desktop] PROGRAMW6432={programw6432}");

    let candidates: Vec<std::path::PathBuf> = [
        if localappdata.is_empty() { None } else {
            Some(std::path::PathBuf::from(&localappdata).join(r"Programs\Docker\Docker\Docker Desktop.exe"))
        },
        if programfiles.is_empty() { None } else {
            Some(std::path::PathBuf::from(&programfiles).join(r"Docker\Docker\Docker Desktop.exe"))
        },
        if programw6432.is_empty() { None } else {
            Some(std::path::PathBuf::from(&programw6432).join(r"Docker\Docker\Docker Desktop.exe"))
        },
        Some(std::path::PathBuf::from(r"C:\Program Files\Docker\Docker\Docker Desktop.exe")),
        Some(std::path::PathBuf::from(r"C:\Program Files (x86)\Docker\Docker\Docker Desktop.exe")),
    ]
    .into_iter()
    .flatten()
    .collect();

    for p in &candidates {
        eprintln!("[find_docker_desktop] checking {} → exists={}", p.display(), p.exists());
    }

    let found = candidates.into_iter().find(|p| p.exists());
    eprintln!("[find_docker_desktop] result: {:?}", found);
    found
}

#[cfg(target_os = "windows")]
fn start_engine_windows() -> Result<(), AppError> {
    eprintln!("[start_engine_windows] called");

    match find_docker_desktop() {
        None => {
            eprintln!("[start_engine_windows] exe not found in any candidate path");
        }
        Some(path) => {
            eprintln!("[start_engine_windows] launching: {}", path.display());
            match std::process::Command::new(&path).spawn() {
                Ok(child) => {
                    eprintln!("[start_engine_windows] spawned, pid={}", child.id());
                    return Ok(());
                }
                Err(e) => {
                    eprintln!("[start_engine_windows] spawn error: {e}");
                    return Err(AppError::Other(format!(
                        "Found Docker Desktop at {} but failed to launch: {e}",
                        path.display()
                    )));
                }
            }
        }
    }

    // Last resort: Windows service (requires elevation)
    eprintln!("[start_engine_windows] falling back to net start com.docker.service");
    let svc = std::process::Command::new("net")
        .args(["start", "com.docker.service"])
        .output();
    eprintln!("[start_engine_windows] net start result: {:?}", svc.map(|o| o.status));

    Err(AppError::Other(
        "Could not find Docker Desktop. \
         Checked: %LOCALAPPDATA%\\Programs\\Docker, %PROGRAMFILES%\\Docker, C:\\Program Files\\Docker. \
         Please start Docker Desktop manually from the Start Menu."
            .into(),
    ))
}

/// Stop the Docker engine / Docker Desktop.
#[tauri::command]
pub fn docker_stop_engine() -> Result<(), AppError> {
    #[cfg(target_os = "windows")]
    {
        // Try the service first (clean stop), then kill the UI
        let _ = std::process::Command::new("net")
            .args(["stop", "com.docker.service"])
            .output();
        if let Some(path) = find_docker_desktop() {
            let _ = std::process::Command::new(&path).arg("-Quit").spawn();
        } else {
            std::process::Command::new("taskkill")
                .args(["/F", "/IM", "Docker Desktop.exe"])
                .spawn()
                .map_err(|e| AppError::Other(format!("Failed to stop Docker Desktop: {e}")))?;
        }
        return Ok(());
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("osascript")
            .args(["-e", r#"quit app "Docker Desktop""#])
            .spawn()
            .map_err(|e| AppError::Other(format!("Failed to quit Docker Desktop: {e}")))?;
        return Ok(());
    }

    #[cfg(target_os = "linux")]
    {
        if std::process::Command::new("systemctl")
            .args(["stop", "docker"])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
        {
            return Ok(());
        }
        std::process::Command::new("service")
            .args(["docker", "stop"])
            .spawn()
            .map_err(|e| AppError::Other(format!("Failed to stop Docker: {e}")))?;
        return Ok(());
    }
}

/// Run a shell command inside a container and return stdout + stderr.
/// Uses `sh -c` so the full shell syntax (pipes, quotes, etc.) works.
#[tauri::command]
pub async fn docker_exec_cmd(container_id: String, cmd: String) -> Result<ExecResult, AppError> {
    let mut dcmd = tokio::process::Command::new("docker");
    dcmd.args(["exec", &container_id, "sh", "-c", &cmd]);
    no_window!(dcmd);
    let result = timeout(Duration::from_secs(30), dcmd.output()).await
    .map_err(|_| AppError::Other("exec timed out after 30s".into()))?
    .map_err(|e| AppError::Other(format!("docker exec: {e}")))?;

    Ok(ExecResult {
        stdout: String::from_utf8_lossy(&result.stdout).trim_end().to_string(),
        stderr: String::from_utf8_lossy(&result.stderr).trim_end().to_string(),
    })
}

/// Stream Docker container events and emit `docker:event` to the frontend.
/// Uses tokio::select so the task exits within ~500 ms after stop is requested.
#[tauri::command]
pub async fn docker_watch_events(
    app: AppHandle,
    state: State<'_, DockerEventState>,
) -> Result<(), AppError> {
    let new_flag = Arc::new(AtomicBool::new(true));
    {
        let mut guard = state.flag.lock().map_err(|_| AppError::Other("lock poisoned".into()))?;
        if let Some(old) = guard.take() {
            old.store(false, Ordering::SeqCst);
        }
        *guard = Some(Arc::clone(&new_flag));
    }

    let app_clone = app.clone();
    let flag = new_flag;

    tauri::async_runtime::spawn(async move {
        let docker = match docker_client() {
            Ok(d) => d,
            Err(_) => return,
        };

        let opts: Option<EventsOptions<String>> = None;
        let mut stream = docker.events(opts);

        loop {
            tokio::select! {
                result = stream.next() => {
                    match result {
                        Some(Ok(_)) => {
                            if !flag.load(Ordering::SeqCst) { break; }
                            let _ = app_clone.emit("docker:event", ());
                        }
                        _ => break,
                    }
                }
                _ = tokio::time::sleep(Duration::from_millis(500)) => {
                    if !flag.load(Ordering::SeqCst) { break; }
                }
            }
        }
    });

    Ok(())
}

#[tauri::command]
pub fn docker_stop_watch_events(state: State<'_, DockerEventState>) -> Result<(), AppError> {
    if let Ok(mut guard) = state.flag.lock() {
        if let Some(flag) = guard.take() {
            flag.store(false, Ordering::SeqCst);
        }
    }
    Ok(())
}

async fn run_compose(
    project_path: String,
    compose_rel_path: String,
    sub_args: &[&str],
) -> Result<(), AppError> {
    let canonical = validate_path_in_project(&project_path, &compose_rel_path)?;
    let compose_str = canonical
        .to_str()
        .ok_or_else(|| AppError::InvalidPath("non-UTF8 path".into()))?
        .to_string();

    let mut args = vec!["compose".to_string(), "-f".to_string(), compose_str];
    args.extend(sub_args.iter().map(|s| s.to_string()));

    let mut dcmd = tokio::process::Command::new("docker");
    dcmd.args(&args).current_dir(&project_path);
    no_window!(dcmd);
    let output = dcmd.output().await
        .map_err(|e| AppError::Other(format!("Failed to run docker: {e}")))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(AppError::Other(format!(
            "docker compose failed: {}",
            stderr.trim()
        )))
    }
}