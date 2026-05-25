import { invoke } from "@tauri-apps/api/core";

/**
 * @typedef {Object} KVPair
 * @property {string} key
 * @property {string} value
 * @property {boolean} enabled
 */

/**
 * @typedef {Object} AuthNone
 * @property {"none"} type
 */

/**
 * @typedef {Object} AuthBasic
 * @property {"basic"} type
 * @property {string} username
 * @property {string} password
 */

/**
 * @typedef {Object} AuthBearer
 * @property {"bearer"} type
 * @property {string} token
 */

/**
 * @typedef {Object} AuthApiKey
 * @property {"apikey"} type
 * @property {string} key
 * @property {string} value
 * @property {string} addTo - "header" or "query"
 */

/**
 * @typedef {AuthNone | AuthBasic | AuthBearer | AuthApiKey} AuthConfig
 */

/**
 * @typedef {Object} FormParam
 * @property {string} key
 * @property {string} value
 * @property {"text"|"file"} param_type
 * @property {boolean} enabled
 */

/**
 * @typedef {Object} RequestData
 * @property {string} method - HTTP method (GET, POST, PUT, etc.)
 * @property {string} url - URL with optional {{variable}} templates
 * @property {KVPair[]} headers
 * @property {KVPair[]} params - Query params
 * @property {KVPair[]} path_params - Path params (:id segments)
 * @property {FormParam[]} form_params - Form body fields (used when body_type is "form")
 * @property {AuthConfig} auth
 * @property {string} body_type - "json" | "form" | "raw" | "graphql" | "none"
 * @property {string} request_body - HTTP request body content (for non-form types)
 * @property {string} body - Markdown notes (not the HTTP body)
 */

/**
 * @typedef {Object} RequestResponse
 * @property {number} status
 * @property {string} statusText
 * @property {KVPair[]} headers
 * @property {string} body
 * @property {number} durationMs
 * @property {number} sizeBytes
 */

/**
 * @typedef {Object} SendRequestArgs
 * @property {string} projectPath
 * @property {RequestData} request
 * @property {Record<string,string>} envVars
 * @property {boolean} followRedirects
 * @property {number} timeoutMs
 */

/**
 * @typedef {Object} RequestTreeFile
 * @property {"file"} type
 * @property {string} name - Display name (without .md)
 * @property {string} path - Relative path from .anide/requests/
 * @property {string} method - HTTP method
 */

/**
 * @typedef {Object} RequestTreeFolder
 * @property {"folder"} type
 * @property {string} name - Folder name
 * @property {string} path - Relative path from .anide/requests/
 * @property {RequestTreeNode[]} children
 */

/**
 * @typedef {RequestTreeFile | RequestTreeFolder} RequestTreeNode
 */

// ── Commands ─────────────────────────────────────────────────────────────

/**
 * Ensures .anide/requests/ directory exists. Creates if missing.
 * @param {string} projectPath - Absolute path to the project folder
 * @returns {Promise<boolean>} true if it already existed, false if just created
 */
export async function initRequestsDir(projectPath) {
  return invoke("init_requests_dir", { projectPath });
}

/**
 * Returns the full request tree from .anide/requests/.
 * Folders first (alphabetical), then files (alphabetical).
 * @param {string} projectPath - Absolute path to the project folder
 * @returns {Promise<RequestTreeNode[]>}
 */
export async function getRequestTree(projectPath) {
  return invoke("get_request_tree", { projectPath });
}

/**
 * Reads a single request file and returns parsed data.
 * @param {string} projectPath - Absolute path to the project folder
 * @param {string} requestPath - Relative path from .anide/requests/, e.g. "auth/login.md"
 * @returns {Promise<RequestData>}
 */
export async function readRequest(projectPath, requestPath) {
  return invoke("read_request", { projectPath, requestPath });
}

/**
 * Creates a new request file. Automatically appends .md if missing.
 * Fails if the file already exists.
 * @param {string} projectPath - Absolute path to the project folder
 * @param {string} requestPath - Relative path, e.g. "auth/login.md"
 * @param {RequestData} data - Request data to write
 * @returns {Promise<void>}
 */
export async function createRequest(projectPath, requestPath, data) {
  return invoke("create_request", { projectPath, requestPath, data });
}

/**
 * Updates an existing request file. Overwrites the file.
 * @param {string} projectPath - Absolute path to the project folder
 * @param {string} requestPath - Relative path, e.g. "auth/login.md"
 * @param {RequestData} data - Updated request data
 * @returns {Promise<void>}
 */
export async function updateRequest(projectPath, requestPath, data) {
  return invoke("update_request", { projectPath, requestPath, data });
}

/**
 * Deletes a request file. Cleans up empty parent folders.
 * @param {string} projectPath - Absolute path to the project folder
 * @param {string} requestPath - Relative path, e.g. "auth/login.md"
 * @returns {Promise<void>}
 */
export async function deleteRequest(projectPath, requestPath) {
  return invoke("delete_request", { projectPath, requestPath });
}

/**
 * Duplicates a request file with auto-naming (login-copy.md, login-copy-2.md, etc.)
 * @param {string} projectPath - Absolute path to the project folder
 * @param {string} requestPath - Relative path of the request to duplicate
 * @returns {Promise<string>} The new file's relative path
 */
export async function duplicateRequest(projectPath, requestPath) {
  return invoke("duplicate_request", { projectPath, requestPath });
}

/**
 * Renames a request file within its current directory.
 * @param {string} projectPath
 * @param {string} requestPath - Current relative path, e.g. "auth/login.md"
 * @param {string} newName - New stem (without .md), e.g. "signin"
 * @returns {Promise<string>} The new relative path
 */
export async function renameRequest(projectPath, requestPath, newName) {
  return invoke("rename_request", { projectPath, requestPath, newName });
}

/**
 * Renames a collection (folder) within its current parent directory.
 * @param {string} projectPath
 * @param {string} collectionPath - Current relative path, e.g. "auth"
 * @param {string} newName - New folder name
 * @returns {Promise<string>} The new relative path
 */
export async function renameCollection(projectPath, collectionPath, newName) {
  return invoke("rename_collection", { projectPath, collectionPath, newName });
}

/**
 * Creates a collection (folder) inside .anide/requests/.
 * Supports nested paths, e.g. "auth/admin" creates both auth/ and auth/admin/.
 * @param {string} projectPath - Absolute path to the project folder
 * @param {string} collectionPath - Relative folder path, e.g. "auth/admin"
 * @returns {Promise<void>}
 */
export async function createCollection(projectPath, collectionPath) {
  return invoke("create_collection", { projectPath, collectionPath });
}

/**
 * Send an HTTP request, resolving {{ENV_VAR}} tokens before sending.
 * @param {SendRequestArgs} args
 * @returns {Promise<RequestResponse>}
 */
export async function sendRequest(args) {
  return invoke("send_request", { args });
}

/**
 * Resolve a single template string against env vars.
 * @param {string} template
 * @param {Record<string,string>} envVars
 * @returns {Promise<{ resolved: string, tokens: Array<{token: string, value: string|null}> }>}
 */
export async function resolveTemplate(template, envVars) {
  return invoke("resolve_template", { template, envVars });
}

// ── Helpers ──────────────────────────────────────────────────────────────

/**
 * Creates a default empty RequestData object.
 * @param {string} [method="GET"] - HTTP method
 * @returns {RequestData}
 */
export function createEmptyRequest(method = "GET") {
  return {
    method,
    url: "",
    headers: [],
    params: [],
    path_params: [],
    form_params: [],
    auth: { type: "none" },
    body_type: "none",
    request_body: "",
    body: "",
  };
}
