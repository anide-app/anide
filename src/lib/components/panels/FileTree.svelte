<script>
  // @ts-nocheck
  /**
   * Unified file tree used by docs, env, and git modes.
   *
   * Node shape expected from the parent:
   *   dir:  { type:'dir',  name, path, children, count? }
   *   file: { type:'file', name, path, gitFile?, envFile? }
   *
   * gitStatusMap (Map<path, statusType>) is used by docs/env modes to overlay
   * git status dots without requiring a full gitFile object.
   */
  let {
    nodes = [],
    mode,           // 'git' | 'docs' | 'env'
    gitStatusMap = null,
    activeFile = null,
    projectPath,
    onRefresh,      // () => void — called after create/rename/delete (docs mode)
    // git mode callbacks
    onFileClick,
    onToggle,
    onDiscard,
    onGitignore,
    // env callbacks
    onDelete,
    onToggleGitignore,
  } = $props();

  import { ChevronRight, ChevronDown, Folder, FolderOpen, FileText, ShieldCheck, ShieldOff, FilePlus, FolderPlus, Loader2 } from '@lucide/svelte';
  import * as ContextMenu from '$lib/components/ui/context-menu/index.js';
  import * as AlertDialog from '$lib/components/ui/alert-dialog/index.js';
  import { gitDiscardFile, gitAddToGitignore, openFileDefault } from '$lib/commands/git.js';
  import { createProjectFile, createProjectDir, deleteProjectPath, renameProjectPath } from '$lib/commands/files.js';
  import { revealItemInDir } from '@tauri-apps/plugin-opener';
  import { workspace } from '$lib/stores/workspace.svelte.js';
  import { tick } from 'svelte';
  import { toast } from 'svelte-sonner';

  // ── Virtual scroll ────────────────────────────────────────────────────────────
  const ROW_H = 28;
  const OVER  = 10;

  let scrollEl  = $state(null);
  let scrollTop = $state(0);
  let vpH       = $state(500);

  $effect(() => {
    if (!scrollEl) return;
    vpH = scrollEl.clientHeight;
    const ro = new ResizeObserver(() => { vpH = scrollEl.clientHeight; });
    ro.observe(scrollEl);
    return () => ro.disconnect();
  });

  // ── Tree collapse ─────────────────────────────────────────────────────────────
  let collapsed = $state(new Set());
  function toggleDir(path) {
    const next = new Set(collapsed);
    next.has(path) ? next.delete(path) : next.add(path);
    collapsed = next;
  }

  // ── Docs mode: create / rename / delete ───────────────────────────────────────
  let creating        = $state(null);  // { parentPath: string, type: 'file'|'dir' }
  let renaming        = $state(null);  // { path: string, isDir: boolean }
  let inputVal        = $state('');
  let inputEl         = $state(null);
  let docDeleteTarget = $state(null);  // { path: string, isDir: boolean }
  let docDeleteOpen   = $state(false);
  let docDeleting     = $state(false);

  function flatten(nodes, depth, out = []) {
    for (const node of nodes) {
      if (renaming?.path === node.path) {
        out.push({ node: { ...node, _renaming: true }, depth });
      } else {
        out.push({ node, depth });
      }
      if (node.type === 'dir' && !collapsed.has(node.path)) {
        flatten(node.children, depth + 1, out);
        if (creating?.parentPath === node.path) {
          out.push({ node: { type: '_create', path: '__create__', createType: creating.type }, depth: depth + 1 });
        }
      }
    }
    return out;
  }

  let flat = $derived.by(() => {
    const items = flatten(nodes, 0);
    if (creating?.parentPath === '') {
      items.push({ node: { type: '_create', path: '__create__', createType: creating.type }, depth: 0 });
    }
    return items;
  });

  let totalH   = $derived(flat.length * ROW_H);
  let startIdx = $derived(Math.max(0, Math.floor(scrollTop / ROW_H) - OVER));
  let endIdx   = $derived(Math.min(flat.length, Math.ceil((scrollTop + vpH) / ROW_H) + OVER));
  let visible  = $derived(flat.slice(startIdx, endIdx));
  let padTop   = $derived(startIdx * ROW_H);

  function startCreate(parentPath, type) {
    renaming = null;
    creating = { parentPath, type };
    inputVal = '';
    if (parentPath) {
      const next = new Set(collapsed);
      next.delete(parentPath);
      collapsed = next;
    }
    tick().then(() => inputEl?.focus());
  }

  async function commitCreate() {
    if (!creating || !inputVal.trim()) { creating = null; return; }
    const { parentPath, type } = creating;
    let name = inputVal.trim();
    if (type === 'file' && mode === 'docs' && !name.includes('.')) name += '.md';
    const relPath = parentPath ? `${parentPath}/${name}` : name;
    creating = null; inputVal = '';
    try {
      if (type === 'file') {
        await createProjectFile(projectPath, relPath);
        if (mode === 'docs') {
          workspace.openTab({ id: `doc::${relPath}`, type: 'doc', title: name, data: { relPath, folderPath: projectPath } });
        }
      } else {
        await createProjectDir(projectPath, relPath);
      }
      onRefresh?.();
    } catch (e) { toast.error(e?.message ?? String(e)); }
  }

  function startRename(path, isDir) {
    creating = null;
    renaming = { path, isDir };
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
      workspace.closeTab(`doc::${path}`);
      onRefresh?.();
    } catch (e) { toast.error(e?.message ?? String(e)); }
  }

  function cancelInput() { creating = null; renaming = null; inputVal = ''; }

  function handleInputKey(e) {
    if (e.key === 'Enter')  { e.preventDefault(); creating ? commitCreate() : commitRename(); }
    if (e.key === 'Escape') { e.preventDefault(); cancelInput(); }
  }

  async function handleDocsDelete() {
    if (!docDeleteTarget) return;
    docDeleting = true;
    try {
      await deleteProjectPath(projectPath, docDeleteTarget.path);
      if (docDeleteTarget.isDir) {
        workspace.tabs
          .filter(t => t.type === 'doc' && t.data?.relPath?.startsWith(docDeleteTarget.path + '/'))
          .forEach(t => workspace.closeTab(t.id));
      } else {
        workspace.closeTab(`doc::${docDeleteTarget.path}`);
      }
      docDeleteOpen = false;
      docDeleteTarget = null;
      onRefresh?.();
      toast.success('Deleted');
    } catch (e) { toast.error(e?.message ?? String(e)); }
    finally { docDeleting = false; }
  }

  // ── Status colours ────────────────────────────────────────────────────────────
  const DOT_COLOR  = { added: 'bg-green-500', modified: 'bg-yellow-400', deleted: 'bg-red-500', renamed: 'bg-blue-400' };
  const NAME_COLOR = { added: 'text-green-600 dark:text-green-400', modified: '', deleted: 'text-red-500 line-through opacity-60', renamed: 'text-blue-500' };

  function statusType(node) {
    if (node.gitFile) return node.gitFile.indexStatus?.type ?? node.gitFile.worktreeStatus?.type;
    if (gitStatusMap) return gitStatusMap.get(node.path) ?? null;
    return null;
  }

  function dotColor(node) {
    if (node.gitFile?.conflicted) return 'bg-red-500';
    const t = statusType(node);
    return t ? (DOT_COLOR[t] ?? 'bg-muted-foreground/40') : null;
  }

  // ── Helpers ───────────────────────────────────────────────────────────────────
  function guideX(i)       { return 16 + i * 14; }
  function getExt(name)    { const d = name.lastIndexOf('.'); return d > 0 ? name.slice(d + 1) : ''; }
  function getDirPart(rel) { const s = rel.lastIndexOf('/'); return s > 0 ? rel.slice(0, s) : ''; }
  function absPath(rel) {
    const sep = projectPath.includes('\\') ? '\\' : '/';
    return projectPath.replace(/[/\\]$/, '') + sep + rel.replace(/\//g, sep);
  }

  async function copyToClipboard(text) { try { await navigator.clipboard.writeText(text); } catch {} }
  async function showInExplorer(rel)   { try { await revealItemInDir(absPath(rel)); }         catch {} }
  async function openWithDefault(rel)  { try { await openFileDefault(absPath(rel)); }          catch {} }

  function openInFileViewer(relPath) {
    workspace.openTab({
      id: `file-edit::${relPath}`,
      type: 'file-edit',
      title: relPath.split('/').pop() ?? relPath,
      data: { projectPath, relPath, language: null },
    });
  }

  // ── Git-mode actions ──────────────────────────────────────────────────────────
  async function handleDiscard(node) {
    const target = node.gitFile ?? node;
    try { await gitDiscardFile(projectPath, target.path ?? node.path); onDiscard?.(target); }
    catch (e) { console.error('discard', e); }
  }

  async function handleIgnore(pattern) {
    try { await gitAddToGitignore(projectPath, pattern); onGitignore?.(); }
    catch (e) { console.error('gitignore', e); }
  }
</script>

<!-- ── Inline input snippets ────────────────────────────────────────────────── -->

{#snippet createInputRow(node, depth)}
  <div
    class="relative flex items-center gap-1.5 h-7"
    style="padding-left: {10 + depth * 14}px; padding-right: 16px;"
  >
    {#each Array.from({ length: depth }) as _, i}
      <span class="absolute top-0 bottom-0 w-px bg-border/50 pointer-events-none" style="left: {guideX(i)}px;"></span>
    {/each}
    {#if node.createType === 'dir'}
      <FolderPlus size={12} class="shrink-0 text-amber-400/80" />
    {:else}
      <FilePlus size={12} class="shrink-0 opacity-60" />
    {/if}
    <input
      bind:this={inputEl}
      bind:value={inputVal}
      onkeydown={handleInputKey}
      onblur={cancelInput}
      placeholder={node.createType === 'dir' ? 'folder name' : 'doc name'}
      class="flex-1 min-w-0 bg-muted/60 border border-primary/40 rounded px-1.5 py-0 text-xs
             outline-none focus:border-primary text-foreground placeholder:text-muted-foreground/50"
    />
  </div>
{/snippet}

{#snippet renameInputRow(node, depth)}
  <div
    class="relative flex items-center gap-1.5 h-7"
    style="padding-left: {10 + depth * 14}px; padding-right: 16px;"
  >
    {#each Array.from({ length: depth }) as _, i}
      <span class="absolute top-0 bottom-0 w-px bg-border/50 pointer-events-none" style="left: {guideX(i)}px;"></span>
    {/each}
    {#if node.type === 'dir'}
      <FolderOpen size={13} class="shrink-0 opacity-60 text-amber-400/80" />
    {:else}
      <FileText size={12} class="shrink-0 opacity-50" />
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
{/snippet}

<!-- ── Row snippets ──────────────────────────────────────────────────────────── -->

{#snippet fileRow(node, depth)}
  {@const dc      = dotColor(node)}
  {@const gitFile = node.gitFile}
  {@const envFile = node.envFile}
  {@const staged  = gitFile ? !!gitFile.indexStatus : false}
  {@const isActive = activeFile === node.path}
  {@const ext     = getExt(node.name)}
  {@const dir     = getDirPart(node.path)}
  {@const nameKind = mode === 'git' ? (gitFile?.indexStatus?.type ?? gitFile?.worktreeStatus?.type) : null}

  <ContextMenu.Root>
    <ContextMenu.Trigger class="block w-full">
      <div
        class="relative flex items-center gap-1.5 h-7 group transition-colors
          {isActive ? 'bg-muted' : 'hover:bg-muted/50'}"
        style="padding-left: {10 + depth * 14}px; padding-right: 16px;"
        role="none"
      >
        {#each Array.from({ length: depth }) as _, i}
          <span class="absolute top-0 bottom-0 w-px bg-border/50 pointer-events-none" style="left: {guideX(i)}px;"></span>
        {/each}

        {#if mode === 'docs'}
          <FileText size={12} class="shrink-0 opacity-50" />
        {/if}

        <button
          type="button"
          class="flex-1 text-left text-[13px] truncate min-w-0 transition-colors
            {gitFile?.conflicted
              ? 'text-red-500 dark:text-red-400'
              : isActive
                ? 'text-foreground'
                : 'text-muted-foreground group-hover:text-foreground'}
            {gitFile?.conflicted ? '' : (NAME_COLOR[nameKind] ?? '')}"
          onclick={() => onFileClick?.(node)}
          ondblclick={(e) => { e.stopPropagation(); openInFileViewer(node.path); }}
        >
          {node.name}{#if gitFile?.conflicted}<span class="ml-1 text-[10px] font-bold opacity-70">!!</span>{/if}
        </button>

        {#if mode === 'env' && envFile}
          {#if envFile.inGitignore}
            <ShieldCheck size={12} class="shrink-0 text-green-600/70" />
          {:else}
            <ShieldOff size={12} class="shrink-0 opacity-20" />
          {/if}
        {/if}

        {#if dc}
          <span class="shrink-0 w-2 h-2 rounded-full {dc}" title={gitFile?.conflicted ? 'merge conflict' : statusType(node)}></span>
        {/if}

        {#if mode === 'git' && gitFile}
          <button
            type="button"
            aria-label="{staged ? 'Unstage' : 'Stage'} {node.path}"
            onclick={(e) => { e.stopPropagation(); onToggle?.(gitFile, !staged); }}
            class="shrink-0 w-3.5 h-3.5 ml-1 rounded-[3px] border flex items-center justify-center transition-all
              {staged
                ? 'bg-primary border-primary text-primary-foreground'
                : 'border-muted-foreground/30 hover:border-primary/70 group-hover:border-muted-foreground/60'}"
          >
            {#if staged}
              <svg viewBox="0 0 10 10" class="w-2.5 h-2.5" fill="none" stroke="currentColor" stroke-width="2.2">
                <polyline points="1.5,5 4,8 8.5,2" />
              </svg>
            {/if}
          </button>
        {/if}
      </div>
    </ContextMenu.Trigger>

    <ContextMenu.Content class="w-56">
      {#if mode === 'git'}
        <ContextMenu.Item class="text-destructive focus:text-destructive focus:bg-destructive/10" onclick={() => handleDiscard(node)}>
          Discard changes
        </ContextMenu.Item>
        <ContextMenu.Separator />
        <ContextMenu.Item onclick={() => handleIgnore(node.path)}>Ignore file</ContextMenu.Item>
        {#if dir}<ContextMenu.Item onclick={() => handleIgnore(dir + '/')}>Ignore folder</ContextMenu.Item>{/if}
        {#if ext}<ContextMenu.Item onclick={() => handleIgnore('*.' + ext)}>Ignore all .{ext} files</ContextMenu.Item>{/if}
        <ContextMenu.Separator />
        <ContextMenu.Item onclick={() => copyToClipboard(absPath(node.path))}>Copy file path</ContextMenu.Item>
        <ContextMenu.Item onclick={() => copyToClipboard(node.path)}>Copy relative path</ContextMenu.Item>
        <ContextMenu.Separator />
        <ContextMenu.Item onclick={() => showInExplorer(node.path)}>Show in Explorer</ContextMenu.Item>
        <ContextMenu.Item onclick={() => openWithDefault(node.path)}>Open with default program</ContextMenu.Item>
      {:else if mode === 'docs'}
        <ContextMenu.Item onclick={() => onFileClick?.(node)}>Open</ContextMenu.Item>
        <ContextMenu.Separator />
        <ContextMenu.Item onclick={() => startRename(node.path, false)}>Rename</ContextMenu.Item>
        <ContextMenu.Separator />
        <ContextMenu.Item onclick={() => copyToClipboard(absPath(node.path))}>Copy file path</ContextMenu.Item>
        <ContextMenu.Item onclick={() => copyToClipboard(node.path)}>Copy relative path</ContextMenu.Item>
        <ContextMenu.Separator />
        <ContextMenu.Item onclick={() => showInExplorer(node.path)}>Show in Explorer</ContextMenu.Item>
        <ContextMenu.Separator />
        <ContextMenu.Item
          class="text-destructive focus:text-destructive focus:bg-destructive/10"
          onclick={() => { docDeleteTarget = { path: node.path, isDir: false }; docDeleteOpen = true; }}
        >
          Delete file
        </ContextMenu.Item>
      {:else if mode === 'env'}
        <ContextMenu.Item onclick={() => onToggleGitignore?.(envFile)}>
          {envFile?.inGitignore ? 'Remove from .gitignore' : 'Add to .gitignore'}
        </ContextMenu.Item>
        <ContextMenu.Separator />
        <ContextMenu.Item onclick={() => copyToClipboard(absPath(node.path))}>Copy file path</ContextMenu.Item>
        <ContextMenu.Item onclick={() => copyToClipboard(node.path)}>Copy relative path</ContextMenu.Item>
        <ContextMenu.Separator />
        <ContextMenu.Item onclick={() => showInExplorer(node.path)}>Show in Explorer</ContextMenu.Item>
        <ContextMenu.Separator />
        <ContextMenu.Item
          class="text-destructive focus:text-destructive focus:bg-destructive/10"
          onclick={() => onDelete?.(envFile)}
        >
          Delete file
        </ContextMenu.Item>
      {/if}
    </ContextMenu.Content>
  </ContextMenu.Root>
{/snippet}

{#snippet dirRow(node, depth)}
  {@const open = !collapsed.has(node.path)}
  <ContextMenu.Root>
    <ContextMenu.Trigger class="block w-full">
      <div class="relative h-7" role="none">
        {#each Array.from({ length: depth }) as _, i}
          <span class="absolute top-0 bottom-0 w-px bg-border/50 pointer-events-none" style="left: {guideX(i)}px;"></span>
        {/each}
        <button
          type="button"
          class="w-full h-full flex items-center gap-1.5 hover:bg-muted/50 text-muted-foreground hover:text-foreground transition-colors text-[13px] select-none"
          style="padding-left: {10 + depth * 14}px; padding-right: 16px;"
          onclick={() => toggleDir(node.path)}
        >
          {#if open}<FolderOpen size={13} class="shrink-0 opacity-60" />{:else}<Folder size={13} class="shrink-0 opacity-60" />{/if}
          <span class="shrink-0 font-medium">{node.name}</span>
          {#if node.count > 0}<span class="text-[11px] opacity-50 shrink-0 ml-1">{node.count}</span>{/if}
          <span class="flex-1"></span>
          {#if open}<ChevronDown size={11} class="shrink-0 opacity-40" />{:else}<ChevronRight size={11} class="shrink-0 opacity-40" />{/if}
        </button>
      </div>
    </ContextMenu.Trigger>

    {#if mode === 'git'}
      <ContextMenu.Content class="w-56">
        <ContextMenu.Item onclick={() => handleIgnore(node.path + '/')}>Ignore folder</ContextMenu.Item>
        <ContextMenu.Separator />
        <ContextMenu.Item onclick={() => copyToClipboard(absPath(node.path))}>Copy folder path</ContextMenu.Item>
        <ContextMenu.Item onclick={() => copyToClipboard(node.path)}>Copy relative path</ContextMenu.Item>
        <ContextMenu.Separator />
        <ContextMenu.Item onclick={() => showInExplorer(node.path)}>Show in Explorer</ContextMenu.Item>
      </ContextMenu.Content>
    {:else if mode === 'docs'}
      <ContextMenu.Content class="w-56">
        <ContextMenu.Item onclick={() => startCreate(node.path, 'file')}>
          <FilePlus size={13} class="mr-2 opacity-60" />New Doc
        </ContextMenu.Item>
        <ContextMenu.Item onclick={() => startCreate(node.path, 'dir')}>
          <FolderPlus size={13} class="mr-2 opacity-60" />New Folder
        </ContextMenu.Item>
        <ContextMenu.Separator />
        <ContextMenu.Item onclick={() => startRename(node.path, true)}>Rename</ContextMenu.Item>
        <ContextMenu.Separator />
        <ContextMenu.Item onclick={() => copyToClipboard(absPath(node.path))}>Copy folder path</ContextMenu.Item>
        <ContextMenu.Item onclick={() => copyToClipboard(node.path)}>Copy relative path</ContextMenu.Item>
        <ContextMenu.Separator />
        <ContextMenu.Item onclick={() => showInExplorer(node.path)}>Show in Explorer</ContextMenu.Item>
        <ContextMenu.Separator />
        <ContextMenu.Item
          class="text-destructive focus:text-destructive focus:bg-destructive/10"
          onclick={() => { docDeleteTarget = { path: node.path, isDir: true }; docDeleteOpen = true; }}
        >
          Delete folder
        </ContextMenu.Item>
      </ContextMenu.Content>
    {/if}
  </ContextMenu.Root>
{/snippet}

<!-- ── Virtual scroll container ─────────────────────────────────────────────── -->
<ContextMenu.Root>
  <ContextMenu.Trigger class="block w-full h-full">
    <div
      bind:this={scrollEl}
      class="w-full h-full overflow-y-auto overflow-x-hidden"
      onscroll={(e) => { scrollTop = e.currentTarget.scrollTop; }}
    >
      <div style="height: {totalH}px; position: relative;">
        <div style="position: absolute; top: {padTop}px; left: 0; right: 0;">
          {#each visible as { node, depth } (`${node.path}::${depth}`)}
            {#if node.type === '_create'}
              {@render createInputRow(node, depth)}
            {:else if node._renaming}
              {@render renameInputRow(node, depth)}
            {:else if node.type === 'dir'}
              {@render dirRow(node, depth)}
            {:else}
              {@render fileRow(node, depth)}
            {/if}
          {/each}
        </div>
      </div>
    </div>
  </ContextMenu.Trigger>
  {#if mode === 'docs'}
    <ContextMenu.Content class="w-52">
      <ContextMenu.Item onclick={() => startCreate('', 'file')}>
        <FilePlus size={13} class="mr-2 opacity-60" />New Doc
      </ContextMenu.Item>
      <ContextMenu.Item onclick={() => startCreate('', 'dir')}>
        <FolderPlus size={13} class="mr-2 opacity-60" />New Folder
      </ContextMenu.Item>
    </ContextMenu.Content>
  {/if}
</ContextMenu.Root>

<!-- Docs mode: delete confirmation -->
{#if mode === 'docs'}
  <AlertDialog.Root bind:open={docDeleteOpen}>
    <AlertDialog.Content class="sm:max-w-sm">
      <AlertDialog.Header>
        <AlertDialog.Title>Delete {docDeleteTarget?.path.split('/').pop()}?</AlertDialog.Title>
        <AlertDialog.Description>
          {#if docDeleteTarget?.isDir}
            This will permanently delete the folder and all its contents.
          {:else}
            This will permanently delete <span class="font-mono text-foreground">{docDeleteTarget?.path}</span> from disk.
          {/if}
          This cannot be undone.
        </AlertDialog.Description>
      </AlertDialog.Header>
      <AlertDialog.Footer>
        <AlertDialog.Cancel disabled={docDeleting}>Cancel</AlertDialog.Cancel>
        <AlertDialog.Action
          class="bg-destructive text-destructive-foreground hover:bg-destructive/90"
          disabled={docDeleting}
          onclick={handleDocsDelete}
        >
          {#if docDeleting}<Loader2 size={13} class="mr-1.5 animate-spin inline" />{/if}
          Delete
        </AlertDialog.Action>
      </AlertDialog.Footer>
    </AlertDialog.Content>
  </AlertDialog.Root>
{/if}
