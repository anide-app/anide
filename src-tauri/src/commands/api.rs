use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::utils::frontmatter;

// ── Types ────────────────────────────────────────────────────────────────

/// A key-value pair with an enable toggle (for headers, params)
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct KVPair {
    pub key: String,
    pub value: String,
    pub enabled: bool,
}

/// Auth configuration — tagged union serialized as { "type": "bearer", ... }
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum AuthConfig {
    None,
    Basic {
        username: String,
        password: String,
    },
    Bearer {
        token: String,
    },
    #[serde(rename = "apikey")]
    ApiKey {
        key: String,
        value: String,
        #[serde(rename = "addTo")]
        add_to: String,
    },
}

impl Default for AuthConfig {
    fn default() -> Self {
        AuthConfig::None
    }
}

/// The YAML frontmatter portion of a request file.
/// This is what gets serialized to/from the YAML block between `---` delimiters.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RequestFrontmatter {
    pub method: String,
    pub url: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub headers: Vec<KVPair>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<KVPair>,
    #[serde(default)]
    pub auth: AuthConfig,
}

/// Full request data sent to/from the frontend.
/// Combines the frontmatter fields with the markdown body.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RequestData {
    pub method: String,
    pub url: String,
    #[serde(default)]
    pub headers: Vec<KVPair>,
    #[serde(default)]
    pub params: Vec<KVPair>,
    #[serde(default)]
    pub auth: AuthConfig,
    /// Markdown body content (everything after the frontmatter `---`)
    #[serde(default)]
    pub body: String,
}

/// A node in the request tree (either a file or a folder)
#[derive(Serialize, Debug)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum RequestTreeNode {
    File {
        /// Display name (filename without .md)
        name: String,
        /// Relative path from .takerest/requests/ (forward slashes)
        path: String,
        /// HTTP method for quick display (parsed from frontmatter)
        method: String,
    },
    Folder {
        /// Folder name
        name: String,
        /// Relative path from .takerest/requests/ (forward slashes)
        path: String,
        /// Children nodes (sorted: folders first, then files, alphabetically)
        children: Vec<RequestTreeNode>,
    },
}

// ── Commands ─────────────────────────────────────────────────────────────

/// Ensures .takerest/requests/ exists. Creates if missing.
/// Returns true if it already existed, false if it was just created.
#[tauri::command]
pub fn init_requests_dir(project_path: String) -> Result<bool, AppError> {
    let requests_dir = requests_dir_path(&project_path);
    if requests_dir.exists() {
        Ok(true)
    } else {
        fs::create_dir_all(&requests_dir)?;
        Ok(false)
    }
}

/// Returns the full request tree from .takerest/requests/.
/// Folders become Folder nodes, .md files become File nodes.
/// Sorted: folders first (alphabetical), then files (alphabetical).
#[tauri::command]
pub fn get_request_tree(project_path: String) -> Result<Vec<RequestTreeNode>, AppError> {
    let requests_dir = requests_dir_path(&project_path);

    // Ensure the directory exists
    init_requests_dir(project_path)?;

    if !requests_dir.is_dir() {
        return Ok(Vec::new());
    }

    build_tree(&requests_dir, &requests_dir)
}

/// Reads a single request file and returns parsed data.
/// `request_path` is relative to .takerest/requests/, e.g. "auth/login.md"
#[tauri::command]
pub fn read_request(project_path: String, request_path: String) -> Result<RequestData, AppError> {
    let full_path = resolve_request_path(&project_path, &request_path)?;

    if !full_path.exists() {
        return Err(AppError::NotFound(format!(
            "Request file not found: {}",
            request_path
        )));
    }

    let content = fs::read_to_string(&full_path)?;
    parse_request_file(&content)
}

/// Creates a new request file.
/// `request_path` is relative, e.g. "auth/login.md"
/// Automatically appends .md if missing.
/// Fails if file already exists.
#[tauri::command]
pub fn create_request(
    project_path: String,
    request_path: String,
    data: RequestData,
) -> Result<(), AppError> {
    let path = ensure_md_extension(&request_path);
    let full_path = resolve_request_path(&project_path, &path)?;

    if full_path.exists() {
        return Err(AppError::AlreadyExists(format!(
            "Request file already exists: {}",
            path
        )));
    }

    // Ensure parent directory exists
    if let Some(parent) = full_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let content = serialize_request_file(&data)?;
    fs::write(&full_path, content)?;
    Ok(())
}

/// Updates an existing request file. Overwrites the file.
#[tauri::command]
pub fn update_request(
    project_path: String,
    request_path: String,
    data: RequestData,
) -> Result<(), AppError> {
    let full_path = resolve_request_path(&project_path, &request_path)?;

    if !full_path.exists() {
        return Err(AppError::NotFound(format!(
            "Request file not found: {}",
            request_path
        )));
    }

    let content = serialize_request_file(&data)?;
    fs::write(&full_path, content)?;
    Ok(())
}

/// Deletes a request file.
#[tauri::command]
pub fn delete_request(project_path: String, request_path: String) -> Result<(), AppError> {
    let full_path = resolve_request_path(&project_path, &request_path)?;

    if !full_path.exists() {
        return Err(AppError::NotFound(format!(
            "Request file not found: {}",
            request_path
        )));
    }

    fs::remove_file(&full_path)?;

    // Clean up empty parent directories (up to .takerest/requests/)
    let requests_dir = requests_dir_path(&project_path);
    let mut parent = full_path.parent();
    while let Some(dir) = parent {
        if dir == requests_dir {
            break;
        }
        // Only remove if the directory is empty
        if fs::read_dir(dir).map_or(false, |mut d| d.next().is_none()) {
            let _ = fs::remove_dir(dir);
            parent = dir.parent();
        } else {
            break;
        }
    }

    Ok(())
}

/// Duplicates a request file.
/// New file is named "<original>-copy.md", or "<original>-copy-2.md" if that exists, etc.
/// Returns the new file's relative path.
#[tauri::command]
pub fn duplicate_request(project_path: String, request_path: String) -> Result<String, AppError> {
    let full_path = resolve_request_path(&project_path, &request_path)?;

    if !full_path.exists() {
        return Err(AppError::NotFound(format!(
            "Request file not found: {}",
            request_path
        )));
    }

    // Read original content
    let content = fs::read_to_string(&full_path)?;

    // Generate copy name
    let stem = full_path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let parent = full_path
        .parent()
        .ok_or_else(|| AppError::Other("invalid request path: no parent directory".into()))?;
    let requests_dir = requests_dir_path(&project_path);

    let copy_path = generate_copy_name(parent, &stem);
    fs::write(&copy_path, &content)?;

    // Return relative path with forward slashes
    let rel = copy_path
        .strip_prefix(&requests_dir)
        .unwrap_or(&copy_path)
        .to_string_lossy()
        .replace('\\', "/");

    Ok(rel)
}

/// Creates a collection (folder) inside .takerest/requests/.
/// Supports nested paths, e.g. "auth/admin" creates auth/admin/ (and auth/ if needed).
#[tauri::command]
pub fn create_collection(project_path: String, collection_path: String) -> Result<(), AppError> {
    // Validate components before touching the filesystem
    for component in Path::new(&collection_path).components() {
        match component {
            std::path::Component::ParentDir
            | std::path::Component::RootDir
            | std::path::Component::Prefix(_) => {
                return Err(AppError::InvalidPath(
                    "Collection path must be within .anide/requests/".to_string(),
                ));
            }
            _ => {}
        }
    }

    let requests_dir = requests_dir_path(&project_path);
    let full_path = requests_dir.join(&collection_path);

    let canonical_requests = requests_dir
        .canonicalize()
        .unwrap_or_else(|_| requests_dir.clone());
    fs::create_dir_all(&full_path)?;
    let canonical_target = full_path
        .canonicalize()
        .unwrap_or_else(|_| full_path.clone());

    if !canonical_target.starts_with(&canonical_requests) {
        let _ = fs::remove_dir_all(&full_path);
        return Err(AppError::InvalidPath(
            "Collection path must be within .anide/requests/".to_string(),
        ));
    }

    Ok(())
}

// ── Helpers ──────────────────────────────────────────────────────────────

/// Get the path to .takerest/requests/ directory.
fn requests_dir_path(project_path: &str) -> PathBuf {
    Path::new(project_path)
        .join(".anide")
        .join("requests")
}

/// Resolve a request_path relative to .takerest/requests/ and validate it's safe.
fn resolve_request_path(project_path: &str, request_path: &str) -> Result<PathBuf, AppError> {
    let requests_dir = requests_dir_path(project_path);
    let full_path = requests_dir.join(request_path);

    // Reject absolute paths — on Unix a leading '/' becomes RootDir; on Windows
    // a drive letter becomes Prefix.  Both cause join() to discard requests_dir.
    if Path::new(request_path).is_absolute() {
        return Err(AppError::InvalidPath(
            "Path traversal not allowed in request path".to_string(),
        ));
    }

    // Reject ParentDir (..) and, defensively, any RootDir/Prefix component.
    for component in Path::new(request_path).components() {
        match component {
            std::path::Component::ParentDir
            | std::path::Component::RootDir
            | std::path::Component::Prefix(_) => {
                return Err(AppError::InvalidPath(
                    "Path traversal not allowed in request path".to_string(),
                ));
            }
            _ => {}
        }
    }

    // ── Canonical containment check (mirrors create_collection) ─────────────
    // Prevents symlink escapes even after lexical validation.
    let canonical_requests = requests_dir
        .canonicalize()
        .unwrap_or_else(|_| requests_dir.clone());

    let canonical_target = if full_path.exists() {
        // Target exists → safe to canonicalize directly (used by read/update/delete/duplicate).
        full_path
            .canonicalize()
            .unwrap_or_else(|_| full_path.clone())
    } else if let Some(parent) = full_path.parent() {
        if parent.exists() {
            // Parent exists but file does not (typical create_request case).
            // Canonicalize parent + re-join filename so any symlinks in the
            // existing ancestor path are resolved.
            let canonical_parent = parent
                .canonicalize()
                .unwrap_or_else(|_| parent.to_path_buf());
            let file_name = full_path.file_name().unwrap_or_default();
            canonical_parent.join(file_name)
        } else {
            // Parent does not exist yet → new subtree. Lexical checks already
            // passed and no symlinks can exist in a non-existent path, so safe.
            full_path.clone()
        }
    } else {
        full_path.clone()
    };

    if !canonical_target.starts_with(&canonical_requests) {
        return Err(AppError::InvalidPath(
            "Path traversal not allowed in request path".to_string(),
        ));
    }

    Ok(full_path)
}

/// Ensure a path ends with .md extension.
fn ensure_md_extension(path: &str) -> String {
    if path.ends_with(".md") {
        path.to_string()
    } else {
        format!("{}.md", path)
    }
}

/// Generate a copy filename that doesn't already exist.
/// login.md → login-copy.md → login-copy-2.md → login-copy-3.md → ...
fn generate_copy_name(parent: &Path, stem: &str) -> PathBuf {
    let first = parent.join(format!("{}-copy.md", stem));
    if !first.exists() {
        return first;
    }

    let mut n = 2u32;
    loop {
        let candidate = parent.join(format!("{}-copy-{}.md", stem, n));
        if !candidate.exists() {
            return candidate;
        }
        n += 1;
    }
}

/// Parse a request file's content into RequestData.
fn parse_request_file(content: &str) -> Result<RequestData, AppError> {
    let (yaml_str, body) = frontmatter::parse(content)?;

    let fm: RequestFrontmatter = serde_yaml::from_str(&yaml_str)?;

    Ok(RequestData {
        method: fm.method,
        url: fm.url,
        headers: fm.headers,
        params: fm.params,
        auth: fm.auth,
        body,
    })
}

/// Serialize RequestData into a complete markdown file with YAML frontmatter.
fn serialize_request_file(data: &RequestData) -> Result<String, AppError> {
    let fm = RequestFrontmatter {
        method: data.method.clone(),
        url: data.url.clone(),
        headers: data.headers.clone(),
        params: data.params.clone(),
        auth: data.auth.clone(),
    };

    let yaml_str = serde_yaml::to_string(&fm)?;
    Ok(frontmatter::serialize(&yaml_str, &data.body))
}

/// Quick-parse only the HTTP method from a request file's frontmatter.
/// Used by get_request_tree to avoid fully parsing every file.
fn quick_parse_method(content: &str) -> String {
    // Only scan lines that are within the frontmatter block.
    // A well-formed file starts with "---\n"; the second "---" closes it.
    if !content.starts_with("---") {
        return "GET".to_string();
    }

    let mut seen_opening = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == "---" {
            if !seen_opening {
                // This is the opening delimiter.
                seen_opening = true;
                continue;
            } else {
                // This is the closing delimiter — stop scanning.
                break;
            }
        }
        if seen_opening && trimmed.starts_with("method:") {
            return trimmed
                .strip_prefix("method:")
                .unwrap_or("")
                .trim()
                .trim_matches('"')
                .trim_matches('\'')
                .to_uppercase();
        }
    }
    "GET".to_string() // default
}

/// Recursively build the request tree from the filesystem.
fn build_tree(dir: &Path, requests_root: &Path) -> Result<Vec<RequestTreeNode>, AppError> {
    let mut folders: Vec<RequestTreeNode> = Vec::new();
    let mut files: Vec<RequestTreeNode> = Vec::new();

    // Use a BTreeMap to get sorted directory entries
    let mut entries: Vec<_> = fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .collect();

    // Sort entries alphabetically
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();

        // Relative path from requests root (with forward slashes for cross-platform)
        let rel_path = path
            .strip_prefix(requests_root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");

        // ── Symlink-safe check (replaces path.is_dir()) ─────────────────────
        if let Ok(file_type) = entry.file_type() {
            if file_type.is_symlink() {
                continue; // Explicitly ignore symlinks (prevents escape + cycles)
            }

            if file_type.is_dir() {
                let children = build_tree(&path, requests_root)?;
                folders.push(RequestTreeNode::Folder {
                    name,
                    path: rel_path,
                    children,
                });
            } else if file_type.is_file()
                && path.extension().map_or(false, |ext| ext == "md")
            {
                // Only include real .md files (symlink-to-file is already skipped)
                let content = fs::read_to_string(&path).unwrap_or_default();
                let method = quick_parse_method(&content);
                let display_name = name.strip_suffix(".md").unwrap_or(&name).to_string();

                files.push(RequestTreeNode::File {
                    name: display_name,
                    path: rel_path,
                    method,
                });
            }
        }
        // file_type() error → silently skip (matches original error tolerance)
    }

    // Folders first, then files (both already sorted alphabetically)
    let mut result = Vec::with_capacity(folders.len() + files.len());
    result.append(&mut folders);
    result.append(&mut files);
    Ok(result)
}