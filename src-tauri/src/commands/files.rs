use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use crate::error::AppError;
use crate::utils::scanner;

// ── File tree ──────────────────────────────────────────────────────────────

#[derive(serde::Serialize)]
pub struct FileEntry {
    pub path: String,
    pub is_dir: bool,
}

const EXPLORER_SKIP: &[&str] = &[
    "node_modules", "target", ".git", "dist", "build",
    ".svelte-kit", ".next", ".nuxt", ".turbo", ".output",
    "__pycache__", ".venv", "venv", ".tox",
];

fn walk_dir(root: &Path, dir: &Path, out: &mut Vec<FileEntry>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut items: Vec<_> = rd.flatten().collect();
    // Dirs first, then files; each group alphabetical
    items.sort_by(|a, b| {
        let a_dir = a.file_type().map_or(false, |t| t.is_dir());
        let b_dir = b.file_type().map_or(false, |t| t.is_dir());
        b_dir.cmp(&a_dir).then_with(|| a.file_name().cmp(&b.file_name()))
    });
    for item in items {
        let Ok(ft) = item.file_type() else { continue };
        let is_dir = ft.is_dir();
        let name = item.file_name().to_string_lossy().to_string();
        if is_dir && EXPLORER_SKIP.contains(&name.as_str()) { continue; }
        let path = item.path();
        let Ok(rel) = path.strip_prefix(root) else { continue };
        let rel = rel.to_string_lossy().replace('\\', "/");
        out.push(FileEntry { path: rel, is_dir });
        if is_dir { walk_dir(root, &path, out); }
    }
}

// ── Mutating file-system operations ───────────────────────────────────────

#[tauri::command]
pub fn create_project_file(project_path: String, rel_path: String) -> Result<(), AppError> {
    let path = resolve_write_in_project(&project_path, &rel_path)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(AppError::Io)?;
    }
    std::fs::OpenOptions::new()
        .write(true).create_new(true).open(&path)
        .map(|_| ())
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::AlreadyExists {
                AppError::AlreadyExists(rel_path)
            } else {
                AppError::Io(e)
            }
        })
}

#[tauri::command]
pub fn create_project_dir(project_path: String, rel_path: String) -> Result<(), AppError> {
    let root = PathBuf::from(&project_path);
    let canonical_root = root.canonicalize()?;
    let full = root.join(&rel_path);
    let parent = full.parent().ok_or_else(|| AppError::InvalidPath(rel_path.clone()))?;
    let canonical_parent = parent.canonicalize()
        .map_err(|_| AppError::InvalidPath(rel_path.clone()))?;
    if !canonical_parent.starts_with(&canonical_root) {
        return Err(AppError::InvalidPath(rel_path));
    }
    std::fs::create_dir(&full).map_err(|e| {
        if e.kind() == std::io::ErrorKind::AlreadyExists {
            AppError::AlreadyExists(rel_path)
        } else {
            AppError::Io(e)
        }
    })
}

#[tauri::command]
pub fn delete_project_path(project_path: String, rel_path: String) -> Result<(), AppError> {
    let root = PathBuf::from(&project_path);
    let canonical_root = root.canonicalize()?;
    let full = root.join(&rel_path);
    let canonical = full.canonicalize()
        .map_err(|_| AppError::NotFound(rel_path.clone()))?;
    if !canonical.starts_with(&canonical_root) || canonical == canonical_root {
        return Err(AppError::InvalidPath(rel_path));
    }
    if canonical.is_dir() {
        std::fs::remove_dir_all(&canonical).map_err(AppError::Io)
    } else {
        std::fs::remove_file(&canonical).map_err(AppError::Io)
    }
}

#[tauri::command]
pub fn rename_project_path(
    project_path: String,
    old_rel: String,
    new_rel: String,
) -> Result<(), AppError> {
    let root = PathBuf::from(&project_path);
    let canonical_root = root.canonicalize()?;

    let old_full = root.join(&old_rel);
    let canonical_old = old_full.canonicalize()
        .map_err(|_| AppError::NotFound(old_rel.clone()))?;
    if !canonical_old.starts_with(&canonical_root) {
        return Err(AppError::InvalidPath(old_rel));
    }

    let new_full = root.join(&new_rel);
    let new_parent = new_full.parent()
        .ok_or_else(|| AppError::InvalidPath(new_rel.clone()))?;
    let canonical_new_parent = new_parent.canonicalize()
        .map_err(|_| AppError::InvalidPath(new_rel.clone()))?;
    if !canonical_new_parent.starts_with(&canonical_root) {
        return Err(AppError::InvalidPath(new_rel));
    }

    std::fs::rename(&canonical_old, &new_full).map_err(AppError::Io)
}

#[tauri::command]
pub fn read_project_file_b64(project_path: String, rel_path: String) -> Result<String, AppError> {
    let path = resolve_in_project(&project_path, &rel_path)?;
    let bytes = std::fs::read(&path).map_err(AppError::Io)?;
    use base64::Engine as _;
    Ok(base64::engine::general_purpose::STANDARD.encode(&bytes))
}

#[tauri::command]
pub fn list_project_tree(project_path: String) -> Result<Vec<FileEntry>, AppError> {
    let root = PathBuf::from(&project_path);
    if !root.is_dir() { return Err(AppError::InvalidPath(project_path)); }
    let mut entries = Vec::new();
    walk_dir(&root, &root, &mut entries);
    Ok(entries)
}

fn resolve_in_project(project_path: &str, rel_path: &str) -> Result<PathBuf, AppError> {
    let root = PathBuf::from(project_path);
    let canonical_root = root.canonicalize()?;

    // For reads the file must exist; canonicalize validates that
    let full = root.join(rel_path);
    let canonical_full = full
        .canonicalize()
        .map_err(|_| AppError::NotFound(rel_path.to_string()))?;

    if !canonical_full.starts_with(&canonical_root) {
        return Err(AppError::InvalidPath(rel_path.to_string()));
    }
    Ok(canonical_full)
}

fn resolve_write_in_project(project_path: &str, rel_path: &str) -> Result<PathBuf, AppError> {
    let root = PathBuf::from(project_path);
    let canonical_root = root.canonicalize()?;
    let full = root.join(rel_path);

    // The file may not exist yet; validate via the parent directory
    let parent = full
        .parent()
        .ok_or_else(|| AppError::InvalidPath(rel_path.to_string()))?;
    let canonical_parent = parent
        .canonicalize()
        .map_err(|_| AppError::InvalidPath(rel_path.to_string()))?;

    if !canonical_parent.starts_with(&canonical_root) {
        return Err(AppError::InvalidPath(rel_path.to_string()));
    }
    Ok(full)
}

#[tauri::command]
pub fn read_project_file(project_path: String, rel_path: String) -> Result<String, AppError> {
    let path = resolve_in_project(&project_path, &rel_path)?;
    std::fs::read_to_string(&path).map_err(AppError::Io)
}

#[tauri::command]
pub fn write_project_file(
    project_path: String,
    rel_path: String,
    content: String,
) -> Result<(), AppError> {
    let path = resolve_write_in_project(&project_path, &rel_path)?;
    std::fs::write(&path, content.as_bytes()).map_err(AppError::Io)
}

/// Returns directories and .md/.excalidraw files in the project (gitignore-aware).
/// Directories are included even if empty, so the user can see folders they created.
/// Skips .anide/, .takerest/, and the project root itself.
#[tauri::command]
pub fn list_doc_files(project_path: String) -> Result<Vec<FileEntry>, AppError> {
    let root = PathBuf::from(&project_path);
    if !root.is_dir() {
        return Err(AppError::InvalidPath(project_path));
    }

    let skip_roots: &[&OsStr] = &[
        OsStr::new(".anide"),
        OsStr::new(".takerest"),
    ];

    let mut entries: Vec<FileEntry> = Vec::new();

    for entry in scanner::walk_project(&project_path) {
        let Ok(entry) = entry else { continue };
        // Skip the root itself
        if entry.depth() == 0 { continue; }

        let rel_path = match entry.path().strip_prefix(&root) {
            Ok(p) => p.to_string_lossy().replace('\\', "/"),
            Err(_) => continue,
        };

        // Skip hidden metadata folders
        if std::path::Path::new(&rel_path)
            .components()
            .next()
            .map(|c| skip_roots.contains(&c.as_os_str()))
            .unwrap_or(false)
        {
            continue;
        }

        let is_dir = entry.file_type().map_or(false, |ft| ft.is_dir());
        if is_dir {
            entries.push(FileEntry { path: rel_path, is_dir: true });
        } else {
            let ext = entry.path().extension().and_then(|e| e.to_str()).unwrap_or("");
            if ext == "md" || ext == "excalidraw" {
                entries.push(FileEntry { path: rel_path, is_dir: false });
            }
        }
    }

    entries.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(entries)
}

#[tauri::command]
pub fn delete_doc_file(project_path: String, rel_path: String) -> Result<(), AppError> {
    let path = resolve_in_project(&project_path, &rel_path)?;
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    if ext != "md" && ext != "excalidraw" {
        return Err(AppError::Other(
            "Only .md and .excalidraw files can be deleted via this command".to_string(),
        ));
    }
    std::fs::remove_file(&path).map_err(AppError::Io)
}
