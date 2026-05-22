<script>
  // @ts-nocheck
  let { nodes, activeFile, projectPath, onFileClick, onToggle, onDiscard, onGitignore } = $props();

  import { ChevronRight, ChevronDown, Folder, FolderOpen } from '@lucide/svelte';
  import * as ContextMenu from '$lib/components/ui/context-menu/index.js';
  import { gitDiscardFile, gitAddToGitignore, openFileDefault } from '$lib/commands/git.js';
  import { revealItemInDir } from '@tauri-apps/plugin-opener';
  import { workspace } from '$lib/stores/workspace.svelte.js';

  function openInFileViewer(relPath) {
    const name = relPath.split('/').pop() ?? relPath;
    workspace.openTab({
      id: `file-edit::${relPath}`,
      type: 'file-edit',
      title: name,
      data: { projectPath, relPath, language: null },
    });
  }

  // ── Virtual list ──────────────────────────────────────────────────────────────
  const ROW_H  = 28;   // must match row CSS (h-7)
  const OVER   = 10;   // rows to render outside the viewport on each side

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

  // ── Tree state ────────────────────────────────────────────────────────────────
  let collapsed = $state(new Set());

  function toggleDir(path) {
    const next = new Set(collapsed);
    next.has(path) ? next.delete(path) : next.add(path);
    collapsed = next;
  }

  // Flatten visible tree nodes into a single array, respecting collapsed dirs
  function flatten(nodes, depth, out = []) {
    for (const node of nodes) {
      out.push({ node, depth });
      if (node.type === 'dir' && !collapsed.has(node.path))
        flatten(node.children, depth + 1, out);
    }
    return out;
  }

  let flat         = $derived(flatten(nodes, 0));
  let totalH       = $derived(flat.length * ROW_H);
  let startIdx     = $derived(Math.max(0, Math.floor(scrollTop / ROW_H) - OVER));
  let endIdx       = $derived(Math.min(flat.length, Math.ceil((scrollTop + vpH) / ROW_H) + OVER));
  let visible      = $derived(flat.slice(startIdx, endIdx));
  let padTop       = $derived(startIdx * ROW_H);

  // ── Colours ───────────────────────────────────────────────────────────────────
  const DOT_COLOR  = { added: 'bg-green-500', modified: 'bg-yellow-400', deleted: 'bg-red-500', renamed: 'bg-blue-400' };
  const NAME_COLOR = { added: 'text-green-600 dark:text-green-400', modified: '', deleted: 'text-red-500 line-through opacity-60', renamed: 'text-blue-500' };

  function statusKind(file)    { return file.indexStatus?.type ?? file.worktreeStatus?.type ?? 'modified'; }
  function isStaged(file)      { return file.indexStatus != null; }
  function dotColor(file)      { return file.conflicted ? 'bg-red-500' : (DOT_COLOR[statusKind(file)] ?? 'bg-muted-foreground/40'); }
  function guideX(i)           { return 16 + i * 14; }
  function getExt(name)        { const d = name.lastIndexOf('.'); return d > 0 ? name.slice(d + 1) : ''; }
  function getDirPart(rel)     { const s = rel.lastIndexOf('/'); return s > 0 ? rel.slice(0, s) : ''; }
  function absPath(rel)        {
    const sep = projectPath.includes('\\') ? '\\' : '/';
    return projectPath.replace(/[/\\]$/, '') + sep + rel.replace(/\//g, sep);
  }

  // ── Context menu actions ──────────────────────────────────────────────────────
  async function handleDiscard(file) {
    try { await gitDiscardFile(projectPath, file.path); onDiscard?.(file); }
    catch (e) { console.error('discard', e); }
  }

  async function handleIgnore(pattern) {
    try { await gitAddToGitignore(projectPath, pattern); onGitignore?.(); }
    catch (e) { console.error('gitignore', e); }
  }

  async function copyToClipboard(text) { try { await navigator.clipboard.writeText(text); } catch {} }

  async function showInExplorer(rel) {
    try { await revealItemInDir(absPath(rel)); } catch (e) { console.error(e); }
  }

  async function openWithDefault(rel) {
    try { await openFileDefault(absPath(rel)); } catch (e) { console.error(e); }
  }
</script>

<!-- ── Row snippets ──────────────────────────────────────────────────────────── -->

{#snippet fileRow(node, depth)}
  {@const file = node.file}
  {@const kind = statusKind(file)}
  {@const staged = isStaged(file)}
  {@const isActive = activeFile === file.path}
  {@const ext = getExt(node.name)}
  {@const dir = getDirPart(file.path)}
  {@const canDiscard = !(file.worktreeStatus?.type === 'added' && !file.indexStatus)}

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

        <button
          type="button"
          class="flex-1 text-left text-[13px] truncate min-w-0 transition-colors
            {file.conflicted ? 'text-red-500 dark:text-red-400' : isActive ? 'text-foreground' : 'text-muted-foreground group-hover:text-foreground'}
            {file.conflicted ? '' : NAME_COLOR[kind] ?? ''}"
          onclick={() => onFileClick(file)}
          ondblclick={(e) => { e.stopPropagation(); openInFileViewer(file.path); }}
        >
          {node.name}{#if file.conflicted}<span class="ml-1 text-[10px] font-bold opacity-70">!!</span>{/if}
        </button>

        <span class="shrink-0 w-2 h-2 rounded-full {dotColor(file)}" title={file.conflicted ? 'merge conflict' : kind}></span>

        <button
          type="button"
          aria-label="{staged ? 'Unstage' : 'Stage'} {file.path}"
          onclick={(e) => { e.stopPropagation(); onToggle(file, !staged); }}
          class="shrink-0 w-3.5 h-3.5 ml-1 rounded-[3px] border flex items-center justify-center transition-all
            {staged ? 'bg-primary border-primary text-primary-foreground' : 'border-muted-foreground/30 hover:border-primary/70 group-hover:border-muted-foreground/60'}"
        >
          {#if staged}
            <svg viewBox="0 0 10 10" class="w-2.5 h-2.5" fill="none" stroke="currentColor" stroke-width="2.2">
              <polyline points="1.5,5 4,8 8.5,2" />
            </svg>
          {/if}
        </button>
      </div>
    </ContextMenu.Trigger>

    <ContextMenu.Content class="w-56">
      {#if canDiscard}
        <ContextMenu.Item class="text-destructive focus:text-destructive focus:bg-destructive/10" onclick={() => handleDiscard(file)}>
          Discard changes
        </ContextMenu.Item>
        <ContextMenu.Separator />
      {/if}
      <ContextMenu.Item onclick={() => handleIgnore(file.path)}>Ignore file</ContextMenu.Item>
      {#if dir}<ContextMenu.Item onclick={() => handleIgnore(dir + '/')}>Ignore folder</ContextMenu.Item>{/if}
      {#if ext}<ContextMenu.Item onclick={() => handleIgnore('*.' + ext)}>Ignore all .{ext} files</ContextMenu.Item>{/if}
      <ContextMenu.Separator />
      <ContextMenu.Item onclick={() => copyToClipboard(absPath(file.path))}>Copy file path</ContextMenu.Item>
      <ContextMenu.Item onclick={() => copyToClipboard(file.path)}>Copy relative path</ContextMenu.Item>
      <ContextMenu.Separator />
      <ContextMenu.Item onclick={() => showInExplorer(file.path)}>Show in Explorer</ContextMenu.Item>
      <ContextMenu.Item onclick={() => openWithDefault(file.path)}>Open with default program</ContextMenu.Item>
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

    <ContextMenu.Content class="w-56">
      <ContextMenu.Item onclick={() => handleIgnore(node.path + '/')}>Ignore folder</ContextMenu.Item>
      <ContextMenu.Separator />
      <ContextMenu.Item onclick={() => copyToClipboard(absPath(node.path))}>Copy folder path</ContextMenu.Item>
      <ContextMenu.Item onclick={() => copyToClipboard(node.path)}>Copy relative path</ContextMenu.Item>
      <ContextMenu.Separator />
      <ContextMenu.Item onclick={() => showInExplorer(node.path)}>Show in Explorer</ContextMenu.Item>
    </ContextMenu.Content>
  </ContextMenu.Root>
{/snippet}

<!-- ── Virtual scroll container ─────────────────────────────────────────────── -->
<div
  bind:this={scrollEl}
  class="w-full h-full overflow-y-auto overflow-x-hidden"
  onscroll={(e) => { scrollTop = e.currentTarget.scrollTop; }}
>
  <!-- Full-height sentinel so the scrollbar represents the real list size -->
  <div style="height: {totalH}px; position: relative;">
    <!-- Only the visible slice, offset to its correct position -->
    <div style="position: absolute; top: {padTop}px; left: 0; right: 0;">
      {#each visible as { node, depth } (`${node.path}::${depth}`)}
        {#if node.type === 'dir'}
          {@render dirRow(node, depth)}
        {:else}
          {@render fileRow(node, depth)}
        {/if}
      {/each}
    </div>
  </div>
</div>
