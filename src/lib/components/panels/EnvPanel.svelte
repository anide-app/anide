<script>
  // @ts-nocheck
  import { workspace } from '$lib/stores/workspace.svelte.js';
  import { listEnvFiles, createEnvFile, deleteEnvFile, toggleEnvGitignore } from '$lib/commands/env';
  import { FileLock2, Plus, ShieldCheck, ShieldOff, Loader2, FileKey, ChevronRight, ChevronDown } from '@lucide/svelte';
  import { Button } from '$lib/components/ui/button/index.js';
  import { Input } from '$lib/components/ui/input/index.js';
  import { Label } from '$lib/components/ui/label/index.js';
  import { Badge } from '$lib/components/ui/badge/index.js';
  import { Checkbox } from '$lib/components/ui/checkbox/index.js';
  import * as Dialog from '$lib/components/ui/dialog/index.js';
  import * as ContextMenu from '$lib/components/ui/context-menu/index.js';
  import * as AlertDialog from '$lib/components/ui/alert-dialog/index.js';
  import { revealItemInDir } from '@tauri-apps/plugin-opener';

  let envFiles = $state([]);
  let listLoading = $state(false);
  let dialogOpen = $state(false);
  let newFileSuffix = $state('');
  let addToGitignore = $state(true);
  let creating = $state(false);
  let createError = $state('');

  let envClickTimer = null;

  let deleteTarget = $state(null);
  let deleteConfirmOpen = $state(false);
  let deleting = $state(false);

  let newFileName = $derived(
    newFileSuffix.trim() ? `.env.${newFileSuffix.trim()}` : '.env'
  );

  $effect(() => {
    const path = workspace.folderPath;
    const _ver = workspace.envFilesVersion;
    if (!path) { envFiles = []; return; }
    void (async () => {
      listLoading = true;
      try {
        const files = await listEnvFiles(path);
        envFiles = files;
        const t = buildTree(files);
        const addDirs = (nodes) => {
          for (const node of nodes) {
            if (node.type === 'dir') { expanded.add(node.key); addDirs(node.children); }
          }
        };
        addDirs(t);
        expanded = new Set(expanded);
      }
      catch (e) { console.error('Failed to list env files', e); }
      finally { listLoading = false; }
    })();
  });

  function openEnvFile(file) {
    workspace.openTab({
      id: `env::${file.relPath}`,
      type: 'env-file',
      title: file.name,
      data: { relPath: file.relPath, folderPath: workspace.folderPath },
    });
  }

  function absPath(relPath) {
    const base = workspace.folderPath ?? '';
    const sep = base.includes('\\') ? '\\' : '/';
    return base.replace(/[/\\]$/, '') + sep + relPath.replace(/\//g, sep);
  }

  async function copyToClipboard(text) {
    try { await navigator.clipboard.writeText(text); } catch {}
  }

  async function showInExplorer(relPath) {
    try { await revealItemInDir(absPath(relPath)); } catch (e) { console.error(e); }
  }

  async function handleToggleGitignore(file) {
    try {
      await toggleEnvGitignore(workspace.folderPath, file);
      workspace.refreshEnvFiles();
    } catch (e) { console.error(e); }
  }

  async function handleDelete() {
    if (!deleteTarget) return;
    deleting = true;
    try {
      await deleteEnvFile(workspace.folderPath, deleteTarget.relPath);
      workspace.closeTab(`env::${deleteTarget.relPath}`);
      workspace.refreshEnvFiles();
      deleteConfirmOpen = false;
      deleteTarget = null;
    } catch (e) { console.error(e); }
    finally { deleting = false; }
  }

  let expanded = $state(new Set());

  function buildTree(files) {
    const root = [];
    const dirMap = new Map();
    for (const file of files) {
      const parts = file.relPath.split('/');
      let arr = root;
      for (let i = 0; i < parts.length - 1; i++) {
        const key = parts.slice(0, i + 1).join('/');
        if (!dirMap.has(key)) {
          const node = { type: 'dir', name: parts[i], key, children: [] };
          arr.push(node);
          dirMap.set(key, node);
        }
        arr = dirMap.get(key).children;
      }
      arr.push({ type: 'file', name: parts[parts.length - 1], file });
    }
    return root;
  }

  let tree = $derived(buildTree(envFiles));

  function toggleDir(key) {
    if (expanded.has(key)) expanded.delete(key); else expanded.add(key);
    expanded = new Set(expanded);
  }

  async function handleCreate() {
    if (!workspace.folderPath) return;
    createError = '';
    creating = true;
    try {
      const relPath = newFileSuffix.trim() ? `.env.${newFileSuffix.trim()}` : '.env';
      const created = await createEnvFile(workspace.folderPath, {
        relPath,
        content: '',
        addToGitignore,
      });
      workspace.refreshEnvFiles();
      dialogOpen = false;
      newFileSuffix = '';
      addToGitignore = true;
      openEnvFile(created);
    } catch (e) {
      createError = e?.message ?? String(e);
    } finally {
      creating = false;
    }
  }
</script>

<div class="h-full flex flex-col overflow-hidden">
  <!-- Sub-header: count + add -->
  <div class="flex items-center justify-between px-3 py-2 border-b shrink-0">
    <div class="flex items-center gap-1.5">
      <FileKey class="w-3.5 h-3.5 text-muted-foreground" />
      <span class="text-xs font-medium">Files</span>
      {#if envFiles.length > 0}
        <Badge variant="secondary" class="text-xs h-4 px-1.5">{envFiles.length}</Badge>
      {/if}
    </div>
    <Button variant="ghost" size="icon" class="h-6 w-6" onclick={() => (dialogOpen = true)} title="New env file">
      <Plus class="w-3.5 h-3.5" />
    </Button>
  </div>

  <!-- File tree -->
  <div class="flex-1 overflow-y-auto py-1">
    {#if listLoading}
      <div class="flex items-center justify-center py-8 text-muted-foreground">
        <Loader2 class="w-4 h-4 animate-spin mr-2" />
        <span class="text-xs">Loading...</span>
      </div>
    {:else if envFiles.length === 0}
      <div class="flex flex-col items-center justify-center py-8 gap-2 text-muted-foreground px-3">
        <FileLock2 class="w-7 h-7 opacity-40" />
        <p class="text-xs text-center">No .env files found.<br />Create one to get started.</p>
        <Button variant="outline" size="sm" class="text-xs h-7 mt-1" onclick={() => (dialogOpen = true)}>
          <Plus class="w-3 h-3 mr-1" />New file
        </Button>
      </div>
    {:else}
      {#snippet renderNodes(nodes, depth)}
        {#each nodes as node (node.type === 'dir' ? node.key : node.file.relPath)}
          {#if node.type === 'dir'}
            <button
              type="button"
              onclick={() => toggleDir(node.key)}
              class="w-full h-7 flex items-center gap-1.5 text-sm text-muted-foreground hover:text-foreground hover:bg-muted/50 transition-colors"
              style="padding-left: {0.5 + depth * 1.1}rem; padding-right: 0.5rem"
            >
              {#if expanded.has(node.key)}
                <ChevronDown size={13} class="shrink-0" />
              {:else}
                <ChevronRight size={13} class="shrink-0" />
              {/if}
              <span class="truncate font-medium">{node.name}</span>
            </button>
            {#if expanded.has(node.key)}
              {@render renderNodes(node.children, depth + 1)}
            {/if}
          {:else}
            {@const isActive = workspace.activeTabId === `env::${node.file.relPath}`}
            <ContextMenu.Root>
              <ContextMenu.Trigger class="block w-full">
                <button
                  type="button"
                  onclick={(e) => { clearTimeout(envClickTimer); if (e.detail === 1) envClickTimer = setTimeout(() => { envClickTimer = null; openEnvFile(node.file); }, 220); }}
                  ondblclick={() => { clearTimeout(envClickTimer); envClickTimer = null; workspace.openTab({ id: `file-edit::${node.file.relPath}`, type: 'file-edit', title: node.file.name, data: { projectPath: workspace.folderPath, relPath: node.file.relPath, language: null } }); }}
                  class="w-full h-7 flex items-center justify-between gap-2 text-sm transition-colors
                    {isActive
                      ? 'bg-muted/70 text-foreground'
                      : 'text-muted-foreground hover:text-foreground hover:bg-muted/40'}"
                  style="padding-left: {0.9 + depth * 1.1}rem; padding-right: 0.5rem"
                >
                  <span class="truncate">{node.file.name}</span>
                  {#if node.file.inGitignore}
                    <ShieldCheck class="w-3.5 h-3.5 shrink-0 text-green-600/70" />
                  {:else}
                    <ShieldOff class="w-3.5 h-3.5 shrink-0 opacity-20" />
                  {/if}
                </button>
              </ContextMenu.Trigger>
              <ContextMenu.Content class="w-56">
                <ContextMenu.Item onclick={() => handleToggleGitignore(node.file)}>
                  {node.file.inGitignore ? 'Remove from .gitignore' : 'Add to .gitignore'}
                </ContextMenu.Item>
                <ContextMenu.Separator />
                <ContextMenu.Item onclick={() => copyToClipboard(absPath(node.file.relPath))}>Copy file path</ContextMenu.Item>
                <ContextMenu.Item onclick={() => copyToClipboard(node.file.relPath)}>Copy relative path</ContextMenu.Item>
                <ContextMenu.Separator />
                <ContextMenu.Item onclick={() => showInExplorer(node.file.relPath)}>Show in Explorer</ContextMenu.Item>
                <ContextMenu.Separator />
                <ContextMenu.Item
                  class="text-destructive focus:text-destructive focus:bg-destructive/10"
                  onclick={() => { deleteTarget = node.file; deleteConfirmOpen = true; }}
                >
                  Delete file
                </ContextMenu.Item>
              </ContextMenu.Content>
            </ContextMenu.Root>
          {/if}
        {/each}
      {/snippet}
      {@render renderNodes(tree, 0)}
    {/if}
  </div>

  <!-- Footer legend -->
  <div class="px-3 py-1.5 border-t shrink-0">
    <p class="text-[10px] text-muted-foreground flex items-center gap-1">
      <ShieldCheck class="w-3 h-3 text-green-600" /> in .gitignore
    </p>
  </div>
</div>

<!-- Create dialog -->
<Dialog.Root bind:open={dialogOpen}>
  <Dialog.Content class="sm:max-w-md">
    <Dialog.Header>
      <Dialog.Title>New Environment File</Dialog.Title>
      <Dialog.Description>
        Create a new <span class="font-mono text-foreground">.env</span> file in your project.
      </Dialog.Description>
    </Dialog.Header>
    <div class="space-y-5 py-2">
      <div class="space-y-2">
        <Label>Filename</Label>
        <div class="flex items-center rounded-md border focus-within:ring-2 focus-within:ring-ring overflow-hidden">
          <span class="px-3 py-2 text-sm font-mono bg-muted border-r text-muted-foreground select-none">.env.</span>
          <Input
            class="border-0 rounded-none shadow-none focus-visible:ring-0 font-mono text-sm"
            placeholder="local"
            bind:value={newFileSuffix}
            onkeydown={(e) => e.key === 'Enter' && handleCreate()}
            autofocus
          />
        </div>
        <p class="text-xs text-muted-foreground">
          Will create: <span class="font-mono text-foreground">{newFileName}</span>
        </p>
      </div>
      <div class="flex items-start gap-3">
        <Checkbox id="gitignore-check" bind:checked={addToGitignore} class="mt-0.5" />
        <div class="space-y-0.5">
          <Label for="gitignore-check" class="cursor-pointer">Add to .gitignore</Label>
          <p class="text-xs text-muted-foreground">Recommended — keeps secrets out of version control.</p>
        </div>
      </div>
      {#if createError}
        <p class="text-xs text-destructive bg-destructive/10 border border-destructive/20 rounded-md px-3 py-2">
          {createError}
        </p>
      {/if}
    </div>
    <Dialog.Footer class="gap-2">
      <Button variant="outline" onclick={() => { dialogOpen = false; createError = ''; }} disabled={creating}>
        Cancel
      </Button>
      <Button onclick={handleCreate} disabled={creating || !workspace.folderPath}>
        {#if creating}
          <Loader2 class="w-3.5 h-3.5 mr-1.5 animate-spin" />Creating...
        {:else}
          <Plus class="w-3.5 h-3.5 mr-1.5" />Create
        {/if}
      </Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>

<!-- Delete confirmation -->
<AlertDialog.Root bind:open={deleteConfirmOpen}>
  <AlertDialog.Content class="sm:max-w-sm">
    <AlertDialog.Header>
      <AlertDialog.Title>Delete {deleteTarget?.name}?</AlertDialog.Title>
      <AlertDialog.Description>
        This will permanently delete <span class="font-mono text-foreground">{deleteTarget?.relPath}</span> from disk. This cannot be undone.
      </AlertDialog.Description>
    </AlertDialog.Header>
    <AlertDialog.Footer>
      <AlertDialog.Cancel disabled={deleting}>Cancel</AlertDialog.Cancel>
      <AlertDialog.Action
        class="bg-destructive text-destructive-foreground hover:bg-destructive/90"
        disabled={deleting}
        onclick={handleDelete}
      >
        {#if deleting}<Loader2 size={13} class="mr-1.5 animate-spin inline" />{/if}
        Delete
      </AlertDialog.Action>
    </AlertDialog.Footer>
  </AlertDialog.Content>
</AlertDialog.Root>
