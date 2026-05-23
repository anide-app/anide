<script>
  // @ts-nocheck
  import { workspace } from '$lib/stores/workspace.svelte.js';
  import {
    listProjectTree, createProjectFile, createProjectDir,
    deleteProjectPath, renameProjectPath,
  } from '$lib/commands/files.js';
  import * as ContextMenu from '$lib/components/ui/context-menu/index.js';
  import * as AlertDialog from '$lib/components/ui/alert-dialog/index.js';
  import { revealItemInDir } from '@tauri-apps/plugin-opener';
  import { toast } from 'svelte-sonner';
  import { tick } from 'svelte';
  import {
    Folder, FolderOpen, FolderPlus, FilePlus,
    ChevronRight, ChevronDown, Loader2, FileCode,
    AlertTriangle, RefreshCw,
  } from '@lucide/svelte';

  let projectPath = $derived(workspace.folderPath);
  let entries  = $state([]);
  let loading  = $state(true);
  let error    = $state('');
  let expanded = $state(new Set());
  let lastPath = '';

  // ── Inline create/rename ──────────────────────────────────────────────────
  let creating    = $state(null); // { parentPath: string, type: 'file'|'dir' }
  let renaming    = $state(null); // { path: string }
  let inputVal    = $state('');
  let inputEl     = $state(null);
  let contextTarget = $state(null); // entry | null (for panel bg)

  // ── Delete confirmation ────────────────────────────────────────────────────
  let deleteTarget      = $state(null); // { path: string, isDir: bool }
  let deleteConfirmOpen = $state(false);
  let deleting          = $state(false);

  // ── Load ──────────────────────────────────────────────────────────────────
  async function load(resetExpanded) {
    if (!projectPath) { entries = []; loading = false; return; }
    if (resetExpanded) loading = true;
    error = '';
    try {
      const result = await listProjectTree(projectPath);
      entries = result;
      if (resetExpanded) {
        const next = new Set();
        for (const e of result) {
          if (e.is_dir && !e.path.includes('/')) next.add(e.path);
        }
        expanded = next;
      }
    } catch (e) {
      error = String(e);
    } finally {
      if (resetExpanded) loading = false;
    }
  }

  $effect(() => {
    const p = projectPath;
    const fresh = p !== lastPath;
    lastPath = p ?? '';
    void load(fresh);
  });

  $effect(() => {
    workspace.worktreeChangeTick;
    void load(false);
  });

  // ── Helpers ───────────────────────────────────────────────────────────────
  function absPath(relPath) {
    const base = projectPath ?? '';
    const sep = base.includes('\\') ? '\\' : '/';
    return base.replace(/[/\\]$/, '') + sep + relPath.replace(/\//g, sep);
  }

  async function copyToClipboard(text) {
    try { await navigator.clipboard.writeText(text); toast.success('Copied'); } catch {}
  }

  async function showInExplorer(relPath) {
    try { await revealItemInDir(absPath(relPath)); } catch {}
  }

  function isVisible(path) {
    const parts = path.split('/');
    for (let i = 1; i < parts.length; i++) {
      if (!expanded.has(parts.slice(0, i).join('/'))) return false;
    }
    return true;
  }

  function isOpen(relPath) {
    return workspace.tabs.some(t => t.id === `file-edit::${relPath}`);
  }

  function toggle(path) {
    const next = new Set(expanded);
    if (next.has(path)) next.delete(path); else next.add(path);
    expanded = next;
  }

  function openFile(relPath, name) {
    workspace.openTab({
      id: `file-edit::${relPath}`,
      type: 'file-edit',
      title: name,
      data: { projectPath, relPath, language: null },
    });
  }

  // ── Inline create ─────────────────────────────────────────────────────────
  function startCreate(parentPath, type) {
    renaming = null;
    creating = { parentPath, type };
    inputVal = '';
    if (parentPath) {
      const next = new Set(expanded);
      next.add(parentPath);
      expanded = next;
    }
    tick().then(() => inputEl?.focus());
  }

  async function commitCreate() {
    if (!creating || !inputVal.trim()) { creating = null; return; }
    const { parentPath, type } = creating;
    const name = inputVal.trim();
    const relPath = parentPath ? `${parentPath}/${name}` : name;
    creating = null; inputVal = '';
    try {
      if (type === 'file') {
        await createProjectFile(projectPath, relPath);
        openFile(relPath, name);
      } else {
        await createProjectDir(projectPath, relPath);
      }
      await load(false);
    } catch (e) { toast.error(e?.message ?? String(e)); }
  }

  // ── Inline rename ─────────────────────────────────────────────────────────
  function startRename(path) {
    creating = null;
    renaming = { path };
    inputVal = path.split('/').pop() ?? path;
    tick().then(() => { inputEl?.focus(); inputEl?.select(); });
  }

  async function commitRename() {
    if (!renaming || !inputVal.trim()) { renaming = null; return; }
    const { path } = renaming;
    const dir = path.includes('/') ? path.slice(0, path.lastIndexOf('/')) : '';
    const newRel = dir ? `${dir}/${inputVal.trim()}` : inputVal.trim();
    renaming = null; inputVal = '';
    if (newRel === path) return;
    try {
      await renameProjectPath(projectPath, path, newRel);
      workspace.closeTab(`file-edit::${path}`);
      await load(false);
    } catch (e) { toast.error(e?.message ?? String(e)); }
  }

  function cancelInput() { creating = null; renaming = null; inputVal = ''; }

  function handleInputKey(e) {
    if (e.key === 'Enter')  { e.preventDefault(); creating ? commitCreate() : commitRename(); }
    if (e.key === 'Escape') { e.preventDefault(); cancelInput(); }
  }

  // ── Delete ────────────────────────────────────────────────────────────────
  async function handleDelete() {
    if (!deleteTarget) return;
    deleting = true;
    try {
      await deleteProjectPath(projectPath, deleteTarget.path);
      if (deleteTarget.isDir) {
        for (const t of workspace.tabs) {
          if (t.type === 'file-edit' && t.data?.relPath?.startsWith(deleteTarget.path + '/')) {
            workspace.closeTab(t.id);
          }
        }
      } else {
        workspace.closeTab(`file-edit::${deleteTarget.path}`);
      }
      deleteConfirmOpen = false;
      deleteTarget = null;
      toast.success('Deleted');
      await load(false);
    } catch (e) { toast.error(e?.message ?? String(e)); }
    finally { deleting = false; }
  }

  // ── Display list (entries + virtual create placeholder) ───────────────────
  let displayItems = $derived.by(() => {
    const visible = entries.filter(e => isVisible(e.path));
    if (!creating) return visible;

    const { parentPath } = creating;
    let insertIdx = 0;
    for (let i = 0; i < visible.length; i++) {
      const p = visible[i].path;
      const inParent = parentPath === ''
        ? !p.includes('/')
        : p.startsWith(parentPath + '/');
      if (inParent) insertIdx = i + 1;
    }
    const virt = { path: '__create__', is_dir: creating.type === 'dir', _virt: true };
    return [...visible.slice(0, insertIdx), virt, ...visible.slice(insertIdx)];
  });
</script>

<!-- Outer context menu: right-click anywhere in the panel -->
<ContextMenu.Root>
  <ContextMenu.Trigger class="flex-1 block h-full">
    <div
      class="h-full overflow-y-auto select-none"
      oncontextmenu={() => (contextTarget = null)}
    >
      {#if loading}
        <div class="flex items-center justify-center py-8 text-muted-foreground">
          <Loader2 size={14} class="animate-spin" />
        </div>
      {:else if error}
        <div class="flex items-center gap-2 p-3 text-destructive text-xs">
          <AlertTriangle size={13} class="shrink-0" />{error}
        </div>
      {:else}
        {#if entries.length === 0 && !creating}
          <p class="p-4 text-muted-foreground text-xs text-center">Empty folder</p>
        {/if}

        {#each displayItems as item (item.path)}
          {@const parts = item.path.split('/')}
          {@const name  = parts[parts.length - 1]}
          {@const depth = item._virt ? (creating.parentPath ? creating.parentPath.split('/').length : 0) : parts.length - 1}

          {#if item._virt}
            <!-- Inline create input -->
            <div
              class="flex items-center gap-1 py-[3px] pr-2"
              style="padding-left: {8 + depth * 12}px"
            >
              {#if creating.type === 'dir'}
                <FolderPlus size={13} class="shrink-0 text-amber-400/80" />
              {:else}
                <FilePlus size={13} class="shrink-0 opacity-60" />
              {/if}
              <input
                bind:this={inputEl}
                bind:value={inputVal}
                onkeydown={handleInputKey}
                onblur={cancelInput}
                placeholder={creating.type === 'dir' ? 'folder name' : 'file name'}
                class="flex-1 min-w-0 bg-muted/60 border border-primary/40 rounded px-1.5 py-0 text-xs
                       outline-none focus:border-primary text-foreground placeholder:text-muted-foreground/50"
              />
            </div>

          {:else if renaming?.path === item.path}
            <!-- Inline rename input -->
            <div
              class="flex items-center gap-1 py-[3px] pr-2"
              style="padding-left: {8 + depth * 12}px"
            >
              {#if item.is_dir}
                <FolderOpen size={13} class="shrink-0 text-amber-400/80" />
              {:else}
                <FileCode size={13} class="shrink-0 opacity-50" />
              {/if}
              <input
                bind:this={inputEl}
                bind:value={inputVal}
                onkeydown={handleInputKey}
                onblur={commitRename}
                class="flex-1 min-w-0 bg-muted/60 border border-primary/40 rounded px-1.5 py-0 text-xs
                       outline-none focus:border-primary text-foreground"
              />
            </div>

          {:else}
            <!-- Normal entry with per-item context menu -->
            <ContextMenu.Root>
              <ContextMenu.Trigger
                class="block w-full"
                oncontextmenu={(e) => { e.stopPropagation(); contextTarget = item; }}
              >
                {@const active = !item.is_dir && isOpen(item.path)}
                <button
                  type="button"
                  onclick={() => item.is_dir ? toggle(item.path) : openFile(item.path, name)}
                  class="w-full flex items-center gap-1 py-[3px] pr-2 text-xs text-left transition-colors
                    {active ? 'text-foreground bg-muted/60' : 'text-muted-foreground hover:text-foreground hover:bg-muted/40'}"
                  style="padding-left: {8 + depth * 12}px"
                >
                  {#if item.is_dir}
                    {#if expanded.has(item.path)}
                      <ChevronDown  size={12} class="shrink-0 opacity-50" />
                      <FolderOpen   size={13} class="shrink-0 text-amber-400/80" />
                    {:else}
                      <ChevronRight size={12} class="shrink-0 opacity-50" />
                      <Folder       size={13} class="shrink-0 text-amber-400/80" />
                    {/if}
                  {:else}
                    <span class="w-3 shrink-0"></span>
                    <FileCode size={13} class="shrink-0 {active ? 'text-primary' : 'opacity-50'}" />
                  {/if}
                  <span class="truncate {active ? 'font-medium' : ''}">{name}</span>
                </button>
              </ContextMenu.Trigger>

              <ContextMenu.Content class="w-52">
                {#if item.is_dir}
                  <ContextMenu.Item onclick={() => startCreate(item.path, 'file')}>
                    <FilePlus size={13} class="mr-2 opacity-60" />New File
                  </ContextMenu.Item>
                  <ContextMenu.Item onclick={() => startCreate(item.path, 'dir')}>
                    <FolderPlus size={13} class="mr-2 opacity-60" />New Folder
                  </ContextMenu.Item>
                  <ContextMenu.Separator />
                {:else}
                  <ContextMenu.Item onclick={() => openFile(item.path, name)}>Open</ContextMenu.Item>
                  <ContextMenu.Separator />
                {/if}
                <ContextMenu.Item onclick={() => copyToClipboard(absPath(item.path))}>
                  Copy path
                </ContextMenu.Item>
                <ContextMenu.Item onclick={() => copyToClipboard(item.path)}>
                  Copy relative path
                </ContextMenu.Item>
                <ContextMenu.Separator />
                <ContextMenu.Item onclick={() => showInExplorer(item.path)}>
                  Show in Explorer
                </ContextMenu.Item>
                <ContextMenu.Separator />
                <ContextMenu.Item onclick={() => startRename(item.path)}>Rename</ContextMenu.Item>
                <ContextMenu.Separator />
                <ContextMenu.Item
                  class="text-destructive focus:text-destructive focus:bg-destructive/10"
                  onclick={() => { deleteTarget = { path: item.path, isDir: item.is_dir }; deleteConfirmOpen = true; }}
                >
                  Delete
                </ContextMenu.Item>
              </ContextMenu.Content>
            </ContextMenu.Root>
          {/if}
        {/each}
      {/if}
    </div>
  </ContextMenu.Trigger>

  <!-- Panel background context menu -->
  <ContextMenu.Content class="w-52">
    <ContextMenu.Item onclick={() => startCreate('', 'file')}>
      <FilePlus size={13} class="mr-2 opacity-60" />New File
    </ContextMenu.Item>
    <ContextMenu.Item onclick={() => startCreate('', 'dir')}>
      <FolderPlus size={13} class="mr-2 opacity-60" />New Folder
    </ContextMenu.Item>
    <ContextMenu.Separator />
    <ContextMenu.Item onclick={() => load(false)}>
      <RefreshCw size={13} class="mr-2 opacity-60" />Refresh
    </ContextMenu.Item>
  </ContextMenu.Content>
</ContextMenu.Root>

<!-- Delete confirmation -->
<AlertDialog.Root bind:open={deleteConfirmOpen}>
  <AlertDialog.Content class="sm:max-w-sm">
    <AlertDialog.Header>
      <AlertDialog.Title>
        Delete {deleteTarget?.path.split('/').pop()}?
      </AlertDialog.Title>
      <AlertDialog.Description>
        {deleteTarget?.isDir
          ? 'This will permanently delete the folder and all its contents.'
          : 'This file will be permanently deleted.'}
      </AlertDialog.Description>
    </AlertDialog.Header>
    <AlertDialog.Footer>
      <AlertDialog.Cancel disabled={deleting}>Cancel</AlertDialog.Cancel>
      <AlertDialog.Action
        onclick={handleDelete}
        disabled={deleting}
        class="bg-destructive text-destructive-foreground hover:bg-destructive/90"
      >
        {#if deleting}<Loader2 size={13} class="mr-1.5 animate-spin inline" />{/if}
        Delete
      </AlertDialog.Action>
    </AlertDialog.Footer>
  </AlertDialog.Content>
</AlertDialog.Root>
