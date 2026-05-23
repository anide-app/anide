use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::process::Command;

use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use gix::bstr::ByteSlice;
use imara_diff::intern::InternedInput;
use imara_diff::{diff as imara_diff_fn, Algorithm, UnifiedDiffBuilder};
use serde::Serialize;
use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;

use crate::error::AppError;

/// Hide the console window that Windows creates for every subprocess spawn.
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

// ── Types ─────────────────────────────────────────────────────────────────────

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct FileStatus {
    pub path: String,
    pub index_status: Option<ChangeKind>,
    pub worktree_status: Option<ChangeKind>,
    pub file_kind: FileKind,
    pub conflicted: bool,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum ChangeKind {
    Added,
    Modified,
    Deleted,
    Renamed { from: String },
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub enum FileKind {
    Text,
    Image,
    Binary,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffResult {
    pub hunks: Vec<DiffHunk>,
    pub truncated: bool,
    pub total_added: u32,
    pub total_removed: u32,
    pub is_binary: bool,
    pub is_image: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffHunk {
    pub old_start: u32,
    pub old_lines: u32,
    pub new_start: u32,
    pub new_lines: u32,
    pub lines: Vec<DiffLine>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffLine {
    pub kind: LineKind,
    pub content: String,
    pub old_lineno: Option<u32>,
    pub new_lineno: Option<u32>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LineKind {
    Added,
    Removed,
    Context,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitInfo {
    pub hash: String,
    pub short_hash: String,
    pub summary: String,
    pub body: Option<String>,
    pub author_name: String,
    pub author_email: String,
    pub timestamp: i64,
    pub parents: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchInfo {
    pub name: String,
    pub is_remote: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchList {
    pub current: String,
    pub is_detached: bool,
    pub branches: Vec<BranchInfo>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageBlob {
    pub data: String,
    pub mime: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteStatus {
    pub ahead: usize,
    pub behind: usize,
    pub remote_name: Option<String>,
    pub remote_branch: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusResult {
    pub files: Vec<FileStatus>,
    pub total: usize,
}

// ── Helpers ───────────────────────────────────────────────────────────────────

const STATUS_LIMIT: usize = 2_000;

const IMAGE_EXTS: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "svg", "webp", "ico", "bmp", "tiff", "avif",
];

fn classify_path(path: &str) -> FileKind {
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default();
    if IMAGE_EXTS.contains(&ext.as_str()) {
        FileKind::Image
    } else {
        FileKind::Text
    }
}

fn sniff_binary(data: &[u8]) -> bool {
    data[..data.len().min(8192)].contains(&0u8)
}

fn mime_for_ext(ext: &str) -> &'static str {
    match ext {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "bmp" => "image/bmp",
        "tiff" => "image/tiff",
        "avif" => "image/avif",
        _ => "application/octet-stream",
    }
}

fn open_repo(path: &str) -> Result<gix::Repository, AppError> {
    gix::open(path).map_err(|e| AppError::Git(e.to_string()))
}

/// Recursively collect all blob entries from a tree into a path→oid map.
fn collect_tree_entries(
    repo: &gix::Repository,
    tree: &gix::Tree<'_>,
    prefix: &str,
    out: &mut BTreeMap<String, gix::ObjectId>,
) -> Result<(), AppError> {
    use gix::object::tree::EntryKind;
    for entry_ref in tree.iter() {
        let entry = entry_ref.map_err(|e| AppError::Git(e.to_string()))?;
        let name = entry.filename().to_str_lossy();
        let full = if prefix.is_empty() {
            name.into_owned()
        } else {
            format!("{prefix}/{name}")
        };
        match entry.mode().kind() {
            EntryKind::Tree => {
                let sub = repo
                    .find_object(entry.oid())
                    .map_err(|e| AppError::Git(e.to_string()))?
                    .try_into_tree()
                    .map_err(|e| AppError::Git(e.to_string()))?;
                collect_tree_entries(repo, &sub, &full, out)?;
            }
            EntryKind::Blob | EntryKind::BlobExecutable | EntryKind::Link => {
                out.insert(full, entry.oid().to_owned());
            }
            _ => {}
        }
    }
    Ok(())
}

/// Compute unified diff between two byte slices.
fn diff_bytes(old: &[u8], new: &[u8], max_lines: usize) -> (Vec<DiffHunk>, u32, u32, bool) {
    let old_str = String::from_utf8_lossy(old);
    let new_str = String::from_utf8_lossy(new);
    let input = InternedInput::new(old_str.as_ref(), new_str.as_ref());
    let raw = imara_diff_fn(Algorithm::Histogram, &input, UnifiedDiffBuilder::new(&input));

    let mut hunks: Vec<DiffHunk> = Vec::new();
    let mut total_added: u32 = 0;
    let mut total_removed: u32 = 0;
    let mut total_lines: usize = 0;
    let mut truncated = false;
    let mut current_hunk: Option<DiffHunk> = None;
    let mut old_lineno: u32 = 1;
    let mut new_lineno: u32 = 1;

    for line in raw.lines() {
        if total_lines >= max_lines {
            truncated = true;
            if let Some(h) = current_hunk.take() { hunks.push(h); }
            break;
        }
        if line.starts_with("@@ ") {
            if let Some(h) = current_hunk.take() { hunks.push(h); }
            let parts: Vec<&str> = line.split_whitespace().collect();
            let (os, ol) = parse_hunk_range(parts.get(1).copied().unwrap_or("-0,0"));
            let (ns, nl) = parse_hunk_range(parts.get(2).copied().unwrap_or("+0,0"));
            old_lineno = os;
            new_lineno = ns;
            current_hunk = Some(DiffHunk { old_start: os, old_lines: ol, new_start: ns, new_lines: nl, lines: Vec::new() });
        } else if let Some(ref mut hunk) = current_hunk {
            let (kind, content, oln, nln) = if let Some(s) = line.strip_prefix('+') {
                let n = new_lineno; new_lineno += 1; total_added += 1;
                (LineKind::Added, s.to_string(), None, Some(n))
            } else if let Some(s) = line.strip_prefix('-') {
                let o = old_lineno; old_lineno += 1; total_removed += 1;
                (LineKind::Removed, s.to_string(), Some(o), None)
            } else {
                let s = line.strip_prefix(' ').unwrap_or(line);
                let o = old_lineno; let n = new_lineno; old_lineno += 1; new_lineno += 1;
                (LineKind::Context, s.to_string(), Some(o), Some(n))
            };
            hunk.lines.push(DiffLine { kind, content, old_lineno: oln, new_lineno: nln });
            total_lines += 1;
        }
    }
    if let Some(h) = current_hunk { hunks.push(h); }
    (hunks, total_added, total_removed, truncated)
}

fn parse_hunk_range(s: &str) -> (u32, u32) {
    let s = s.trim_start_matches(['+', '-']);
    if let Some((start, len)) = s.split_once(',') {
        (start.parse().unwrap_or(1), len.parse().unwrap_or(1))
    } else {
        (s.parse().unwrap_or(1), 1)
    }
}

fn run_git(project_path: &str, args: &[&str]) -> Result<(), AppError> {
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(project_path).args(args);
    no_window!(cmd);
    let out = cmd.output()
        .map_err(|e| AppError::Git(format!("failed to spawn git: {e}")))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(AppError::Git(if stderr.is_empty() {
            format!("git {} failed", args.first().unwrap_or(&""))
        } else {
            stderr
        }));
    }
    Ok(())
}

// ── Read Commands (gix) ───────────────────────────────────────────────────────

#[tauri::command]
pub fn git_status(project_path: String) -> Result<StatusResult, AppError> {
    let mut cmd = Command::new("git");
    cmd.arg("--no-optional-locks").arg("-C").arg(&project_path).args(["status", "--porcelain=v1", "-u"]);
    no_window!(cmd);
    let output = cmd.output()
        .map_err(|e| AppError::Git(format!("failed to spawn git: {e}")))?;

    if !output.status.success() {
        return Err(AppError::Git(String::from_utf8_lossy(&output.stderr).trim().to_string()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut files: Vec<FileStatus> = Vec::with_capacity(STATUS_LIMIT.min(256));
    let mut total = 0usize;

    for line in stdout.lines() {
        if line.len() < 3 { continue; }
        total += 1;

        // Count all lines but only parse the first STATUS_LIMIT — keeps the
        // IPC payload small even when tens of thousands of files are changed.
        if files.len() >= STATUS_LIMIT { continue; }

        let x = line.as_bytes()[0] as char;
        let y = line.as_bytes()[1] as char;
        let path_raw = &line[3..];

        let path_unquoted = if path_raw.starts_with('"') && path_raw.ends_with('"') {
            &path_raw[1..path_raw.len() - 1]
        } else {
            path_raw
        };

        let (path, rename_from) = if let Some(idx) = path_unquoted.find(" -> ") {
            let from = path_unquoted[..idx].trim_matches('"').to_string();
            let to   = path_unquoted[idx + 4..].trim_matches('"').to_string();
            (to, Some(from))
        } else {
            (path_unquoted.to_string(), None)
        };

        let file_kind = classify_path(&path);

        let conflicted = matches!(
            (x, y),
            ('U','U') | ('A','A') | ('D','D') |
            ('A','U') | ('U','A') | ('D','U') | ('U','D')
        );

        if conflicted {
            files.push(FileStatus {
                path,
                index_status: None,
                worktree_status: Some(ChangeKind::Modified),
                file_kind,
                conflicted: true,
            });
            continue;
        }

        let index_status = match x {
            'A' => Some(ChangeKind::Added),
            'M' => Some(ChangeKind::Modified),
            'D' => Some(ChangeKind::Deleted),
            'R' | 'C' => Some(ChangeKind::Renamed { from: rename_from.clone().unwrap_or_default() }),
            _ => None,
        };

        let worktree_status = match y {
            'M' => Some(ChangeKind::Modified),
            'D' => Some(ChangeKind::Deleted),
            '?' => Some(ChangeKind::Added),
            _ => None,
        };

        if index_status.is_some() || worktree_status.is_some() {
            files.push(FileStatus { path, index_status, worktree_status, file_kind, conflicted: false });
        }
    }

    Ok(StatusResult { files, total })
}

#[tauri::command]
pub fn git_diff_file(
    project_path: String,
    rel_path: String,
    staged: bool,
    max_lines: Option<usize>,
) -> Result<DiffResult, AppError> {
    let max = max_lines.unwrap_or(10_000);

    // Use git CLI to read blob content — avoids gix open_index() which fails
    // on Windows for repos with large or locked indexes.
    let git_show = |spec: &str| -> Vec<u8> {
        let mut cmd = Command::new("git");
        cmd.arg("-C").arg(&project_path).args(["show", spec]);
        no_window!(cmd);
        cmd.output().map(|o| if o.status.success() { o.stdout } else { vec![] }).unwrap_or_default()
    };

    let (old_bytes, new_bytes): (Vec<u8>, Vec<u8>) = if staged {
        // staged diff: HEAD vs index
        let head_spec = format!("HEAD:{rel_path}");
        let idx_spec  = format!(":{rel_path}");
        (git_show(&head_spec), git_show(&idx_spec))
    } else {
        // unstaged diff: index vs worktree
        let idx_spec = format!(":{rel_path}");
        let old = git_show(&idx_spec);
        let new = std::fs::read(Path::new(&project_path).join(&rel_path)).unwrap_or_default();
        (old, new)
    };

    let ext = Path::new(&rel_path).extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase()).unwrap_or_default();
    let is_image = IMAGE_EXTS.contains(&ext.as_str());
    let is_binary = !is_image && (sniff_binary(&old_bytes) || sniff_binary(&new_bytes));

    if is_image || is_binary {
        return Ok(DiffResult { hunks: vec![], truncated: false, total_added: 0, total_removed: 0, is_binary, is_image });
    }

    let (hunks, total_added, total_removed, truncated) = diff_bytes(&old_bytes, &new_bytes, max);
    Ok(DiffResult { hunks, truncated, total_added, total_removed, is_binary: false, is_image: false })
}

#[tauri::command]
pub fn git_log(project_path: String, limit: Option<usize>) -> Result<Vec<CommitInfo>, AppError> {
    let limit = limit.unwrap_or(200);
    let repo = open_repo(&project_path)?;
    let head_id = repo.head_id().map_err(|e| AppError::Git(e.to_string()))?;

    let walk = repo
        .rev_walk([head_id])
        .sorting(gix::revision::walk::Sorting::BreadthFirst)
        .all()
        .map_err(|e| AppError::Git(e.to_string()))?;

    let mut result = Vec::with_capacity(limit.min(64));
    for info in walk.take(limit) {
        let info = info.map_err(|e| AppError::Git(e.to_string()))?;
        let commit = repo.find_commit(info.id).map_err(|e| AppError::Git(e.to_string()))?;
        let msg = commit.message().map_err(|e| AppError::Git(e.to_string()))?;
        let author = commit.author().map_err(|e| AppError::Git(e.to_string()))?;
        let parents: Vec<String> = commit.parent_ids().map(|id| id.to_string()).collect();
        result.push(CommitInfo {
            hash: info.id.to_string(),
            short_hash: info.id.to_string().chars().take(8).collect(),
            summary: msg.summary().to_str_lossy().to_string(),
            body: {
                let raw = String::from_utf8_lossy(commit.message_raw().unwrap_or_default());
                raw.split_once("\n\n").map(|(_, b)| b.trim().to_string()).filter(|s| !s.is_empty())
            },
            author_name: author.name.to_string(),
            author_email: author.email.to_string(),
            timestamp: author.time.seconds,
            parents,
        });
    }
    Ok(result)
}

#[tauri::command]
pub async fn git_branches(project_path: String) -> Result<BranchList, AppError> {
    // Run local-branch listing and remote-branch listing in parallel (no gix open).
    let mut local_cmd = tokio::process::Command::new("git");
    local_cmd.arg("--no-optional-locks").arg("-C").arg(&project_path)
        .args(["for-each-ref", "--format=%(HEAD)\t%(refname:short)", "refs/heads/"]);
    no_window!(local_cmd);
    let mut remote_cmd = tokio::process::Command::new("git");
    remote_cmd.arg("--no-optional-locks").arg("-C").arg(&project_path)
        .args(["for-each-ref", "--format=%(refname:short)", "refs/remotes/"]);
    no_window!(remote_cmd);
    let local_fut = local_cmd.output();
    let remote_fut = remote_cmd.output();

    let (local_res, remote_res) = tokio::join!(local_fut, remote_fut);
    let local_out = local_res.map_err(|e| AppError::Git(format!("failed to spawn git: {e}")))?;

    let mut current = String::new();
    let mut is_detached = false;
    let mut branches: Vec<BranchInfo> = Vec::new();

    if local_out.status.success() {
        for line in String::from_utf8_lossy(&local_out.stdout).lines() {
            let mut parts = line.splitn(2, '\t');
            let head_marker = parts.next().unwrap_or("");
            let name = parts.next().unwrap_or("").trim().to_string();
            if name.is_empty() { continue; }
            if head_marker == "*" { current = name.clone(); }
            branches.push(BranchInfo { name, is_remote: false });
        }
    }

    // Detached HEAD — current is still empty
    if current.is_empty() {
        let mut hcmd = tokio::process::Command::new("git");
        hcmd.arg("--no-optional-locks").arg("-C").arg(&project_path).args(["rev-parse", "--short", "HEAD"]);
        no_window!(hcmd);
        if let Ok(out) = hcmd.output().await
        {
            if out.status.success() {
                current = String::from_utf8_lossy(&out.stdout).trim().to_string();
                is_detached = true;
            }
        }
    }

    if let Ok(remote_out) = remote_res {
        if remote_out.status.success() {
            for name in String::from_utf8_lossy(&remote_out.stdout).lines() {
                let name = name.trim().to_string();
                if name.is_empty() || name.ends_with("/HEAD") { continue; }
                branches.push(BranchInfo { name, is_remote: true });
            }
        }
    }

    Ok(BranchList { current, is_detached, branches })
}

#[tauri::command]
pub async fn git_remote_status(project_path: String) -> Result<RemoteStatus, AppError> {
    // Two parallel calls: branch→remote config, and ahead/behind commit count.
    // Using git CLI avoids gix rev_walk (expensive pack-file traversal on Windows).
    let mut config_cmd = tokio::process::Command::new("git");
    config_cmd.arg("--no-optional-locks").arg("-C").arg(&project_path)
        .args(["for-each-ref", "--format=%(HEAD)\t%(upstream:remotename)\t%(upstream:short)", "refs/heads/"]);
    no_window!(config_cmd);
    let mut ab_cmd = tokio::process::Command::new("git");
    ab_cmd.arg("--no-optional-locks").arg("-C").arg(&project_path)
        .args(["rev-list", "--left-right", "--count", "@{u}...HEAD"]);
    no_window!(ab_cmd);
    let config_fut = config_cmd.output();
    let ab_fut = ab_cmd.output();

    let (config_res, ab_res) = tokio::join!(config_fut, ab_fut);

    let mut remote_name: Option<String> = None;
    let mut remote_branch: Option<String> = None;

    if let Ok(out) = config_res {
        if out.status.success() {
            for line in String::from_utf8_lossy(&out.stdout).lines() {
                let mut parts = line.splitn(3, '\t');
                let head_marker = parts.next().unwrap_or("");
                let rname = parts.next().unwrap_or("").trim().to_string();
                let rshort = parts.next().unwrap_or("").trim().to_string();
                if head_marker == "*" {
                    if !rname.is_empty() {
                        // rshort is "origin/main" — strip the "origin/" prefix
                        let branch = if rshort.starts_with(&format!("{rname}/")) {
                            rshort[rname.len() + 1..].to_string()
                        } else {
                            rshort
                        };
                        remote_name = Some(rname);
                        if !branch.is_empty() { remote_branch = Some(branch); }
                    }
                    break;
                }
            }
        }
    }

    // `git rev-list --left-right --count @{u}...HEAD` outputs "behind\tahead"
    let (ahead, behind) = if let Ok(out) = ab_res {
        if out.status.success() {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let mut parts = stdout.trim().split('\t');
            let behind: usize = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
            let ahead: usize = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
            (ahead, behind)
        } else {
            (0, 0) // no upstream configured — not an error
        }
    } else {
        (0, 0)
    };

    Ok(RemoteStatus { ahead, behind, remote_name, remote_branch })
}

#[tauri::command]
pub fn git_read_blob_worktree(project_path: String, rel_path: String) -> Result<ImageBlob, AppError> {
    let repo = open_repo(&project_path)?;
    let workdir = repo.work_dir().ok_or_else(|| AppError::Git("bare repo".into()))?.to_path_buf();
    let data = std::fs::read(workdir.join(&rel_path)).map_err(AppError::Io)?;
    let ext = Path::new(&rel_path).extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase()).unwrap_or_default();
    Ok(ImageBlob { data: B64.encode(&data), mime: mime_for_ext(&ext).into() })
}

#[tauri::command]
pub fn git_read_blob_head(project_path: String, rel_path: String) -> Result<ImageBlob, AppError> {
    let repo = open_repo(&project_path)?;
    let commit = repo.head_commit().map_err(|e| AppError::Git(e.to_string()))?;
    let tree = commit.tree().map_err(|e| AppError::Git(e.to_string()))?;
    let mut m = BTreeMap::new();
    collect_tree_entries(&repo, &tree, "", &mut m)?;
    let oid = m.get(&rel_path).copied().ok_or_else(|| AppError::NotFound(rel_path.clone()))?;
    let data = repo.find_object(oid).map_err(|e| AppError::Git(e.to_string()))?.data.to_vec();
    let ext = Path::new(&rel_path).extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase()).unwrap_or_default();
    Ok(ImageBlob { data: B64.encode(&data), mime: mime_for_ext(&ext).into() })
}

#[tauri::command]
pub fn git_read_blob_at_commit(project_path: String, commit_hash: String, rel_path: String) -> Result<ImageBlob, AppError> {
    let spec = format!("{commit_hash}:{rel_path}");
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(&project_path).args(["show", &spec]);
    no_window!(cmd);
    let output = cmd.output().map_err(AppError::Io)?;
    if !output.status.success() {
        return Err(AppError::Git(String::from_utf8_lossy(&output.stderr).trim().to_string()));
    }
    let ext = Path::new(&rel_path).extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase()).unwrap_or_default();
    Ok(ImageBlob { data: B64.encode(&output.stdout), mime: mime_for_ext(&ext).into() })
}

// ── Mutating Commands (git CLI) ───────────────────────────────────────────────
// gix's index/commit mutation API is still maturing; git CLI handles all edge
// cases (modes, symlinks, submodules, hooks) correctly and is universally available.

#[tauri::command]
pub fn git_stage_file(project_path: String, rel_path: String) -> Result<(), AppError> {
    run_git(&project_path, &["add", "--", &rel_path])
}

#[tauri::command]
pub fn git_unstage_file(project_path: String, rel_path: String) -> Result<(), AppError> {
    // `git restore --staged` is available since git 2.23
    let r = run_git(&project_path, &["restore", "--staged", "--", &rel_path]);
    if r.is_err() {
        // Fallback for older git: reset HEAD
        run_git(&project_path, &["reset", "HEAD", "--", &rel_path])
    } else {
        r
    }
}

#[tauri::command]
pub fn git_stage_all(project_path: String) -> Result<(), AppError> {
    run_git(&project_path, &["add", "-A"])
}

#[tauri::command]
pub fn git_unstage_all(project_path: String) -> Result<(), AppError> {
    run_git(&project_path, &["restore", "--staged", "."])
}

#[tauri::command]
pub fn git_commit(project_path: String, summary: String, body: Option<String>) -> Result<String, AppError> {
    let message = match body.as_deref() {
        Some(b) if !b.trim().is_empty() => format!("{}\n\n{}", summary.trim(), b.trim()),
        _ => summary.trim().to_string(),
    };
    run_git(&project_path, &["commit", "-m", &message])?;
    // Return the new HEAD hash
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(&project_path).args(["rev-parse", "HEAD"]);
    no_window!(cmd);
    let out = cmd.output().map_err(|e| AppError::Git(e.to_string()))?;
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

#[tauri::command]
pub fn git_checkout_branch(project_path: String, branch: String) -> Result<(), AppError> {
    run_git(&project_path, &["checkout", &branch])
}

#[tauri::command]
pub fn git_create_branch(project_path: String, name: String, base: Option<String>) -> Result<(), AppError> {
    match base.as_deref() {
        Some(b) => run_git(&project_path, &["checkout", "-b", &name, b]),
        None => run_git(&project_path, &["checkout", "-b", &name]),
    }
}

#[tauri::command]
pub fn git_fetch(project_path: String) -> Result<(), AppError> {
    run_git(&project_path, &["fetch", "--all", "--prune"])
}

#[tauri::command]
pub fn git_pull(project_path: String) -> Result<(), AppError> {
    // Regular merge pull — safe, preserves all commits, shows merge commits in history.
    // --ff-only was too strict: it rejected any divergent branch, leaving the user stuck.
    run_git(&project_path, &["pull"])
}

#[tauri::command]
pub fn git_merge_abort(project_path: String) -> Result<(), AppError> {
    run_git(&project_path, &["merge", "--abort"])
}

#[tauri::command]
pub fn git_stash(project_path: String) -> Result<(), AppError> {
    run_git(&project_path, &["stash", "push", "--include-untracked", "-m", "anide: auto-stash"])
}

/// Checkout carrying uncommitted changes to the new branch (git checkout <branch> -- keeps changes).
#[tauri::command]
pub fn git_checkout_force(project_path: String, branch: String) -> Result<(), AppError> {
    run_git(&project_path, &["checkout", "-m", &branch])
}

#[tauri::command]
pub fn git_push(project_path: String, force: Option<bool>) -> Result<(), AppError> {
    if force.unwrap_or(false) {
        run_git(&project_path, &["push", "--force-with-lease"])
    } else {
        run_git(&project_path, &["push"])
    }
}

#[tauri::command]
pub fn git_publish_branch(project_path: String, branch: String) -> Result<(), AppError> {
    run_git(&project_path, &["push", "--set-upstream", "origin", &branch])
}

/// Delete a local branch. Use force=true for branches not fully merged.
#[tauri::command]
pub fn git_delete_branch(project_path: String, branch: String, force: Option<bool>) -> Result<(), AppError> {
    let flag = if force.unwrap_or(false) { "-D" } else { "-d" };
    run_git(&project_path, &["branch", flag, &branch])
}

/// Returns the list of files changed in a commit (vs its first parent).
/// Uses `git diff <parent> <hash>` which correctly handles merge commits.
#[tauri::command]
pub fn git_commit_files(project_path: String, hash: String) -> Result<Vec<FileStatus>, AppError> {
    // Resolve the first parent; fall back to the empty-tree object for initial commits.
    const EMPTY_TREE: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";
    let mut pcmd = Command::new("git");
    pcmd.arg("-C").arg(&project_path).args(["log", "--format=%P", "-n", "1", &hash]);
    no_window!(pcmd);
    let parent_out = pcmd.output().map_err(|e| AppError::Git(format!("failed to spawn git: {e}")))?;
    let parents_str = String::from_utf8_lossy(&parent_out.stdout);
    let first_parent: &str = parents_str.split_whitespace().next().unwrap_or(EMPTY_TREE);

    let mut dcmd = Command::new("git");
    dcmd.arg("-C").arg(&project_path).args(["diff", first_parent, &hash, "--name-status"]);
    no_window!(dcmd);
    let output = dcmd.output().map_err(|e| AppError::Git(format!("failed to spawn git: {e}")))?;

    if !output.status.success() {
        return Err(AppError::Git(String::from_utf8_lossy(&output.stderr).trim().to_string()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut files = Vec::new();

    for line in stdout.lines() {
        if line.is_empty() { continue; }
        let parts: Vec<&str> = line.splitn(3, '\t').collect();
        let status_char = parts.first().and_then(|s| s.chars().next()).unwrap_or(' ');
        let (path, kind) = match status_char {
            'A' => (parts.get(1).map(|s| s.to_string()).unwrap_or_default(), ChangeKind::Added),
            'M' => (parts.get(1).map(|s| s.to_string()).unwrap_or_default(), ChangeKind::Modified),
            'D' => (parts.get(1).map(|s| s.to_string()).unwrap_or_default(), ChangeKind::Deleted),
            'R' | 'C' => {
                let new = parts.get(2).map(|s| s.to_string()).unwrap_or_default();
                let old = parts.get(1).map(|s| s.to_string()).unwrap_or_default();
                (new, ChangeKind::Renamed { from: old })
            }
            _ => continue,
        };
        let file_kind = classify_path(&path);
        files.push(FileStatus { path, index_status: Some(kind), worktree_status: None, file_kind, conflicted: false });
    }
    Ok(files)
}

/// Diffs a single file between two commits (or from empty tree for initial commits).
#[tauri::command]
pub fn git_diff_commit_file(
    project_path: String,
    hash: String,
    rel_path: String,
    parent_hash: Option<String>,
    max_lines: Option<usize>,
) -> Result<DiffResult, AppError> {
    let max = max_lines.unwrap_or(10_000);

    let read_blob = |spec: String| -> Vec<u8> {
        let mut cmd = Command::new("git");
        cmd.arg("-C").arg(&project_path).args(["show", &spec]);
        no_window!(cmd);
        cmd.output().ok().filter(|o| o.status.success()).map(|o| o.stdout).unwrap_or_default()
    };

    let new_bytes = read_blob(format!("{hash}:{rel_path}"));
    let old_bytes = match parent_hash.as_deref() {
        Some(p) => read_blob(format!("{p}:{rel_path}")),
        None => vec![],
    };

    let ext = Path::new(&rel_path).extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase()).unwrap_or_default();
    let is_image = IMAGE_EXTS.contains(&ext.as_str());
    let is_binary = !is_image && (sniff_binary(&old_bytes) || sniff_binary(&new_bytes));

    if is_image || is_binary {
        return Ok(DiffResult { hunks: vec![], truncated: false, total_added: 0, total_removed: 0, is_binary, is_image });
    }

    let (hunks, total_added, total_removed, truncated) = diff_bytes(&old_bytes, &new_bytes, max);
    Ok(DiffResult { hunks, truncated, total_added, total_removed, is_binary: false, is_image: false })
}

/// Open a file with the OS default application.
#[tauri::command]
pub fn open_file_default(app: AppHandle, path: String) -> Result<(), AppError> {
    app.opener()
        .open_path(&path, None::<&str>)
        .map_err(|e| AppError::Other(e.to_string()))
}

/// Discard ALL changes — restores every tracked file (staged + worktree) to HEAD.
#[tauri::command]
pub fn git_discard_all(project_path: String) -> Result<(), AppError> {
    run_git(
        &project_path,
        &["restore", "--source=HEAD", "--staged", "--worktree", "."],
    )
}

/// Discard changes to a file, restoring it to HEAD.
/// For tracked files: restores staged + worktree from HEAD.
/// For newly staged files not in HEAD: just unstages them.
#[tauri::command]
pub fn git_discard_file(project_path: String, rel_path: String) -> Result<(), AppError> {
    if run_git(
        &project_path,
        &["restore", "--source=HEAD", "--staged", "--worktree", "--", &rel_path],
    )
    .is_ok()
    {
        return Ok(());
    }
    if run_git(&project_path, &["restore", "--staged", "--", &rel_path]).is_ok() {
        return Ok(());
    }
    // Untracked / new files that git restore can't touch — delete from disk
    let full = Path::new(&project_path).join(&rel_path);
    if full.is_dir() {
        fs::remove_dir_all(&full).map_err(|e| AppError::Other(e.to_string()))
    } else {
        fs::remove_file(&full).map_err(|e| AppError::Other(e.to_string()))
    }
}

/// Append a pattern to the project's .gitignore, creating it if needed.
/// No-ops if the pattern is already present.
#[tauri::command]
pub fn git_add_to_gitignore(project_path: String, pattern: String) -> Result<(), AppError> {
    let gitignore = Path::new(&project_path).join(".gitignore");
    let existing = if gitignore.exists() {
        fs::read_to_string(&gitignore).map_err(|e| AppError::Other(e.to_string()))?
    } else {
        String::new()
    };
    let pattern = pattern.trim();
    if existing.lines().any(|l| l.trim() == pattern) {
        return Ok(());
    }
    let new_content = if existing.is_empty() || existing.ends_with('\n') {
        format!("{}{}\n", existing, pattern)
    } else {
        format!("{}\n{}\n", existing, pattern)
    };
    fs::write(gitignore, new_content).map_err(|e| AppError::Other(e.to_string()))
}
