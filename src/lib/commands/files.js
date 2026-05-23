import { invoke } from '@tauri-apps/api/core';

export const readProjectFile = (projectPath, relPath) =>
  invoke('read_project_file', { projectPath, relPath });

export const writeProjectFile = (projectPath, relPath, content) =>
  invoke('write_project_file', { projectPath, relPath, content });

export const listDocFiles = (projectPath) =>
  invoke('list_doc_files', { projectPath });

export const deleteDocFile = (projectPath, relPath) =>
  invoke('delete_doc_file', { projectPath, relPath });

export const readProjectFileB64 = (projectPath, relPath) =>
  invoke('read_project_file_b64', { projectPath, relPath });

export const createProjectFile = (projectPath, relPath) =>
  invoke('create_project_file', { projectPath, relPath });

export const createProjectDir = (projectPath, relPath) =>
  invoke('create_project_dir', { projectPath, relPath });

export const deleteProjectPath = (projectPath, relPath) =>
  invoke('delete_project_path', { projectPath, relPath });

export const renameProjectPath = (projectPath, oldRel, newRel) =>
  invoke('rename_project_path', { projectPath, oldRel, newRel });

export const listProjectTree = (projectPath) =>
  invoke('list_project_tree', { projectPath });
