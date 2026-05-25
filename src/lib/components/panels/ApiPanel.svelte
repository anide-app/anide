<script>
  // @ts-nocheck
  import { onMount } from 'svelte';
  import { workspace } from '$lib/stores/workspace.svelte.js';
  import {
    getRequestTree, initRequestsDir, createRequest, createCollection,
    deleteRequest, duplicateRequest, createEmptyRequest,
    renameRequest, renameCollection,
  } from '$lib/commands/api.js';
  import {
    Globe, FolderOpen, FolderPlus, FilePlus, Search, ChevronRight, ChevronDown,
    Trash2, Copy, MoreHorizontal, Loader2, Pencil,
  } from '@lucide/svelte';
  import { toast } from 'svelte-sonner';
  import { Input } from '$lib/components/ui/input/index.js';

  let tree = $state([]);
  let loading = $state(true);
  let error = $state('');
  let search = $state('');
  let expanded = $state(new Set());

  // Track which node has the context menu open
  let menuNode = $state(null);
  let menuPos = $state({ x: 0, y: 0 });

  // New-item inline form
  let newItem = $state(null); // { parentPath: string|null, type: 'request'|'collection' }
  let newName = $state('');
  let newNameEl = $state(null);

  // Inline rename
  let renameNode = $state(null);
  let renameName = $state('');
  let renameEl = $state(null);

  $effect(() => {
    if (newNameEl) setTimeout(() => newNameEl?.focus(), 0);
  });

  $effect(() => {
    if (renameEl) setTimeout(() => { renameEl?.focus(); renameEl?.select(); }, 0);
  });

  const folderPath = $derived(workspace.folderPath);

  async function load() {
    if (!folderPath) return;
    loading = true; error = '';
    try {
      await initRequestsDir(folderPath);
      tree = await getRequestTree(folderPath);
    } catch (e) {
      error = e?.message ?? String(e);
    } finally {
      loading = false;
    }
  }

  onMount(load);

  $effect(() => {
    workspace.worktreeChangeTick;
    if (folderPath) void load();
  });

  // ── Search filter ──────────────────────────────────────────────────────────
  function flatFiles(nodes) {
    const out = [];
    for (const n of nodes) {
      if (n.type === 'file') out.push(n);
      else out.push(...flatFiles(n.children ?? []));
    }
    return out;
  }

  let filtered = $derived(
    search.trim()
      ? flatFiles(tree).filter(f =>
          f.name.toLowerCase().includes(search.toLowerCase()) ||
          f.method.toLowerCase().includes(search.toLowerCase())
        )
      : null
  );

  // ── Open tab ───────────────────────────────────────────────────────────────
  function openRequest(node) {
    workspace.openTab({
      id: `api-request:${node.path}`,
      type: 'api-request',
      title: node.name,
      data: { relPath: node.path, folderPath },
    });
  }

  // ── Toggle folder ──────────────────────────────────────────────────────────
  function toggleFolder(path) {
    const next = new Set(expanded);
    if (next.has(path)) next.delete(path); else next.add(path);
    expanded = next;
  }

  // ── Context menu ───────────────────────────────────────────────────────────
  function openMenu(e, node) {
    e.preventDefault();
    e.stopPropagation();
    menuNode = node;
    menuPos = { x: e.clientX, y: e.clientY };
  }

  function closeMenu() { menuNode = null; }

  async function menuDelete() {
    const node = menuNode; closeMenu();
    if (!node) return;
    try {
      if (node.type === 'file') await deleteRequest(folderPath, node.path);
      await load();
    } catch (e) { toast.error(e?.message ?? 'Delete failed'); }
  }

  async function menuDuplicate() {
    const node = menuNode; closeMenu();
    if (!node || node.type !== 'file') return;
    try {
      await duplicateRequest(folderPath, node.path);
      await load();
    } catch (e) { toast.error(e?.message ?? 'Duplicate failed'); }
  }

  // ── New item ───────────────────────────────────────────────────────────────
  function startNew(type, parentPath = null) {
    closeMenu();
    newItem = { type, parentPath };
    newName = '';
  }

  async function commitNew() {
    if (!newName.trim() || !newItem) { newItem = null; return; }
    const name = newName.trim();
    try {
      if (newItem.type === 'collection') {
        const p = newItem.parentPath ? `${newItem.parentPath}/${name}` : name;
        await createCollection(folderPath, p);
      } else {
        const dir = newItem.parentPath ? `${newItem.parentPath}/` : '';
        const relPath = `${dir}${name}.md`;
        await createRequest(folderPath, relPath, { ...createEmptyRequest('GET'), body: `# ${name}\n\ndescription - \n` });
        workspace.openTab({
          id: `api-request:${relPath}`,
          type: 'api-request',
          title: name,
          data: { relPath, folderPath },
        });
      }
      await load();
    } catch (e) {
      toast.error(e?.message ?? 'Failed to create');
    }
    newItem = null;
  }

  // ── Rename ─────────────────────────────────────────────────────────────────
  function startRename(node) {
    closeMenu();
    renameNode = node;
    renameName = node.name;
  }

  async function commitRename() {
    if (!renameNode || !renameName.trim()) { renameNode = null; return; }
    const newName = renameName.trim();
    const node = renameNode;
    renameNode = null;
    if (newName === node.name) return;
    try {
      if (node.type === 'folder') {
        await renameCollection(folderPath, node.path, newName);
      } else {
        const newRelPath = await renameRequest(folderPath, node.path, newName);
        workspace.updateApiRequestTab(node.path, newRelPath, newName);
      }
      await load();
    } catch (e) {
      toast.error(e?.message ?? 'Rename failed');
    }
  }

  // ── Method badge colour ────────────────────────────────────────────────────
  const METHOD_COLOR = {
    GET: 'text-green-500', POST: 'text-blue-400', PUT: 'text-yellow-500',
    PATCH: 'text-orange-400', DELETE: 'text-red-500', HEAD: 'text-purple-400',
    OPTIONS: 'text-gray-400',
  };
  function methodColor(m) { return METHOD_COLOR[m?.toUpperCase()] ?? 'text-muted-foreground'; }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="h-full flex flex-col overflow-hidden" onclick={closeMenu}>

  <!-- Toolbar -->
  <div class="flex items-center gap-0.5 px-2 py-1.5 border-b shrink-0">
    <button
      type="button"
      title="New Request"
      onclick={() => startNew('request')}
      class="p-1 rounded hover:bg-muted transition-colors text-muted-foreground hover:text-foreground"
    ><FilePlus size={14} /></button>
    <button
      type="button"
      title="New Collection"
      onclick={() => startNew('collection')}
      class="p-1 rounded hover:bg-muted transition-colors text-muted-foreground hover:text-foreground"
    ><FolderPlus size={14} /></button>
    <div class="flex-1"></div>
    <button
      type="button"
      title="Refresh"
      onclick={load}
      class="p-1 rounded hover:bg-muted transition-colors text-muted-foreground hover:text-foreground"
    ><Globe size={14} /></button>
  </div>

  <!-- Search -->
  <div class="px-2 py-1.5 border-b shrink-0">
    <div class="relative flex items-center">
      <Search size={11} class="text-muted-foreground absolute left-2.5 pointer-events-none" />
      <Input
        type="text"
        placeholder="Search requests…"
        bind:value={search}
        class="pl-7 h-7 text-xs"
      />
    </div>
  </div>

  <!-- Tree / list -->
  <div class="flex-1 overflow-y-auto py-1">
    {#if loading}
      <div class="flex items-center justify-center py-6 gap-2 text-muted-foreground">
        <Loader2 size={14} class="animate-spin" /><span class="text-xs">Loading…</span>
      </div>
    {:else if error}
      <p class="text-xs text-destructive px-3 py-2">{error}</p>
    {:else if filtered !== null}
      <!-- Search results: flat list -->
      {#if filtered.length === 0}
        <p class="text-xs text-muted-foreground px-3 py-4 text-center">No results</p>
      {:else}
        {#each filtered as node (node.path)}
          <div
            role="none"
            onclick={() => openRequest(node)}
            class="flex items-center gap-2 px-3 py-1.5 text-xs hover:bg-muted/60 transition-colors cursor-pointer truncate"
          >
            <span class="font-mono text-[10px] shrink-0 w-12 text-right {methodColor(node.method)}">{node.method}</span>
            <span class="truncate">{node.name}</span>
          </div>
        {/each}
      {/if}
    {:else if tree.length === 0}
      <div class="flex flex-col items-center justify-center py-8 gap-2 text-muted-foreground px-4">
        <Globe size={24} class="opacity-20" />
        <p class="text-xs text-center opacity-60">No requests yet.<br />Click + to create one.</p>
      </div>
    {:else}
      <!-- Full tree -->
      {@render treeNodes(tree, 0)}
    {/if}

    <!-- Inline new-item form at root level -->
    {#if newItem && newItem.parentPath === null}
      {@render newItemInput(null)}
    {/if}
  </div>
</div>

{#snippet treeNodes(nodes, depth)}
  {#each nodes as node (node.path)}
    {#if node.type === 'folder'}
      <!-- Folder row -->
      <div
        role="none"
        class="group flex items-center gap-1 text-xs cursor-pointer select-none hover:bg-muted/40 transition-colors"
        style:padding-left="{8 + depth * 12}px"
        onclick={() => { if (renameNode?.path !== node.path) toggleFolder(node.path); }}
        oncontextmenu={(e) => openMenu(e, node)}
      >
        <span class="shrink-0 text-muted-foreground">
          {#if expanded.has(node.path)}
            <ChevronDown size={11} />
          {:else}
            <ChevronRight size={11} />
          {/if}
        </span>
        <FolderOpen size={12} class="shrink-0 text-muted-foreground" />
        {#if renameNode?.path === node.path}
          <input
            bind:this={renameEl}
            type="text"
            bind:value={renameName}
            onclick={(e) => e.stopPropagation()}
            onkeydown={(e) => { if (e.key === 'Enter') commitRename(); else if (e.key === 'Escape') renameNode = null; }}
            onblur={commitRename}
            class="flex-1 py-1 text-xs bg-muted/60 border border-primary rounded px-1.5 outline-none"
          />
        {:else}
          <span class="py-1.5 truncate flex-1" ondblclick={(e) => { e.stopPropagation(); startRename(node); }}>{node.name}</span>
        {/if}
        <button
          type="button"
          onclick={(e) => { e.stopPropagation(); openMenu(e, node); }}
          class="opacity-0 group-hover:opacity-100 p-1 mr-1 rounded hover:bg-muted transition-all"
        ><MoreHorizontal size={11} /></button>
      </div>

      {#if expanded.has(node.path)}
        {@render treeNodes(node.children ?? [], depth + 1)}
        {#if newItem && newItem.parentPath === node.path}
          {@render newItemInput(node.path, depth + 1)}
        {/if}
      {/if}

    {:else}
      <!-- Request row -->
      <div
        role="none"
        class="group flex items-center gap-2 text-xs cursor-pointer select-none hover:bg-muted/60 transition-colors"
        style:padding-left="{8 + depth * 12}px"
        style:padding-right="4px"
        onclick={() => { if (renameNode?.path !== node.path) openRequest(node); }}
        oncontextmenu={(e) => openMenu(e, node)}
      >
        <span class="font-mono text-[10px] shrink-0 w-12 text-right {methodColor(node.method)}">{node.method}</span>
        {#if renameNode?.path === node.path}
          <input
            bind:this={renameEl}
            type="text"
            bind:value={renameName}
            onclick={(e) => e.stopPropagation()}
            onkeydown={(e) => { if (e.key === 'Enter') commitRename(); else if (e.key === 'Escape') renameNode = null; }}
            onblur={commitRename}
            class="flex-1 py-0.5 text-xs bg-muted/60 border border-primary rounded px-1.5 outline-none"
          />
        {:else}
          <span class="py-1.5 truncate flex-1 text-xs" ondblclick={(e) => { e.stopPropagation(); startRename(node); }}>{node.name}</span>
        {/if}
        <button
          type="button"
          onclick={(e) => { e.stopPropagation(); openMenu(e, node); }}
          class="opacity-0 group-hover:opacity-100 p-1 mr-0.5 rounded hover:bg-muted transition-all shrink-0"
        ><MoreHorizontal size={11} /></button>
      </div>
    {/if}
  {/each}
{/snippet}

{#snippet newItemInput(parentPath, depth = 0)}
  <div class="flex items-center gap-1 px-2 py-1" style:padding-left="{8 + (depth ?? 0) * 12}px">
    {#if newItem?.type === 'collection'}
      <FolderOpen size={12} class="shrink-0 text-muted-foreground" />
    {:else}
      <Globe size={12} class="shrink-0 text-muted-foreground" />
    {/if}
    <input
      bind:this={newNameEl}
      type="text"
      placeholder={newItem?.type === 'collection' ? 'collection-name' : 'request-name'}
      bind:value={newName}
      onkeydown={(e) => { if (e.key === 'Enter') commitNew(); else if (e.key === 'Escape') { newItem = null; } }}
      onblur={commitNew}
      class="flex-1 text-xs bg-muted/60 border border-border rounded px-1.5 py-0.5 outline-none focus:border-primary"
    />
  </div>
{/snippet}

<!-- Context menu -->
{#if menuNode}
  <div
    class="fixed z-50 min-w-36 bg-popover border border-border rounded-md shadow-lg py-1 text-xs"
    style:left="{menuPos.x}px"
    style:top="{menuPos.y}px"
  >
    {#if menuNode.type === 'folder'}
      <button type="button" onclick={() => startNew('request', menuNode.path)}
        class="w-full flex items-center gap-2 px-3 py-1.5 hover:bg-muted transition-colors text-left">
        <FilePlus size={12} />New Request
      </button>
      <button type="button" onclick={() => startNew('collection', menuNode.path)}
        class="w-full flex items-center gap-2 px-3 py-1.5 hover:bg-muted transition-colors text-left">
        <FolderPlus size={12} />New Collection
      </button>
      <div class="border-t border-border my-1"></div>
      <button type="button" onclick={() => startRename(menuNode)}
        class="w-full flex items-center gap-2 px-3 py-1.5 hover:bg-muted transition-colors text-left">
        <Pencil size={12} />Rename
      </button>
    {:else}
      <button type="button" onclick={() => { openRequest(menuNode); closeMenu(); }}
        class="w-full flex items-center gap-2 px-3 py-1.5 hover:bg-muted transition-colors text-left">
        <Globe size={12} />Open
      </button>
      <button type="button" onclick={menuDuplicate}
        class="w-full flex items-center gap-2 px-3 py-1.5 hover:bg-muted transition-colors text-left">
        <Copy size={12} />Duplicate
      </button>
      <button type="button" onclick={() => startRename(menuNode)}
        class="w-full flex items-center gap-2 px-3 py-1.5 hover:bg-muted transition-colors text-left">
        <Pencil size={12} />Rename
      </button>
      <div class="border-t border-border my-1"></div>
      <button type="button" onclick={menuDelete}
        class="w-full flex items-center gap-2 px-3 py-1.5 hover:bg-muted transition-colors text-left text-destructive">
        <Trash2 size={12} />Delete
      </button>
    {/if}
  </div>
{/if}
