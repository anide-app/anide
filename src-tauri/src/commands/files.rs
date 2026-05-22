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

/// Returns relative paths (forward-slash) of all .md and .excalidraw files in the project.
/// Respects .gitignore and skips .anide/, .git/, node_modules/, etc.
#[tauri::command]
pub fn list_doc_files(project_path: String) -> Result<Vec<String>, AppError> {
    let root = PathBuf::from(&project_path);
    if !root.is_dir() {
        return Err(AppError::InvalidPath(project_path));
    }

    let mut files: Vec<String> = Vec::new();

    for entry in scanner::walk_project(&project_path) {
        let Ok(entry) = entry else { continue };
        if entry.file_type().map_or(true, |ft| ft.is_dir()) { continue; }

        let rel_path = match entry.path().strip_prefix(&root) {
            Ok(p) => p.to_string_lossy().replace('\\', "/"),
            Err(_) => continue,
        };

        // Skip .anide/ folder
        if std::path::Path::new(&rel_path)
            .components()
            .next()
            .map(|c| c.as_os_str() == OsStr::new(".anide"))
            .unwrap_or(false)
        {
            continue;
        }

        let ext = entry.path().extension().and_then(|e| e.to_str()).unwrap_or("");
        if ext == "md" || ext == "excalidraw" {
            files.push(rel_path);
        }
    }

    files.sort();
    Ok(files)
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    /// Create a temporary directory with a unique name for each test.
    fn tmp_dir(suffix: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("anide_test_{suffix}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    // ── walk_dir ─────────────────────────────────────────────────────────────

    #[test]
    fn test_walk_dir_dirs_appear_before_files() {
        let root = tmp_dir("walk_order");
        fs::write(root.join("aaa.txt"), "").unwrap();
        fs::create_dir(root.join("bbb")).unwrap();
        fs::write(root.join("ccc.txt"), "").unwrap();

        let mut out = Vec::new();
        walk_dir(&root, &root, &mut out);

        // "bbb" directory must come before any file entries
        let dir_pos  = out.iter().position(|e| e.path == "bbb").unwrap();
        let file_pos = out.iter().position(|e| e.path == "aaa.txt").unwrap();
        assert!(dir_pos < file_pos, "directories must precede files in walk output");
    }

    #[test]
    fn test_walk_dir_alphabetical_within_group() {
        let root = tmp_dir("walk_alpha");
        fs::write(root.join("z.txt"), "").unwrap();
        fs::write(root.join("a.txt"), "").unwrap();
        fs::write(root.join("m.txt"), "").unwrap();

        let mut out = Vec::new();
        walk_dir(&root, &root, &mut out);

        let names: Vec<&str> = out.iter().map(|e| e.path.as_str()).collect();
        assert_eq!(names, vec!["a.txt", "m.txt", "z.txt"]);
    }

    #[test]
    fn test_walk_dir_skips_node_modules() {
        let root = tmp_dir("walk_skip_nm");
        fs::create_dir(root.join("node_modules")).unwrap();
        fs::write(root.join("node_modules").join("pkg.js"), "").unwrap();
        fs::write(root.join("index.js"), "").unwrap();

        let mut out = Vec::new();
        walk_dir(&root, &root, &mut out);

        let paths: Vec<&str> = out.iter().map(|e| e.path.as_str()).collect();
        assert!(!paths.contains(&"node_modules"), "node_modules dir must be skipped");
        assert!(paths.contains(&"index.js"), "regular files must still appear");
    }

    #[test]
    fn test_walk_dir_skips_all_explorer_skip_dirs() {
        let root = tmp_dir("walk_skip_all");
        for skip in EXPLORER_SKIP {
            fs::create_dir(root.join(skip)).unwrap();
        }
        fs::write(root.join("keep.txt"), "").unwrap();

        let mut out = Vec::new();
        walk_dir(&root, &root, &mut out);

        for skip in EXPLORER_SKIP {
            assert!(
                !out.iter().any(|e| e.path == *skip),
                "EXPLORER_SKIP dir '{}' must not appear in walk output",
                skip
            );
        }
        assert!(out.iter().any(|e| e.path == "keep.txt"));
    }

    #[test]
    fn test_walk_dir_recursive_nested_dirs() {
        let root = tmp_dir("walk_nested");
        fs::create_dir_all(root.join("a").join("b")).unwrap();
        fs::write(root.join("a").join("b").join("deep.txt"), "").unwrap();
        fs::write(root.join("top.txt"), "").unwrap();

        let mut out = Vec::new();
        walk_dir(&root, &root, &mut out);

        let paths: Vec<&str> = out.iter().map(|e| e.path.as_str()).collect();
        assert!(paths.contains(&"a"), "parent dir must be listed");
        assert!(paths.contains(&"a/b"), "nested dir must be listed");
        assert!(paths.contains(&"a/b/deep.txt"), "deeply nested file must be listed");
        assert!(paths.contains(&"top.txt"));
    }

    #[test]
    fn test_walk_dir_uses_forward_slashes() {
        let root = tmp_dir("walk_slashes");
        fs::create_dir(root.join("sub")).unwrap();
        fs::write(root.join("sub").join("file.txt"), "").unwrap();

        let mut out = Vec::new();
        walk_dir(&root, &root, &mut out);

        let file_entry = out.iter().find(|e| !e.is_dir).unwrap();
        assert!(!file_entry.path.contains('\\'), "paths must use forward slashes");
    }

    #[test]
    fn test_walk_dir_empty_directory() {
        let root = tmp_dir("walk_empty");
        let mut out = Vec::new();
        walk_dir(&root, &root, &mut out);
        assert!(out.is_empty());
    }

    // ── create_project_file ───────────────────────────────────────────────────

    #[test]
    fn test_create_project_file_success() {
        let root = tmp_dir("cpf_ok");
        let result = create_project_file(
            root.to_str().unwrap().to_string(),
            "new_file.txt".to_string(),
        );
        assert!(result.is_ok());
        assert!(root.join("new_file.txt").exists());
    }

    #[test]
    fn test_create_project_file_creates_intermediate_dirs() {
        let root = tmp_dir("cpf_parents");
        let result = create_project_file(
            root.to_str().unwrap().to_string(),
            "sub/dir/file.txt".to_string(),
        );
        assert!(result.is_ok(), "should succeed even when parent dirs do not exist");
        assert!(root.join("sub/dir/file.txt").exists());
    }

    #[test]
    fn test_create_project_file_already_exists() {
        let root = tmp_dir("cpf_exists");
        fs::write(root.join("existing.txt"), "hello").unwrap();

        let result = create_project_file(
            root.to_str().unwrap().to_string(),
            "existing.txt".to_string(),
        );
        assert!(result.is_err());
        match result.unwrap_err() {
            AppError::AlreadyExists(p) => assert_eq!(p, "existing.txt"),
            e => panic!("expected AlreadyExists, got {:?}", e),
        }
    }

    #[test]
    fn test_create_project_file_traversal_rejected() {
        let root = tmp_dir("cpf_traversal");
        let result = create_project_file(
            root.to_str().unwrap().to_string(),
            "../escape.txt".to_string(),
        );
        assert!(result.is_err(), "path traversal must be rejected");
    }

    // ── create_project_dir ────────────────────────────────────────────────────

    #[test]
    fn test_create_project_dir_success() {
        let root = tmp_dir("cpd_ok");
        let result = create_project_dir(
            root.to_str().unwrap().to_string(),
            "new_folder".to_string(),
        );
        assert!(result.is_ok());
        assert!(root.join("new_folder").is_dir());
    }

    #[test]
    fn test_create_project_dir_already_exists() {
        let root = tmp_dir("cpd_exists");
        fs::create_dir(root.join("existing_dir")).unwrap();

        let result = create_project_dir(
            root.to_str().unwrap().to_string(),
            "existing_dir".to_string(),
        );
        assert!(result.is_err());
        match result.unwrap_err() {
            AppError::AlreadyExists(p) => assert_eq!(p, "existing_dir"),
            e => panic!("expected AlreadyExists, got {:?}", e),
        }
    }

    #[test]
    fn test_create_project_dir_traversal_rejected() {
        let root = tmp_dir("cpd_traversal");
        let result = create_project_dir(
            root.to_str().unwrap().to_string(),
            "../escaped_dir".to_string(),
        );
        assert!(result.is_err(), "path traversal must be rejected");
    }

    // ── delete_project_path ───────────────────────────────────────────────────

    #[test]
    fn test_delete_project_path_deletes_file() {
        let root = tmp_dir("dpp_file");
        fs::write(root.join("to_delete.txt"), "content").unwrap();

        let result = delete_project_path(
            root.to_str().unwrap().to_string(),
            "to_delete.txt".to_string(),
        );
        assert!(result.is_ok());
        assert!(!root.join("to_delete.txt").exists());
    }

    #[test]
    fn test_delete_project_path_deletes_dir_recursively() {
        let root = tmp_dir("dpp_dir");
        fs::create_dir_all(root.join("subdir").join("nested")).unwrap();
        fs::write(root.join("subdir").join("file.txt"), "data").unwrap();

        let result = delete_project_path(
            root.to_str().unwrap().to_string(),
            "subdir".to_string(),
        );
        assert!(result.is_ok());
        assert!(!root.join("subdir").exists());
    }

    #[test]
    fn test_delete_project_path_not_found() {
        let root = tmp_dir("dpp_missing");
        let result = delete_project_path(
            root.to_str().unwrap().to_string(),
            "ghost.txt".to_string(),
        );
        assert!(result.is_err());
        match result.unwrap_err() {
            AppError::NotFound(p) => assert_eq!(p, "ghost.txt"),
            e => panic!("expected NotFound, got {:?}", e),
        }
    }

    #[test]
    fn test_delete_project_path_cannot_delete_root() {
        let root = tmp_dir("dpp_root");
        // Passing an empty rel_path resolves to the project root itself
        let result = delete_project_path(
            root.to_str().unwrap().to_string(),
            ".".to_string(),
        );
        assert!(result.is_err(), "deleting the project root must be rejected");
    }

    #[test]
    fn test_delete_project_path_traversal_rejected() {
        let root = tmp_dir("dpp_traversal");
        // Create a file outside the root we can reference
        let outer = tmp_dir("dpp_traversal_outer");
        fs::write(outer.join("victim.txt"), "data").unwrap();

        let result = delete_project_path(
            root.to_str().unwrap().to_string(),
            "../dpp_traversal_outer/victim.txt".to_string(),
        );
        assert!(result.is_err(), "path traversal must be rejected");
        // Outer file must not have been deleted
        assert!(outer.join("victim.txt").exists(), "file outside project must not be deleted");
    }

    // ── rename_project_path ───────────────────────────────────────────────────

    #[test]
    fn test_rename_project_path_success() {
        let root = tmp_dir("rpp_ok");
        fs::write(root.join("old.txt"), "hello").unwrap();

        let result = rename_project_path(
            root.to_str().unwrap().to_string(),
            "old.txt".to_string(),
            "new.txt".to_string(),
        );
        assert!(result.is_ok());
        assert!(!root.join("old.txt").exists());
        assert!(root.join("new.txt").exists());
    }

    #[test]
    fn test_rename_project_path_source_not_found() {
        let root = tmp_dir("rpp_missing");
        let result = rename_project_path(
            root.to_str().unwrap().to_string(),
            "ghost.txt".to_string(),
            "renamed.txt".to_string(),
        );
        assert!(result.is_err());
        match result.unwrap_err() {
            AppError::NotFound(p) => assert_eq!(p, "ghost.txt"),
            e => panic!("expected NotFound, got {:?}", e),
        }
    }

    #[test]
    fn test_rename_project_path_renames_directory() {
        let root = tmp_dir("rpp_dir");
        fs::create_dir(root.join("old_dir")).unwrap();
        fs::write(root.join("old_dir").join("child.txt"), "data").unwrap();

        let result = rename_project_path(
            root.to_str().unwrap().to_string(),
            "old_dir".to_string(),
            "new_dir".to_string(),
        );
        assert!(result.is_ok());
        assert!(!root.join("old_dir").exists());
        assert!(root.join("new_dir").join("child.txt").exists());
    }

    #[test]
    fn test_rename_project_path_source_traversal_rejected() {
        let root = tmp_dir("rpp_src_trav");
        let outer = tmp_dir("rpp_src_trav_outer");
        fs::write(outer.join("file.txt"), "data").unwrap();

        let result = rename_project_path(
            root.to_str().unwrap().to_string(),
            "../rpp_src_trav_outer/file.txt".to_string(),
            "moved.txt".to_string(),
        );
        assert!(result.is_err(), "source traversal must be rejected");
    }

    #[test]
    fn test_rename_project_path_dest_traversal_rejected() {
        let root = tmp_dir("rpp_dst_trav");
        fs::write(root.join("file.txt"), "data").unwrap();

        let result = rename_project_path(
            root.to_str().unwrap().to_string(),
            "file.txt".to_string(),
            "../rpp_dst_trav_escape.txt".to_string(),
        );
        assert!(result.is_err(), "destination traversal must be rejected");
    }

    // ── read_project_file_b64 ─────────────────────────────────────────────────

    #[test]
    fn test_read_project_file_b64_encodes_correctly() {
        let root = tmp_dir("b64_ok");
        fs::write(root.join("data.bin"), b"hello world").unwrap();

        let result = read_project_file_b64(
            root.to_str().unwrap().to_string(),
            "data.bin".to_string(),
        );
        assert!(result.is_ok());
        use base64::Engine as _;
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(result.unwrap())
            .unwrap();
        assert_eq!(decoded, b"hello world");
    }

    #[test]
    fn test_read_project_file_b64_empty_file() {
        let root = tmp_dir("b64_empty");
        fs::write(root.join("empty.bin"), b"").unwrap();

        let result = read_project_file_b64(
            root.to_str().unwrap().to_string(),
            "empty.bin".to_string(),
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "");
    }

    #[test]
    fn test_read_project_file_b64_not_found() {
        let root = tmp_dir("b64_missing");
        let result = read_project_file_b64(
            root.to_str().unwrap().to_string(),
            "missing.bin".to_string(),
        );
        assert!(result.is_err());
        match result.unwrap_err() {
            AppError::NotFound(p) => assert_eq!(p, "missing.bin"),
            e => panic!("expected NotFound, got {:?}", e),
        }
    }

    #[test]
    fn test_read_project_file_b64_binary_data() {
        let root = tmp_dir("b64_binary");
        let bytes: Vec<u8> = (0u8..=255u8).collect();
        fs::write(root.join("binary.bin"), &bytes).unwrap();

        let result = read_project_file_b64(
            root.to_str().unwrap().to_string(),
            "binary.bin".to_string(),
        );
        assert!(result.is_ok());
        use base64::Engine as _;
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(result.unwrap())
            .unwrap();
        assert_eq!(decoded, bytes);
    }

    // ── list_project_tree ─────────────────────────────────────────────────────

    #[test]
    fn test_list_project_tree_returns_entries() {
        let root = tmp_dir("lpt_basic");
        fs::create_dir(root.join("src")).unwrap();
        fs::write(root.join("src").join("main.rs"), "fn main() {}").unwrap();
        fs::write(root.join("README.md"), "# readme").unwrap();

        let result = list_project_tree(root.to_str().unwrap().to_string());
        assert!(result.is_ok());
        let entries = result.unwrap();
        let paths: Vec<&str> = entries.iter().map(|e| e.path.as_str()).collect();
        assert!(paths.contains(&"src"));
        assert!(paths.contains(&"src/main.rs"));
        assert!(paths.contains(&"README.md"));
    }

    #[test]
    fn test_list_project_tree_invalid_path() {
        let result = list_project_tree("/nonexistent/path/that/does/not/exist".to_string());
        assert!(result.is_err());
        match result.unwrap_err() {
            AppError::InvalidPath(_) => {}
            e => panic!("expected InvalidPath, got {:?}", e),
        }
    }

    #[test]
    fn test_list_project_tree_skips_explorer_skip_dirs() {
        let root = tmp_dir("lpt_skip");
        fs::create_dir(root.join("node_modules")).unwrap();
        fs::write(root.join("node_modules").join("dep.js"), "").unwrap();
        fs::create_dir(root.join("target")).unwrap();
        fs::write(root.join("target").join("binary"), "").unwrap();
        fs::write(root.join("app.js"), "").unwrap();

        let result = list_project_tree(root.to_str().unwrap().to_string());
        assert!(result.is_ok());
        let entries = result.unwrap();
        let paths: Vec<&str> = entries.iter().map(|e| e.path.as_str()).collect();

        assert!(!paths.contains(&"node_modules"), "node_modules must be skipped");
        assert!(!paths.contains(&"target"), "target must be skipped");
        assert!(paths.contains(&"app.js"), "regular files must still appear");
    }

    #[test]
    fn test_list_project_tree_empty_dir() {
        let root = tmp_dir("lpt_empty");
        let result = list_project_tree(root.to_str().unwrap().to_string());
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_list_project_tree_dirs_flagged_correctly() {
        let root = tmp_dir("lpt_flags");
        fs::create_dir(root.join("mydir")).unwrap();
        fs::write(root.join("myfile.txt"), "").unwrap();

        let entries = list_project_tree(root.to_str().unwrap().to_string()).unwrap();
        let dir_entry  = entries.iter().find(|e| e.path == "mydir").unwrap();
        let file_entry = entries.iter().find(|e| e.path == "myfile.txt").unwrap();
        assert!(dir_entry.is_dir);
        assert!(!file_entry.is_dir);
    }
}
