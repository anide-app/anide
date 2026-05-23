use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

use base64::Engine as _;
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

struct PtySession {
    master: Box<dyn portable_pty::MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn portable_pty::Child + Send + Sync>,
    alive: Arc<AtomicBool>,
}

pub struct TerminalState {
    sessions: Mutex<HashMap<String, PtySession>>,
}

impl TerminalState {
    pub fn new() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
        }
    }
}

// ── Shell detection ───────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct ShellInfo {
    pub name: String,
    pub program: String,
    pub args: Vec<String>,
}

fn path_exists(p: &str) -> bool {
    std::path::Path::new(p).exists()
}

/// Search PATH entries for an executable name, returning the full path if found.
fn find_in_path(name: &str) -> Option<String> {
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        #[cfg(target_os = "windows")]
        let candidate = dir.join(name);
        #[cfg(not(target_os = "windows"))]
        let candidate = dir.join(name);
        if candidate.exists() {
            return Some(candidate.to_string_lossy().into_owned());
        }
    }
    None
}

#[tauri::command]
pub fn terminal_list_shells() -> Vec<ShellInfo> {
    let mut shells = Vec::new();

    #[cfg(target_os = "windows")]
    {
        // PowerShell 5 – always present on Windows 10+
        shells.push(ShellInfo {
            name: "PowerShell".into(),
            program: "powershell.exe".into(),
            args: vec![],
        });

        // Command Prompt – always present
        shells.push(ShellInfo {
            name: "Command Prompt".into(),
            program: "cmd.exe".into(),
            args: vec![],
        });

        // Git Bash – check common install locations then PATH
        let git_bash_paths = [
            r"C:\Program Files\Git\bin\bash.exe",
            r"C:\Program Files (x86)\Git\bin\bash.exe",
        ];
        let found_git_bash = git_bash_paths.iter().find(|p| path_exists(p)).copied();
        let git_bash_prog = found_git_bash
            .map(|s| s.to_string())
            .or_else(|| find_in_path("bash.exe"));
        if let Some(prog) = git_bash_prog {
            shells.push(ShellInfo {
                name: "Git Bash".into(),
                program: prog,
                args: vec!["--login".into(), "-i".into()],
            });
        }

        // PowerShell 7 (pwsh) – optional
        let pwsh_paths = [
            r"C:\Program Files\PowerShell\7\pwsh.exe",
            r"C:\Program Files\PowerShell\7-preview\pwsh.exe",
        ];
        let found_pwsh = pwsh_paths.iter().find(|p| path_exists(p)).copied();
        let pwsh_prog = found_pwsh
            .map(|s| s.to_string())
            .or_else(|| find_in_path("pwsh.exe"));
        if let Some(prog) = pwsh_prog {
            shells.push(ShellInfo {
                name: "PowerShell 7".into(),
                program: prog,
                args: vec![],
            });
        }
    }

    #[cfg(target_os = "macos")]
    {
        // zsh – default on macOS Catalina+
        if path_exists("/bin/zsh") {
            shells.push(ShellInfo {
                name: "zsh".into(),
                program: "/bin/zsh".into(),
                args: vec!["-l".into()],
            });
        }
        if path_exists("/bin/bash") {
            shells.push(ShellInfo {
                name: "bash".into(),
                program: "/bin/bash".into(),
                args: vec!["-l".into()],
            });
        }
        // fish – Homebrew (Apple Silicon and Intel)
        for fish in ["/opt/homebrew/bin/fish", "/usr/local/bin/fish"] {
            if path_exists(fish) {
                shells.push(ShellInfo {
                    name: "fish".into(),
                    program: fish.into(),
                    args: vec!["-l".into()],
                });
                break;
            }
        }
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        // Linux: read /etc/shells for the authoritative list
        let etc_shells = std::fs::read_to_string("/etc/shells").unwrap_or_default();
        let from_etc: Vec<ShellInfo> = etc_shells
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty() && !l.starts_with('#') && path_exists(l))
            .map(|l| ShellInfo {
                name: l.split('/').last().unwrap_or(l).to_string(),
                program: l.to_string(),
                args: vec![],
            })
            .collect();
        shells.extend(from_etc);

        // Fallback if /etc/shells is missing or empty
        if shells.is_empty() {
            for (name, path) in [("bash", "/bin/bash"), ("sh", "/bin/sh")] {
                if path_exists(path) {
                    shells.push(ShellInfo {
                        name: name.into(),
                        program: path.into(),
                        args: vec![],
                    });
                }
            }
        }
    }

    shells
}

// ── PATH augmentation ─────────────────────────────────────────────────────────
// The built app is launched by the OS without a shell profile, so PATH only
// contains registry/system entries. Augment it with common user-local tool
// directories so CLIs installed via cargo, npm, scoop, brew, etc. are visible.
fn augmented_path() -> String {
    let current = std::env::var("PATH").unwrap_or_default();
    let sep = if cfg!(target_os = "windows") { ";" } else { ":" };

    let mut extras: Vec<String> = Vec::new();

    #[cfg(target_os = "windows")]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            extras.push(format!(r"{appdata}\npm"));
        }
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            extras.push(format!(r"{local}\pnpm"));
        }
        if let Ok(home) = std::env::var("USERPROFILE") {
            extras.push(format!(r"{home}\.cargo\bin"));
            extras.push(format!(r"{home}\scoop\shims"));
            extras.push(format!(r"{home}\.local\bin"));
            extras.push(format!(r"{home}\.bun\bin"));
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        if let Ok(home) = std::env::var("HOME") {
            extras.push(format!("{home}/.cargo/bin"));
            extras.push(format!("{home}/.local/bin"));
            extras.push(format!("{home}/.bun/bin"));
            extras.push(format!("{home}/.deno/bin"));
        }
        for p in ["/opt/homebrew/bin", "/opt/homebrew/sbin", "/usr/local/bin"] {
            extras.push(p.to_string());
        }
    }

    let existing: std::collections::HashSet<&str> = current.split(sep).collect();
    let new_paths: Vec<String> = extras
        .into_iter()
        .filter(|p| !existing.contains(p.as_str()) && std::path::Path::new(p).exists())
        .collect();

    if new_paths.is_empty() {
        current
    } else {
        format!("{}{sep}{}", current, new_paths.join(sep))
    }
}

// ── PTY session management ────────────────────────────────────────────────────

#[tauri::command]
pub fn terminal_create(
    app: AppHandle,
    state: State<'_, TerminalState>,
    session_id: String,
    cwd: Option<String>,
    cols: u16,
    rows: u16,
    shell: Option<String>,
    shell_args: Option<Vec<String>>,
) -> Result<(), String> {
    let pty_system = native_pty_system();

    let pair = pty_system
        .openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| e.to_string())?;

    #[cfg(target_os = "windows")]
    let default_shell = "powershell.exe".to_string();
    #[cfg(not(target_os = "windows"))]
    let default_shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());

    let shell_program = shell.as_deref().unwrap_or(&default_shell);
    let mut cmd = CommandBuilder::new(shell_program);

    for arg in shell_args.iter().flatten() {
        cmd.arg(arg.as_str());
    }

    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    cmd.env("PATH", augmented_path());
    if let Some(ref path) = cwd {
        if !path.is_empty() {
            cmd.cwd(path);
        }
    }

    let child = pair
        .slave
        .spawn_command(cmd)
        .map_err(|e| e.to_string())?;

    // Must drop slave after spawning so the master gets EOF when the child exits
    drop(pair.slave);

    let writer = pair.master.take_writer().map_err(|e| e.to_string())?;
    let mut reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;

    let alive = Arc::new(AtomicBool::new(true));
    let alive_reader = alive.clone();
    let sid = session_id.clone();
    let app_reader = app.clone();

    // Dedicated reader thread: blocks on PTY output, emits events to frontend.
    // Base64-encodes the raw bytes so binary data survives JSON IPC intact.
    std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        while alive_reader.load(Ordering::Relaxed) {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => {
                    alive_reader.store(false, Ordering::Relaxed);
                    app_reader.emit(&format!("terminal:exit:{sid}"), ()).ok();
                    break;
                }
                Ok(n) => {
                    let encoded = base64::engine::general_purpose::STANDARD.encode(&buf[..n]);
                    app_reader
                        .emit(&format!("terminal:data:{sid}"), encoded)
                        .ok();
                }
            }
        }
    });

    let mut sessions = state.sessions.lock().map_err(|_| "session lock poisoned".to_string())?;
    if sessions.contains_key(&session_id) {
        return Err(format!("session already exists: {session_id}"));
    }
    sessions.insert(
        session_id,
        PtySession { master: pair.master, writer, child, alive },
    );

    Ok(())
}

#[tauri::command]
pub fn terminal_write(
    state: State<'_, TerminalState>,
    session_id: String,
    data: String,
) -> Result<(), String> {
    let mut sessions = state.sessions.lock().map_err(|_| "session lock poisoned".to_string())?;
    let session = sessions
        .get_mut(&session_id)
        .ok_or_else(|| format!("session not found: {session_id}"))?;
    session.writer.write_all(data.as_bytes()).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn terminal_resize(
    state: State<'_, TerminalState>,
    session_id: String,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    let sessions = state.sessions.lock().map_err(|_| "session lock poisoned".to_string())?;
    let session = sessions
        .get(&session_id)
        .ok_or_else(|| format!("session not found: {session_id}"))?;
    session
        .master
        .resize(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn terminal_close(
    state: State<'_, TerminalState>,
    session_id: String,
) -> Result<(), String> {
    if let Some(mut session) = state.sessions.lock().map_err(|_| "session lock poisoned".to_string())?.remove(&session_id) {
        session.alive.store(false, Ordering::Relaxed);
        let _ = session.child.kill();
    }
    Ok(())
}
