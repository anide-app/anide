import { describe, it, expect, vi, beforeEach } from 'vitest';

// ── Mock @tauri-apps/api/core before importing the module under test ────────
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

import { invoke } from '@tauri-apps/api/core';
import {
  readProjectFileB64,
  createProjectFile,
  createProjectDir,
  deleteProjectPath,
  renameProjectPath,
  listProjectTree,
  // Pre-existing exports (unchanged) – still validated for contract consistency
  readProjectFile,
  writeProjectFile,
  listDocFiles,
  deleteDocFile,
} from './files.js';

beforeEach(() => {
  vi.clearAllMocks();
});

// ── readProjectFileB64 ────────────────────────────────────────────────────────

describe('readProjectFileB64', () => {
  it('calls invoke with the correct command and arguments', async () => {
    invoke.mockResolvedValue('aGVsbG8=');
    const result = await readProjectFileB64('/project', 'image.png');
    expect(invoke).toHaveBeenCalledOnce();
    expect(invoke).toHaveBeenCalledWith('read_project_file_b64', {
      projectPath: '/project',
      relPath: 'image.png',
    });
    expect(result).toBe('aGVsbG8=');
  });

  it('propagates rejection from invoke', async () => {
    invoke.mockRejectedValue(new Error('Not found: image.png'));
    await expect(readProjectFileB64('/project', 'image.png')).rejects.toThrow('Not found');
  });
});

// ── createProjectFile ─────────────────────────────────────────────────────────

describe('createProjectFile', () => {
  it('calls invoke with the correct command and arguments', async () => {
    invoke.mockResolvedValue(undefined);
    const result = await createProjectFile('/project', 'src/new.js');
    expect(invoke).toHaveBeenCalledOnce();
    expect(invoke).toHaveBeenCalledWith('create_project_file', {
      projectPath: '/project',
      relPath: 'src/new.js',
    });
    expect(result).toBeUndefined();
  });

  it('propagates AlreadyExists error from invoke', async () => {
    invoke.mockRejectedValue(new Error('Already exists: src/new.js'));
    await expect(createProjectFile('/project', 'src/new.js')).rejects.toThrow('Already exists');
  });
});

// ── createProjectDir ──────────────────────────────────────────────────────────

describe('createProjectDir', () => {
  it('calls invoke with the correct command and arguments', async () => {
    invoke.mockResolvedValue(undefined);
    const result = await createProjectDir('/project', 'components');
    expect(invoke).toHaveBeenCalledOnce();
    expect(invoke).toHaveBeenCalledWith('create_project_dir', {
      projectPath: '/project',
      relPath: 'components',
    });
    expect(result).toBeUndefined();
  });

  it('propagates rejection from invoke', async () => {
    invoke.mockRejectedValue(new Error('Already exists: components'));
    await expect(createProjectDir('/project', 'components')).rejects.toThrow('Already exists');
  });
});

// ── deleteProjectPath ─────────────────────────────────────────────────────────

describe('deleteProjectPath', () => {
  it('calls invoke with the correct command and arguments', async () => {
    invoke.mockResolvedValue(undefined);
    const result = await deleteProjectPath('/project', 'old/file.txt');
    expect(invoke).toHaveBeenCalledOnce();
    expect(invoke).toHaveBeenCalledWith('delete_project_path', {
      projectPath: '/project',
      relPath: 'old/file.txt',
    });
    expect(result).toBeUndefined();
  });

  it('propagates NotFound error from invoke', async () => {
    invoke.mockRejectedValue(new Error('Not found: old/file.txt'));
    await expect(deleteProjectPath('/project', 'old/file.txt')).rejects.toThrow('Not found');
  });

  it('propagates InvalidPath error for traversal attempts', async () => {
    invoke.mockRejectedValue(new Error('Path error: ../escape.txt'));
    await expect(deleteProjectPath('/project', '../escape.txt')).rejects.toThrow('Path error');
  });
});

// ── renameProjectPath ─────────────────────────────────────────────────────────

describe('renameProjectPath', () => {
  it('calls invoke with the correct command and arguments', async () => {
    invoke.mockResolvedValue(undefined);
    const result = await renameProjectPath('/project', 'old.txt', 'new.txt');
    expect(invoke).toHaveBeenCalledOnce();
    expect(invoke).toHaveBeenCalledWith('rename_project_path', {
      projectPath: '/project',
      oldRel: 'old.txt',
      newRel: 'new.txt',
    });
    expect(result).toBeUndefined();
  });

  it('passes oldRel and newRel as separate named arguments (not relPath)', async () => {
    invoke.mockResolvedValue(undefined);
    await renameProjectPath('/project', 'a/b.txt', 'a/c.txt');
    const call = invoke.mock.calls[0];
    // Must NOT use the generic relPath key
    expect(call[1]).not.toHaveProperty('relPath');
    expect(call[1]).toHaveProperty('oldRel', 'a/b.txt');
    expect(call[1]).toHaveProperty('newRel', 'a/c.txt');
  });

  it('propagates NotFound error from invoke', async () => {
    invoke.mockRejectedValue(new Error('Not found: old.txt'));
    await expect(renameProjectPath('/project', 'old.txt', 'new.txt')).rejects.toThrow('Not found');
  });
});

// ── listProjectTree ───────────────────────────────────────────────────────────

describe('listProjectTree', () => {
  it('calls invoke with the correct command and argument', async () => {
    const fakeTree = [
      { path: 'src', is_dir: true },
      { path: 'src/main.js', is_dir: false },
    ];
    invoke.mockResolvedValue(fakeTree);
    const result = await listProjectTree('/project');
    expect(invoke).toHaveBeenCalledOnce();
    expect(invoke).toHaveBeenCalledWith('list_project_tree', {
      projectPath: '/project',
    });
    expect(result).toEqual(fakeTree);
  });

  it('returns an empty array for an empty project', async () => {
    invoke.mockResolvedValue([]);
    const result = await listProjectTree('/empty');
    expect(result).toEqual([]);
  });

  it('propagates InvalidPath error from invoke', async () => {
    invoke.mockRejectedValue(new Error('Path error: /nonexistent'));
    await expect(listProjectTree('/nonexistent')).rejects.toThrow('Path error');
  });
});

// ── Pre-existing exports – contract regression checks ─────────────────────────
// These functions were not changed by the PR but are part of the same module;
// this section guards against accidental signature regressions.

describe('pre-existing file command exports (contract regression)', () => {
  it('readProjectFile invokes correct command', async () => {
    invoke.mockResolvedValue('file content');
    await readProjectFile('/p', 'file.txt');
    expect(invoke).toHaveBeenCalledWith('read_project_file', { projectPath: '/p', relPath: 'file.txt' });
  });

  it('writeProjectFile invokes correct command with content', async () => {
    invoke.mockResolvedValue(undefined);
    await writeProjectFile('/p', 'file.txt', 'hello');
    expect(invoke).toHaveBeenCalledWith('write_project_file', { projectPath: '/p', relPath: 'file.txt', content: 'hello' });
  });

  it('listDocFiles invokes correct command', async () => {
    invoke.mockResolvedValue([]);
    await listDocFiles('/p');
    expect(invoke).toHaveBeenCalledWith('list_doc_files', { projectPath: '/p' });
  });

  it('deleteDocFile invokes correct command', async () => {
    invoke.mockResolvedValue(undefined);
    await deleteDocFile('/p', 'notes.md');
    expect(invoke).toHaveBeenCalledWith('delete_doc_file', { projectPath: '/p', relPath: 'notes.md' });
  });
});
