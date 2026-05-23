<script>
  // @ts-nocheck
  import { onMount, onDestroy } from 'svelte';
  import { workspace } from '$lib/stores/workspace.svelte.js';
  import { readProjectFile, writeProjectFile } from '$lib/commands/files.js';
  import { Loader2, Save, RotateCcw, Workflow } from '@lucide/svelte';
  import { toast } from 'svelte-sonner';
  import { Button } from '$lib/components/ui/button/index.js';

  let { data, tabId } = $props();
  // data = { relPath, folderPath }

  let container = $state(null);
  let mountRoot = null;
  let excalidrawAPI = null;

  let loading = $state(true);
  let loadError = $state('');
  let saving = $state(false);
  let isDirty = $derived(workspace.dirtyTabIds.has(tabId));

  // Skip the first onChange fires that happen during initial render
  let changeGuard = 0;

  function isDarkMode() {
    return document.documentElement.classList.contains('dark');
  }

  async function parseFile() {
    const raw = await readProjectFile(data.folderPath, data.relPath);
    if (!raw?.trim()) return { type: 'excalidraw', version: 2, elements: [], appState: {}, files: {} };
    return JSON.parse(raw);
  }

  async function save() {
    if (!excalidrawAPI || saving) return;
    saving = true;
    try {
      const elements = excalidrawAPI.getSceneElements();
      const appState = excalidrawAPI.getAppState();
      const files = excalidrawAPI.getFiles();
      const payload = {
        type: 'excalidraw',
        version: 2,
        source: 'anide.app',
        elements: elements.filter(el => !el.isDeleted),
        appState: {
          gridSize: appState.gridSize,
          gridStep: appState.gridStep,
          gridModeEnabled: appState.gridModeEnabled,
          viewBackgroundColor: appState.viewBackgroundColor,
        },
        files,
      };
      await writeProjectFile(data.folderPath, data.relPath, JSON.stringify(payload, null, 2));
      workspace.setTabDirty(tabId, false);
      toast.success('Saved');
    } catch (e) {
      toast.error(e?.message ?? 'Save failed');
    } finally {
      saving = false;
    }
  }

  async function discard() {
    let fileData;
    try { fileData = await parseFile(); } catch (err) { toast.error(`Failed to discard: ${err?.message ?? String(err)}`); return; }
    changeGuard = 2;
    excalidrawAPI?.updateScene({
      elements: fileData.elements ?? [],
      appState: fileData.appState ?? {},
    });
    workspace.setTabDirty(tabId, false);
  }

  async function mount() {
    if (!container) return;

    const [{ default: React }, { createRoot }, { Excalidraw }] = await Promise.all([
      import('react'),
      import('react-dom/client'),
      import('@excalidraw/excalidraw'),
    ]);

    let fileData;
    try {
      fileData = await parseFile();
    } catch (err) {
      loadError = err?.message ?? String(err);
      loading = false;
      return;
    }
    changeGuard = 2; // skip the initial onChange burst

    if (mountRoot) mountRoot.unmount();

    mountRoot = createRoot(container);
    mountRoot.render(
      React.createElement(Excalidraw, {
        initialData: {
          elements: fileData.elements ?? [],
          appState: {
            ...(fileData.appState ?? {}),
            theme: isDarkMode() ? 'dark' : 'light',
          },
          files: fileData.files ?? {},
          scrollToContent: true,
        },
        theme: isDarkMode() ? 'dark' : 'light',
        excalidrawAPI: (api) => { excalidrawAPI = api; },
        onChange: () => {
          if (changeGuard > 0) { changeGuard--; return; }
          workspace.setTabDirty(tabId, true);
        },
        UIOptions: {
          canvasActions: {
            toggleTheme: false,
            saveAsImage: true,
            saveToActiveFile: false,
            loadScene: false,
            export: { saveFileToDisk: true },
          },
        },
      })
    );
    loading = false;
  }

  onMount(() => {
    void mount();

    // Ctrl/Cmd+S to save
    function onKeyDown(e) {
      if ((e.ctrlKey || e.metaKey) && e.key === 's' && !e.shiftKey) {
        e.preventDefault();
        void save();
      }
    }
    window.addEventListener('keydown', onKeyDown, true);

    // Sync theme changes
    const observer = new MutationObserver(() => {
      excalidrawAPI?.updateScene({
        appState: { theme: isDarkMode() ? 'dark' : 'light' },
      });
    });
    observer.observe(document.documentElement, { attributes: true, attributeFilter: ['class'] });

    return () => {
      window.removeEventListener('keydown', onKeyDown, true);
      observer.disconnect();
    };
  });

  onDestroy(() => {
    mountRoot?.unmount();
    mountRoot = null;
  });
</script>

<div class="h-full flex flex-col overflow-hidden">
  <!-- Toolbar -->
  <div class="flex items-center justify-between px-3 py-1.5 border-b shrink-0 bg-muted/30">
    <div class="flex items-center gap-1.5">
      <Workflow size={13} class="text-muted-foreground" />
      <span class="text-xs text-muted-foreground font-mono truncate max-w-64">{data.relPath}</span>
      {#if isDirty}
        <span class="w-1.5 h-1.5 rounded-full bg-primary shrink-0" title="Unsaved changes"></span>
      {/if}
    </div>
    <div class="flex items-center gap-1">
      {#if isDirty}
        <Button variant="ghost" size="sm" class="h-6 px-2 text-xs gap-1" onclick={discard} disabled={saving}>
          <RotateCcw size={11} />Discard
        </Button>
      {/if}
      <Button
        size="sm"
        class="h-6 px-2 text-xs gap-1"
        onclick={save}
        disabled={saving || !isDirty}
      >
        {#if saving}
          <Loader2 size={11} class="animate-spin" />
        {:else}
          <Save size={11} />
        {/if}
        Save
      </Button>
    </div>
  </div>

  <!-- Canvas -->
  <div class="flex-1 relative overflow-hidden">
    {#if loading}
      <div class="absolute inset-0 flex items-center justify-center text-muted-foreground gap-2">
        <Loader2 size={16} class="animate-spin" />
        <span class="text-sm">Loading…</span>
      </div>
    {/if}
    {#if loadError}
      <p class="p-4 text-sm text-destructive">{loadError}</p>
    {/if}
    <div bind:this={container} class="w-full h-full"></div>
  </div>
</div>
