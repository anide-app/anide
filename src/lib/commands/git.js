import { invoke } from "@tauri-apps/api/core";

/** @returns {Promise<{ files: FileStatus[], total: number }>} */
export async function gitStatus(projectPath) {
  return invoke("git_status", { projectPath });
}

/**
 * @param {string} projectPath
 * @param {string} relPath
 * @param {boolean} staged - true = HEAD→index, false = index→worktree
 * @param {number} [maxLines]
 * @returns {Promise<DiffResult>}
 */
export async function gitDiffFile(projectPath, relPath, staged, maxLines) {
  return invoke("git_diff_file", { projectPath, relPath, staged, maxLines: maxLines ?? null });
}

/** @returns {Promise<void>} */
export async function gitStageFile(projectPath, relPath) {
  return invoke("git_stage_file", { projectPath, relPath });
}

/** @returns {Promise<void>} */
export async function gitUnstageFile(projectPath, relPath) {
  return invoke("git_unstage_file", { projectPath, relPath });
}

/** @returns {Promise<void>} */
export async function gitStageAll(projectPath) {
  return invoke("git_stage_all", { projectPath });
}

/** @returns {Promise<void>} */
export async function gitUnstageAll(projectPath) {
  return invoke("git_unstage_all", { projectPath });
}

/**
 * @param {string} projectPath
 * @param {string} summary
 * @param {string} [body]
 * @returns {Promise<string>} commit hash
 */
export async function gitCommit(projectPath, summary, body) {
  return invoke("git_commit", { projectPath, summary, body: body ?? null });
}

/**
 * @param {string} projectPath
 * @param {number} [limit]
 * @returns {Promise<CommitInfo[]>}
 */
export async function gitLog(projectPath, limit) {
  return invoke("git_log", { projectPath, limit: limit ?? null });
}

/** @returns {Promise<BranchList>} */
export async function gitBranches(projectPath) {
  return invoke("git_branches", { projectPath });
}

/** @returns {Promise<void>} */
export async function gitCheckoutBranch(projectPath, branch) {
  return invoke("git_checkout_branch", { projectPath, branch });
}

/**
 * @param {string} projectPath
 * @param {string} name
 * @param {string} [base] - branch/ref to base from; defaults to HEAD
 * @returns {Promise<void>}
 */
export async function gitCreateBranch(projectPath, name, base) {
  return invoke("git_create_branch", { projectPath, name, base: base ?? null });
}

/** @returns {Promise<void>} */
export async function gitFetch(projectPath) {
  return invoke("git_fetch", { projectPath });
}

/** @returns {Promise<void>} */
export async function gitPull(projectPath) {
  return invoke("git_pull", { projectPath });
}

/** @returns {Promise<void>} */
export async function gitMergeAbort(projectPath) {
  return invoke("git_merge_abort", { projectPath });
}

/** @returns {Promise<void>} */
export async function gitStashPop(projectPath) {
  return invoke("git_stash_pop", { projectPath });
}

/**
 * @param {string} projectPath
 * @param {boolean} [force]
 * @returns {Promise<void>}
 */
export async function gitPush(projectPath, force) {
  return invoke("git_push", { projectPath, force: force ?? null });
}

/** @returns {Promise<void>} */
export async function gitPublishBranch(projectPath, branch) {
  return invoke("git_publish_branch", { projectPath, branch });
}

/**
 * @param {string} projectPath
 * @param {string} branch
 * @param {boolean} [force] - true = git branch -D (force, unmerged ok)
 * @returns {Promise<void>}
 */
export async function gitDeleteBranch(projectPath, branch, force) {
  return invoke("git_delete_branch", { projectPath, branch, force: force ?? null });
}

/** @returns {Promise<FileStatus[]>} */
export async function gitCommitFiles(projectPath, hash) {
  return invoke("git_commit_files", { projectPath, hash });
}

/**
 * @param {string} projectPath
 * @param {string} hash
 * @param {string} relPath
 * @param {string} [parentHash]
 * @param {number} [maxLines]
 * @returns {Promise<DiffResult>}
 */
export async function gitDiffCommitFile(projectPath, hash, relPath, parentHash, maxLines) {
  return invoke("git_diff_commit_file", {
    projectPath, hash, relPath,
    parentHash: parentHash ?? null,
    maxLines: maxLines ?? null,
  });
}

/** @returns {Promise<ImageBlob>} */
export async function gitReadBlobWorktree(projectPath, relPath) {
  return invoke("git_read_blob_worktree", { projectPath, relPath });
}

/** @returns {Promise<ImageBlob>} */
export async function gitReadBlobHead(projectPath, relPath) {
  return invoke("git_read_blob_head", { projectPath, relPath });
}

/** @returns {Promise<ImageBlob>} */
export async function gitReadBlobAtCommit(projectPath, commitHash, relPath) {
  return invoke("git_read_blob_at_commit", { projectPath, commitHash, relPath });
}

/** @returns {Promise<RemoteStatus>} */
export async function gitRemoteStatus(projectPath) {
  return invoke("git_remote_status", { projectPath });
}

/** @returns {Promise<void>} */
export async function gitDiscardAll(projectPath) {
  return invoke("git_discard_all", { projectPath });
}

/** @returns {Promise<void>} */
export async function gitDiscardFile(projectPath, relPath) {
  return invoke("git_discard_file", { projectPath, relPath });
}

/** @returns {Promise<void>} */
export async function gitAddToGitignore(projectPath, pattern) {
  return invoke("git_add_to_gitignore", { projectPath, pattern });
}

/** @returns {Promise<void>} */
export async function openFileDefault(path) {
  return invoke("open_file_default", { path });
}

/**
 * @typedef {{ path: string, indexStatus: ChangeKind|null, worktreeStatus: ChangeKind|null, fileKind: 'text'|'image'|'binary', conflicted: boolean }} FileStatus
 * @typedef {{ type: 'added'|'modified'|'deleted'|'renamed', from?: string }} ChangeKind
 * @typedef {{ hunks: DiffHunk[], truncated: boolean, totalAdded: number, totalRemoved: number, isBinary: boolean, isImage: boolean }} DiffResult
 * @typedef {{ oldStart: number, oldLines: number, newStart: number, newLines: number, lines: DiffLine[] }} DiffHunk
 * @typedef {{ kind: 'added'|'removed'|'context', content: string, oldLineno: number|null, newLineno: number|null }} DiffLine
 * @typedef {{ hash: string, shortHash: string, summary: string, body: string|null, authorName: string, authorEmail: string, timestamp: number, parents: string[] }} CommitInfo
 * @typedef {{ name: string, isRemote: boolean }} BranchInfo
 * @typedef {{ current: string, isDetached: boolean, branches: BranchInfo[] }} BranchList
 * @typedef {{ data: string, mime: string }} ImageBlob
 * @typedef {{ ahead: number, behind: number, remoteName: string|null, remoteBranch: string|null }} RemoteStatus
 */
