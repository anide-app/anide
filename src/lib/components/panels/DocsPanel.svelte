<script>
  // @ts-nocheck
  import { workspace } from '$lib/stores/workspace.svelte.js';
  import { listDocFiles } from '$lib/commands/files.js';
  import { gitStatus } from '$lib/commands/git.js';
  import { Loader2, BookOpen } from '@lucide/svelte';
  import FileTree from './FileTree.svelte';

  let projectPath = $derived(workspace.folderPath);
  let files = $state([]);
  let loading = $state(true);
  let error = $state('');
  let gitStatusMap = $state(new Map());
  let loadedPath = $state('');

  function buildTree(entries) {
    const root = [];
    const dirMap = new Map();

    function ensureParents(parts) {
      let arr = root;
      for (let i = 0; i < parts.length - 1; i++) {
        const key = parts.slice(0, i + 1).join('/');
        if (!dirMap.has(key)) {
          const node = { type: 'dir', name: parts[i], path: key, children: [] };
          arr.push(node);
          dirMap.set(key, node);
        }
        arr = dirMap.get(key).children;
      }
      return arr;
    }

    for (const entry of entries) {
      const parts = entry.path.split('/');
      const name = parts[parts.length - 1];
      const arr = ensureParents(parts);
      if (entry.is_dir) {
        if (!dirMap.has(entry.path)) {
          const node = { type: 'dir', name, path: entry.path, children: [] };
          arr.push(node);
          dirMap.set(entry.path, node);
        }
      } else {
        arr.push({ type: 'file', name, path: entry.path });
      }
    }
    return root;
  }

  async function load() {
    if (!projectPath) return;
    if (loadedPath !== projectPath) loading = true;
    error = '';
    try {
      const [docFiles, statusResult] = await Promise.all([
        listDocFiles(projectPath),
        gitStatus(projectPath).catch(() => ({ files: [], total: 0 })),
      ]);
      files = docFiles;
      gitStatusMap = new Map(
        statusResult.files.map(f => [f.path, f.indexStatus?.type ?? f.worktreeStatus?.type])
      );
      loadedPath = projectPath;
    } catch (e) {
      error = e?.message ?? String(e);
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    workspace.worktreeChangeTick;
    if (projectPath) void load();
  });

  let tree = $derived(buildTree(files));

  function openFile(relPath) {
    const fileName = relPath.split('/').pop();
    const isFlow = relPath.endsWith('.excalidraw');
    workspace.openTab({
      id: `${isFlow ? 'excalidraw' : 'doc'}::${relPath}`,
      type: isFlow ? 'excalidraw' : 'doc',
      title: fileName,
      data: { relPath, folderPath: projectPath },
    });
  }
</script>

<div class="h-full flex flex-col overflow-hidden select-none">
  {#if loading}
    <div class="flex items-center justify-center py-10 gap-2 text-muted-foreground">
      <Loader2 size={14} class="animate-spin" />
      <span class="text-sm">Scanning…</span>
    </div>
  {:else if error}
    <p class="text-sm text-destructive px-3 py-4">{error}</p>
  {:else}
    <div class="flex-1 overflow-hidden relative">
      {#if files.length === 0}
        <div class="absolute inset-0 flex flex-col items-center justify-center gap-2 text-muted-foreground pointer-events-none">
          <BookOpen size={28} class="opacity-30" />
          <p class="text-sm">No markdown files found</p>
          <p class="text-xs opacity-60">Right-click to create a new doc</p>
        </div>
      {/if}
      <FileTree
        mode="docs"
        nodes={tree}
        {gitStatusMap}
        activeFile={
          workspace.activeTabId?.startsWith('doc::') ? workspace.activeTabId.slice(5) :
          workspace.activeTabId?.startsWith('excalidraw::') ? workspace.activeTabId.slice(12) : null
        }
        {projectPath}
        onFileClick={(node) => openFile(node.path)}
        onRefresh={load}
      />
    </div>
  {/if}
</div>
