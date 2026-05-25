use std::collections::HashMap;
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

/// A form body field — either text or a file path for multipart upload
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FormParam {
    pub key: String,
    pub value: String,
    /// "text" | "file"
    #[serde(default = "default_form_type")]
    pub param_type: String,
    pub enabled: bool,
}
fn default_form_type() -> String { "text".to_string() }

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
    #[serde(rename = "oauth2")]
    OAuth2 {
        grant_type: String,
        token_url: String,
        client_id: String,
        client_secret: String,
        scope: String,
        /// Cached access token (not persisted to file)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        access_token: Option<String>,
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub path_params: Vec<KVPair>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub form_params: Vec<FormParam>,
    #[serde(default)]
    pub auth: AuthConfig,
    /// HTTP body type: "json" | "form" | "raw" | "graphql" | "none"
    #[serde(default = "default_body_type", skip_serializing_if = "is_none_body_type")]
    pub body_type: String,
    /// The HTTP request body content (template strings allowed)
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub request_body: String,
}

fn default_body_type() -> String { "none".to_string() }
fn is_none_body_type(s: &String) -> bool { s == "none" }

/// Full request data sent to/from the frontend.
/// Combines the frontmatter fields with the markdown body (notes).
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RequestData {
    pub method: String,
    pub url: String,
    #[serde(default)]
    pub headers: Vec<KVPair>,
    #[serde(default)]
    pub params: Vec<KVPair>,
    #[serde(default)]
    pub path_params: Vec<KVPair>,
    #[serde(default)]
    pub form_params: Vec<FormParam>,
    #[serde(default)]
    pub auth: AuthConfig,
    /// HTTP body type: "json" | "form" | "raw" | "graphql" | "none"
    #[serde(default = "default_body_type")]
    pub body_type: String,
    /// The HTTP request body content (template strings allowed)
    #[serde(default)]
    pub request_body: String,
    /// Markdown notes body (everything after the frontmatter `---`, purely documentation)
    #[serde(default)]
    pub body: String,
}

// ── HTTP execution types ──────────────────────────────────────────────────

/// Input to send_request command
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendRequestArgs {
    pub project_path: String,
    pub request: RequestData,
    /// Flat map of env var key → value (resolved in priority order by frontend)
    pub env_vars: HashMap<String, String>,
    pub follow_redirects: bool,
    pub timeout_ms: u32,
}

/// Response returned from send_request
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestResponse {
    pub status: u16,
    pub status_text: String,
    pub headers: Vec<KVPair>,
    pub body: String,
    pub duration_ms: u64,
    pub size_bytes: usize,
}

/// Result of resolve_template — resolved string plus token info for Variable Inspector
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedTemplate {
    pub resolved: String,
    pub tokens: Vec<TokenInfo>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenInfo {
    pub token: String,
    pub value: Option<String>,
}

/// A node in the request tree (either a file or a folder)
#[derive(Serialize, Debug)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum RequestTreeNode {
    File {
        /// Display name (filename without .md)
        name: String,
        /// Relative path from .anide/requests/ (forward slashes)
        path: String,
        /// HTTP method for quick display (parsed from frontmatter)
        method: String,
    },
    Folder {
        /// Folder name
        name: String,
        /// Relative path from .anide/requests/ (forward slashes)
        path: String,
        /// Children nodes (sorted: folders first, then files, alphabetically)
        children: Vec<RequestTreeNode>,
    },
}

// ── Commands ─────────────────────────────────────────────────────────────

/// Ensures .anide/requests/ exists. Creates if missing.
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

/// Returns the full request tree from .anide/requests/.
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
/// `request_path` is relative to .anide/requests/, e.g. "auth/login.md"
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

    // Clean up empty parent directories (up to .anide/requests/)
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

/// Renames a request file within its current directory.
/// `new_name` is just the stem (no .md needed). Returns the new relative path.
#[tauri::command]
pub fn rename_request(
    project_path: String,
    request_path: String,
    new_name: String,
) -> Result<String, AppError> {
    let full_path = resolve_request_path(&project_path, &request_path)?;
    if !full_path.exists() {
        return Err(AppError::NotFound(format!("Request file not found: {}", request_path)));
    }
    let stem = new_name.trim();
    if stem.is_empty() {
        return Err(AppError::Other("Name cannot be empty".into()));
    }
    if stem.contains('/') || stem.contains('\\') {
        return Err(AppError::InvalidPath("Name cannot contain path separators".into()));
    }
    let parent = full_path.parent().ok_or_else(|| AppError::Other("Invalid path".into()))?;
    let new_filename = if stem.ends_with(".md") { stem.to_string() } else { format!("{}.md", stem) };
    let new_full_path = parent.join(&new_filename);
    if new_full_path.exists() {
        return Err(AppError::AlreadyExists(format!("A request named '{}' already exists", stem)));
    }
    fs::rename(&full_path, &new_full_path)?;
    let requests_dir = requests_dir_path(&project_path);
    let rel = new_full_path
        .strip_prefix(&requests_dir)
        .unwrap_or(&new_full_path)
        .to_string_lossy()
        .replace('\\', "/");
    Ok(rel)
}

/// Renames a collection (folder) within its current parent directory.
/// `new_name` is the new folder name. Returns the new relative path.
#[tauri::command]
pub fn rename_collection(
    project_path: String,
    collection_path: String,
    new_name: String,
) -> Result<String, AppError> {
    let requests_dir = requests_dir_path(&project_path);
    let full_path = requests_dir.join(&collection_path);
    if !full_path.exists() || !full_path.is_dir() {
        return Err(AppError::NotFound(format!("Collection not found: {}", collection_path)));
    }
    // Verify the resolved path is actually inside requests_dir (prevents traversal).
    let canonical_base = requests_dir.canonicalize()?;
    let canonical_full = full_path.canonicalize()?;
    if !canonical_full.starts_with(&canonical_base) {
        return Err(AppError::InvalidPath("Path traversal denied".into()));
    }
    let name = new_name.trim();
    if name.is_empty() {
        return Err(AppError::Other("Name cannot be empty".into()));
    }
    if name.contains('/') || name.contains('\\') {
        return Err(AppError::InvalidPath("Name cannot contain path separators".into()));
    }
    let parent = full_path.parent().ok_or_else(|| AppError::Other("Invalid path".into()))?;
    let new_full_path = parent.join(name);
    if new_full_path.exists() {
        return Err(AppError::AlreadyExists(format!("A collection named '{}' already exists", name)));
    }
    fs::rename(&full_path, &new_full_path)?;
    let rel = new_full_path
        .strip_prefix(&requests_dir)
        .unwrap_or(&new_full_path)
        .to_string_lossy()
        .replace('\\', "/");
    Ok(rel)
}

/// Creates a collection (folder) inside .anide/requests/.
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
                    "Collection path must be within .anide/requests".to_string(),
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
            "Collection path must be within .anide/requests".to_string(),
        ));
    }

    Ok(())
}

/// Send an HTTP request, resolving {{ENV_VAR}} template tokens before sending.
/// Faker tokens must be resolved in the frontend before this is called.
#[tauri::command]
pub async fn send_request(args: SendRequestArgs) -> Result<RequestResponse, AppError> {
    let req = resolve_request_templates(&args.request, &args.env_vars);

    // Substitute path params in URL before parsing (e.g. :userId → "123")
    let url_with_paths = substitute_path_params(&req.url, &req.path_params);

    // Build URL and attach query params
    let mut url = reqwest::Url::parse(&url_with_paths)
        .map_err(|e| AppError::Other(format!("Invalid URL: {e}")))?;

    {
        let mut pairs = url.query_pairs_mut();
        for p in &req.params {
            if p.enabled && !p.key.is_empty() {
                pairs.append_pair(&p.key, &p.value);
            }
        }
    }

    let redirect_policy = if args.follow_redirects {
        reqwest::redirect::Policy::limited(10)
    } else {
        reqwest::redirect::Policy::none()
    };

    let client = reqwest::Client::builder()
        .redirect(redirect_policy)
        .timeout(std::time::Duration::from_millis(args.timeout_ms as u64))
        .danger_accept_invalid_certs(false)
        .build()?;

    let method = reqwest::Method::from_bytes(req.method.to_uppercase().as_bytes())
        .map_err(|e| AppError::Other(format!("Invalid HTTP method: {e}")))?;

    let mut builder = client.request(method, url);

    // Apply enabled headers
    for h in &req.headers {
        if h.enabled && !h.key.is_empty() {
            builder = builder.header(h.key.as_str(), h.value.as_str());
        }
    }

    // Apply auth (mutates builder + may add query param — handled separately)
    builder = apply_auth(builder, &req.auth)?;

    // Apply body — use form_params KV table when present, else fall back to request_body
    if req.body_type == "form" && !req.form_params.is_empty() {
        builder = apply_form_params(builder, &req.form_params).await?;
    } else {
        builder = apply_body(builder, &req.body_type, &req.request_body);
    }

    let start = std::time::Instant::now();
    let response = builder.send().await?;
    let duration_ms = start.elapsed().as_millis() as u64;

    let status = response.status().as_u16();
    let status_text = response
        .status()
        .canonical_reason()
        .unwrap_or("")
        .to_string();

    let resp_headers: Vec<KVPair> = response
        .headers()
        .iter()
        .map(|(k, v)| KVPair {
            key: k.to_string(),
            value: v.to_str().unwrap_or("<binary>").to_string(),
            enabled: true,
        })
        .collect();

    let body_bytes = response.bytes().await?;
    let size_bytes = body_bytes.len();
    let body = String::from_utf8_lossy(&body_bytes).to_string();

    Ok(RequestResponse {
        status,
        status_text,
        headers: resp_headers,
        body,
        duration_ms,
        size_bytes,
    })
}

/// Resolve a single template string against env vars and return token info.
/// Pure function — no network, no file I/O.
#[tauri::command]
pub fn resolve_template(
    template: String,
    env_vars: HashMap<String, String>,
) -> Result<ResolvedTemplate, AppError> {
    let mut tokens = Vec::new();
    let resolved = substitute_template(&template, &env_vars, &mut tokens);
    Ok(ResolvedTemplate { resolved, tokens })
}

// ── Helpers ──────────────────────────────────────────────────────────────

/// Get the path to .anide/requests/ directory.
fn requests_dir_path(project_path: &str) -> PathBuf {
    Path::new(project_path)
        .join(".anide")
        .join("requests")
}

/// Resolve a request_path relative to .anide/requests/ and validate it's safe.
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
        path_params: fm.path_params,
        form_params: fm.form_params,
        auth: fm.auth,
        body_type: fm.body_type,
        request_body: fm.request_body,
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
        path_params: data.path_params.clone(),
        form_params: data.form_params.clone(),
        auth: data.auth.clone(),
        body_type: data.body_type.clone(),
        request_body: data.request_body.clone(),
    };

    let yaml_str = serde_yaml::to_string(&fm)?;
    Ok(frontmatter::serialize(&yaml_str, &data.body))
}

/// Replace all {{KEY}} tokens in `template` using `env_vars`.
/// Unknown tokens and Faker/env-namespaced tokens are left as-is.
fn substitute_template(
    template: &str,
    env_vars: &HashMap<String, String>,
    tokens: &mut Vec<TokenInfo>,
) -> String {
    let mut result = String::with_capacity(template.len());
    let mut rest = template;

    while let Some(open) = rest.find("{{") {
        result.push_str(&rest[..open]);
        let after_open = &rest[open + 2..];
        if let Some(close) = after_open.find("}}") {
            let key = after_open[..close].trim();
            // Faker and namespaced env tokens are resolved by the frontend — leave them
            if key.starts_with("Faker.") || key.starts_with("env.") {
                result.push_str("{{");
                result.push_str(&after_open[..close]);
                result.push_str("}}");
                tokens.push(TokenInfo { token: key.to_string(), value: None });
            } else if let Some(val) = env_vars.get(key) {
                result.push_str(val);
                tokens.push(TokenInfo { token: key.to_string(), value: Some(val.clone()) });
            } else {
                result.push_str("{{");
                result.push_str(&after_open[..close]);
                result.push_str("}}");
                tokens.push(TokenInfo { token: key.to_string(), value: None });
            }
            rest = &after_open[close + 2..];
        } else {
            // No closing }} — emit {{ literally and continue
            result.push_str("{{");
            rest = after_open;
        }
    }
    result.push_str(rest);
    result
}

/// substitute without token collection — for internal use where we don't need the token list.
fn sub(template: &str, env_vars: &HashMap<String, String>) -> String {
    substitute_template(template, env_vars, &mut Vec::new())
}

/// Resolve all template strings in a RequestData, returning a new resolved copy.
fn resolve_request_templates(
    req: &RequestData,
    env_vars: &HashMap<String, String>,
) -> RequestData {
    RequestData {
        method: req.method.clone(),
        url: sub(&req.url, env_vars),
        headers: req
            .headers
            .iter()
            .map(|h| KVPair { key: h.key.clone(), value: sub(&h.value, env_vars), enabled: h.enabled })
            .collect(),
        params: req
            .params
            .iter()
            .map(|p| KVPair { key: p.key.clone(), value: sub(&p.value, env_vars), enabled: p.enabled })
            .collect(),
        path_params: req
            .path_params
            .iter()
            .map(|p| KVPair { key: p.key.clone(), value: sub(&p.value, env_vars), enabled: p.enabled })
            .collect(),
        form_params: req
            .form_params
            .iter()
            .map(|p| FormParam {
                key: p.key.clone(),
                value: sub(&p.value, env_vars),
                param_type: p.param_type.clone(),
                enabled: p.enabled,
            })
            .collect(),
        auth: resolve_auth_templates(&req.auth, env_vars),
        body_type: req.body_type.clone(),
        request_body: sub(&req.request_body, env_vars),
        body: req.body.clone(),
    }
}

fn resolve_auth_templates(auth: &AuthConfig, env_vars: &HashMap<String, String>) -> AuthConfig {
    match auth {
        AuthConfig::None => AuthConfig::None,
        AuthConfig::Basic { username, password } => AuthConfig::Basic {
            username: sub(username, env_vars),
            password: sub(password, env_vars),
        },
        AuthConfig::Bearer { token } => AuthConfig::Bearer {
            token: sub(token, env_vars),
        },
        AuthConfig::ApiKey { key, value, add_to } => AuthConfig::ApiKey {
            key: key.clone(),
            value: sub(value, env_vars),
            add_to: add_to.clone(),
        },
        AuthConfig::OAuth2 { grant_type, token_url, client_id, client_secret, scope, access_token } => {
            AuthConfig::OAuth2 {
                grant_type: grant_type.clone(),
                token_url: sub(token_url, env_vars),
                client_id: sub(client_id, env_vars),
                client_secret: sub(client_secret, env_vars),
                scope: scope.clone(),
                access_token: access_token.clone(),
            }
        }
    }
}

/// Apply auth config to a reqwest RequestBuilder.
fn apply_auth(
    builder: reqwest::RequestBuilder,
    auth: &AuthConfig,
) -> Result<reqwest::RequestBuilder, AppError> {
    use base64::Engine as _;
    match auth {
        AuthConfig::None => Ok(builder),
        AuthConfig::Bearer { token } => {
            Ok(builder.header("Authorization", format!("Bearer {token}")))
        }
        AuthConfig::Basic { username, password } => {
            let encoded = base64::engine::general_purpose::STANDARD
                .encode(format!("{username}:{password}"));
            Ok(builder.header("Authorization", format!("Basic {encoded}")))
        }
        AuthConfig::ApiKey { key, value, add_to } => {
            if add_to == "header" {
                Ok(builder.header(key.as_str(), value.as_str()))
            } else {
                // query — append to URL via the builder's query method
                Ok(builder.query(&[(key.as_str(), value.as_str())]))
            }
        }
        // OAuth2: if a cached access_token is present use it; otherwise tell the caller
        // to exchange credentials first via a dedicated resolve_oauth2 command (future).
        AuthConfig::OAuth2 { access_token, .. } => {
            if let Some(tok) = access_token {
                Ok(builder.header("Authorization", format!("Bearer {tok}")))
            } else {
                Err(AppError::Other(
                    "OAuth2 access token not resolved. Fetch a token first.".to_string(),
                ))
            }
        }
    }
}

/// Replace :paramName segments in a URL with their resolved values.
fn substitute_path_params(url: &str, path_params: &[KVPair]) -> String {
    let mut result = url.to_string();
    for p in path_params {
        if p.enabled && !p.key.is_empty() {
            result = result.replace(&format!(":{}", p.key), &p.value);
        }
    }
    result
}

/// Build the request body from form_params: multipart/form-data when any file param
/// exists, otherwise application/x-www-form-urlencoded.
async fn apply_form_params(
    builder: reqwest::RequestBuilder,
    form_params: &[FormParam],
) -> Result<reqwest::RequestBuilder, AppError> {
    let has_file = form_params
        .iter()
        .any(|p| p.enabled && !p.key.is_empty() && p.param_type == "file");

    if has_file {
        let mut form = reqwest::multipart::Form::new();
        for p in form_params {
            if !p.enabled || p.key.is_empty() { continue; }
            if p.param_type == "file" && !p.value.is_empty() {
                let bytes = tokio::fs::read(&p.value).await.map_err(|e| {
                    AppError::Other(format!("Cannot read file '{}': {e}", p.value))
                })?;
                let fname = std::path::Path::new(&p.value)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                let part = reqwest::multipart::Part::bytes(bytes).file_name(fname);
                form = form.part(p.key.clone(), part);
            } else if p.param_type != "file" {
                form = form.text(p.key.clone(), p.value.clone());
            }
        }
        Ok(builder.multipart(form))
    } else {
        // application/x-www-form-urlencoded using the urlencoding crate
        let body = form_params
            .iter()
            .filter(|p| p.enabled && !p.key.is_empty() && p.param_type != "file")
            .map(|p| {
                format!(
                    "{}={}",
                    urlencoding::encode(&p.key),
                    urlencoding::encode(&p.value)
                )
            })
            .collect::<Vec<_>>()
            .join("&");
        Ok(builder
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(body))
    }
}

/// Apply the request body to a reqwest RequestBuilder based on body_type.
fn apply_body(
    builder: reqwest::RequestBuilder,
    body_type: &str,
    body: &str,
) -> reqwest::RequestBuilder {
    match body_type {
        "json" => builder
            .header("Content-Type", "application/json")
            .body(body.to_string()),
        "form" => builder
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(body.to_string()),
        "graphql" => {
            // Wrap query in {"query":"..."} if not already a JSON object
            let json_body = if body.trim_start().starts_with('{') {
                body.to_string()
            } else {
                let escaped = body.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n");
                format!("{{\"query\":\"{escaped}\"}}")
            };
            builder
                .header("Content-Type", "application/json")
                .body(json_body)
        }
        "raw" => builder.body(body.to_string()),
        _ => builder, // "none" or unknown — no body
    }
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