<script>
  // @ts-nocheck
  let { data, children } = $props();

  import { onMount } from 'svelte';
  import { workspace } from '$lib/stores/workspace.svelte.js';
  import { useSearchParams, createSearchParamsSchema } from 'runed/kit';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { getVersion } from '@tauri-apps/api/app';
  import { platform } from '@tauri-apps/plugin-os';
  import { listen } from '@tauri-apps/api/event';
  import { watchProject, unwatchProject } from '$lib/commands/watcher.js';

  import {
    Minus, Square, X,
    Globe, Database, KeySquare, Box, GitBranch, Container, Braces,
    FileText, FileKey, PanelLeftClose, BookOpen, FolderOpen, Gamepad2,
    Terminal,
  } from '@lucide/svelte';

  import Logo from '$lib/components/logo.svelte';
  import Themetoggle from '$lib/components/themetoggle.svelte';
  import * as Tooltip from '$lib/components/ui/tooltip/index.js';

  import { open as openDialog } from '@tauri-apps/plugin-dialog';
  import { WebviewWindow } from '@tauri-apps/api/webviewWindow';
  import { toast } from 'svelte-sonner';

  import EnvPanel from '$lib/components/panels/EnvPanel.svelte';
  import ApiPanel from '$lib/components/panels/ApiPanel.svelte';
  import GitPanel from '$lib/components/panels/GitPanel.svelte';
  import DockerPanel from '$lib/components/panels/DockerPanel.svelte';
  import DocsPanel from '$lib/components/panels/DocsPanel.svelte';
  import TerminalPanel from '$lib/components/panels/TerminalPanel.svelte';
  import StubPanel from '$lib/components/panels/StubPanel.svelte';
  import ReadmeTab from '$lib/components/workspace/ReadmeTab.svelte';
  import EnvTab from '$lib/components/workspace/EnvTab.svelte';
  import DiffTab from '$lib/components/workspace/DiffTab.svelte';
  import ImageDiffTab from '$lib/components/workspace/ImageDiffTab.svelte';
  import CommitTab from '$lib/components/workspace/CommitTab.svelte';
  import DockerLogsTab from '$lib/components/workspace/DockerLogsTab.svelte';
  import TerminalTab from '$lib/components/workspace/TerminalTab.svelte';
  import FileTab from '$lib/components/workspace/FileTab.svelte';
  import DocTab from '$lib/components/workspace/DocTab.svelte';
  import FilesPanel from '$lib/components/panels/FilesPanel.svelte';
  import TicTacToe from '$lib/components/TicTacToe.svelte';

  let gameOpen  = $state(false);
  let gameWidth = $state(320);

  function startGameResize(e) {
    const startX = e.clientX;
    const startW = gameWidth;
    const onMove = (mv) => {
      gameWidth = Math.max(240, Math.min(600, startW - (mv.clientX - startX)));
    };
    const onUp = () => {
      window.removeEventListener('mousemove', onMove);
      window.removeEventListener('mouseup', onUp);
    };
    window.addEventListener('mousemove', onMove);
    window.addEventListener('mouseup', onUp);
  }

  let isWindows = $state(false);
  let appWindow;
  let appVersion = $state('');

  // ── Sidebar resize ─────────────────────────────────────────────────────────
  let sidebarWidth = $state(280);

  function startSidebarResize(e) {
    const startX = e.clientX;
    const startW = sidebarWidth;
    const onMove = (e) => {
      sidebarWidth = Math.max(160, Math.min(600, startW + e.clientX - startX));
    };
    const onUp = () => {
      window.removeEventListener('mousemove', onMove);
      window.removeEventListener('mouseup', onUp);
    };
    window.addEventListener('mousemove', onMove);
    window.addEventListener('mouseup', onUp);
  }

  const schema = createSearchParamsSchema({ path: { type: 'string', default: '' } });
  const params = useSearchParams(schema, { pushHistory: false });
  let folderPath = $derived(params.path);
  let folderName = $derived(folderPath.split(/[\\/]/).filter(Boolean).pop() ?? 'Project');

  // Keep workspace in sync with URL path
  $effect(() => { workspace.folderPath = folderPath; });

  // File system watcher lifecycle
  $effect(() => {
    const path = folderPath;
    if (!path) return;

    let stopped = false;
    let cleanupListen;

    void watchProject(path).catch(console.error);

    void listen('fs:changed', (event) => {
      if (stopped) return;
      const { modified, created, deleted } = event.payload;
      const allPaths = [...(modified ?? []), ...(created ?? []), ...(deleted ?? [])];

      let gitBumped = false;
      let worktreeBumped = false;
      let fileTickCount = 0;
      for (const rel of allPaths) {
        if (
          rel === '.git/HEAD' ||
          rel === '.git/index' ||
          rel === '.git/COMMIT_EDITMSG' ||
          rel === '.git/packed-refs' ||
          rel.startsWith('.git/refs/')
        ) {
          if (!gitBumped) { workspace.bumpGit(); gitBumped = true; }
        } else if (!rel.startsWith('.git/')) {
          // Cap per-file ticks so mass changes (thousands of files) don't
          // flood Svelte's reactivity. DiffTab / ReadmeTab still refresh
          // via worktreeChangeTick for the common single-file-save case.
          if (fileTickCount < 20) { workspace.bumpFileTick(rel); fileTickCount++; }
          if (!worktreeBumped) { workspace.bumpWorktree(); worktreeBumped = true; }
        }
      }
    }).then(unlisten => {
      if (stopped) { unlisten(); return; }
      cleanupListen = unlisten;
    });

    return () => {
      stopped = true;
      cleanupListen?.();
      void unwatchProject().catch(console.error);
    };
  });

  async function openFolder() {
    try {
      const selected = await openDialog({ directory: true, multiple: false, title: 'Choose a project folder' });
      if (!selected) return;
      const label = `folder-${Date.now()}`;
      const encoded = encodeURIComponent(selected);
      const win = new WebviewWindow(label, {
        url: `app?path=${encoded}`,
        title: selected.split(/[\\/]/).filter(Boolean).pop() ?? 'Project',
        width: 1200, height: 800, transparent: true, decorations: false, resizable: true, focus: true,
      });
      win.once('tauri://error', () => toast.error('Failed to open folder'));
    } catch {
      toast.error('Failed to open folder');
    }
  }

  onMount(async () => {
    appWindow = getCurrentWindow();
    isWindows = (await platform()) === 'windows';
    appVersion = await getVersion();
  });

  const toolItems = [
    { id: 'docs',     icon: BookOpen,  label: 'Docs' },
    { id: 'terminal', icon: Terminal,  label: 'Terminal' },
    { id: 'api',      icon: Globe,     label: 'API' },
    { id: 'db',     icon: Database,   label: 'Database' },
    { id: 'kv',     icon: Braces,     label: 'Cache' },
    { id: 's3',     icon: Box,        label: 'Storage' },
    { id: 'git',    icon: GitBranch,  label: 'Git' },
    { id: 'docker', icon: Container,  label: 'Docker' },
    { id: 'env',    icon: KeySquare,  label: 'Env' },
  ];

  const panelLabels = {
    docs: 'Docs', terminal: 'Terminal', api: 'API Requests', db: 'Databases', kv: 'Cache',
    s3: 'Storage', git: 'Git', docker: 'Docker', env: 'Env Files', files: 'Explorer',
  };

  import { GitBranch as GitBranchIcon, GitCommit, FileCode } from '@lucide/svelte';
  const tabTypeIcons = {
    readme: FileText,
    doc: BookOpen,
    'env-file': FileKey,
    'git-diff': GitBranchIcon,
    'git-commit': GitCommit,
    'docker-logs': Terminal,
    'terminal': Terminal,
    'file-edit': FileCode,
  };

  let activeTab = $derived(workspace.tabs.find(t => t.id === workspace.activeTabId) ?? null);

  function handleTabMouseDown(e, tabId) {
    // Middle click closes the tab
    if (e.button === 1) { e.preventDefault(); workspace.closeTab(tabId); }
  }
</script>

<div class="h-screen w-full flex flex-col text-foreground bg-background overflow-hidden">
  <!-- ── HEADER ── -->
  <header
    data-tauri-drag-region
    class="grid grid-cols-3 w-full items-center border-b shrink-0"
  >
    <div></div>
    <div data-tauri-drag-region class="flex items-center px-4 py-2.5 justify-center gap-2">
      <span class="font-semibold text-sm tracking-tight">{folderName}</span>
      {#if workspace.gitInfo}
        <span class="text-muted-foreground text-xs">·</span>
        <span class="flex items-center gap-1 text-xs text-muted-foreground font-mono">
          <GitBranch size={11} />
          {workspace.gitInfo.branch}
        </span>
      {/if}
    </div>
    <div data-tauri-drag-region class="flex items-center justify-end">
      <!-- Game button — before window controls on Windows/Linux; only button on Mac -->
      <button
        type="button"
        aria-label="Toggle game"
        title="Tic Tac Toe"
        onclick={() => (gameOpen = !gameOpen)}
        class="w-11 h-9 flex items-center justify-center transition-colors
          {gameOpen ? 'text-primary' : 'text-muted-foreground hover:text-foreground'}"
      ><Gamepad2 size={15} /></button>
      {#if isWindows}
        <button
          type="button"
          aria-label="Minimize"
          class="w-11 h-9 flex items-center justify-center hover:bg-muted transition-colors"
          onclick={() => appWindow.minimize()}
        ><Minus size={14} /></button>
        <button
          type="button"
          aria-label="Maximize"
          class="w-11 h-9 flex items-center justify-center hover:bg-muted transition-colors"
          onclick={() => appWindow.toggleMaximize()}
        ><Square size={14} /></button>
        <button
          type="button"
          aria-label="Close"
          class="w-11 h-9 flex items-center justify-center hover:bg-red-500 hover:text-white transition-colors"
          onclick={() => appWindow.close()}
        ><X size={14} /></button>
      {/if}
    </div>
  </header>

  <!-- ── BODY ── -->
  <div class="flex flex-1 overflow-hidden">

    <!-- Activity bar -->
    <aside class="w-12 shrink-0 border-r flex flex-col items-center bg-background/50">
      <Tooltip.Provider>

        <!-- Logo / home -->
        <Tooltip.Root>
          <Tooltip.Trigger>
            {#snippet child({ props })}
              <button
                {...props}
                type="button"
                onclick={() => { workspace.activeTabId = null; workspace.activeTool = null; workspace.sidebarOpen = false; }}
                class="w-full flex justify-center py-3 mb-1 transition-colors text-muted-foreground hover:text-foreground/80"
              >
                <div class="size-9 flex items-center justify-center">
                  <Logo class="size-9" />
                </div>
              </button>
            {/snippet}
          </Tooltip.Trigger>
          <Tooltip.Content side="right">Home</Tooltip.Content>
        </Tooltip.Root>

        <!-- Tool icons -->
        <nav class="flex flex-col items-center gap-0.5 flex-1 w-full">
          {#each toolItems as tool, i (tool.id)}
            <Tooltip.Root>
              <Tooltip.Trigger>
                {#snippet child({ props })}
                  <button
                    {...props}
                    type="button"
                    onclick={() => workspace.setActiveTool(tool.id)}
                    class="w-full h-10 flex items-center justify-center relative transition-colors duration-150
                      {workspace.activeTool === tool.id && workspace.sidebarOpen
                        ? 'text-foreground'
                        : 'text-muted-foreground hover:text-foreground/80'}"
                  >
                    {#if workspace.activeTool === tool.id && workspace.sidebarOpen}
                      <span class="absolute left-0 top-1.5 bottom-1.5 w-0.75 bg-primary rounded-r"></span>
                    {/if}
                    <span class="icon-pop flex items-center justify-center" style="animation-delay: {i * 42}ms">
                      <tool.icon
                        size={20}
                        strokeWidth={workspace.activeTool === tool.id && workspace.sidebarOpen ? 2.2 : 1.5}
                      />
                    </span>
                  </button>
                {/snippet}
              </Tooltip.Trigger>
              <Tooltip.Content side="right">{tool.label}</Tooltip.Content>
            </Tooltip.Root>
          {/each}
        </nav>

        <!-- File explorer toggle -->
        <Tooltip.Root>
          <Tooltip.Trigger>
            {#snippet child({ props })}
              <button
                {...props}
                type="button"
                onclick={() => workspace.setActiveTool('files')}
                class="w-full h-10 flex items-center justify-center relative transition-colors duration-150
                  {workspace.activeTool === 'files' && workspace.sidebarOpen
                    ? 'text-foreground'
                    : 'text-muted-foreground hover:text-foreground/80'}"
              >
                {#if workspace.activeTool === 'files' && workspace.sidebarOpen}
                  <span class="absolute left-0 top-1.5 bottom-1.5 w-0.75 bg-primary rounded-r"></span>
                {/if}
                <FolderOpen
                  size={20}
                  strokeWidth={workspace.activeTool === 'files' && workspace.sidebarOpen ? 2.2 : 1.5}
                />
              </button>
            {/snippet}
          </Tooltip.Trigger>
          <Tooltip.Content side="right">Explorer</Tooltip.Content>
        </Tooltip.Root>

        <!-- Theme toggle -->
        <div class="pb-3 pt-1">
          <Themetoggle ghost={true} />
        </div>
      </Tooltip.Provider>
    </aside>

    <!-- Content: sidebar + tab area in stable flex row.
         The tab area is ALWAYS in the DOM so mounted components (FileEditor etc.)
         survive sidebar open/close without losing state. -->
    <div class="flex flex-1 overflow-hidden min-w-0">

      <!-- Secondary sidebar: enters/leaves DOM freely — no important state here -->
      {#if workspace.sidebarOpen && workspace.activeTool}
        <div
          class="shrink-0 flex flex-col border-r overflow-hidden"
          style:width="{sidebarWidth}px"
          style:min-width="160px"
        >
          <div class="flex items-center justify-between px-3 py-2 border-b shrink-0">
            <span class="text-[11px] font-semibold uppercase tracking-wider text-muted-foreground">
              {panelLabels[workspace.activeTool] ?? workspace.activeTool}
            </span>
            <button
              type="button"
              title="Close panel"
              onclick={() => (workspace.sidebarOpen = false)}
              class="text-muted-foreground hover:text-foreground transition-colors"
            >
              <PanelLeftClose size={14} />
            </button>
          </div>
          <div class="flex-1 overflow-hidden">
            {#if workspace.activeTool === 'docs'}
              <DocsPanel />
            {:else if workspace.activeTool === 'terminal'}
              <TerminalPanel />
            {:else if workspace.activeTool === 'env'}
              <EnvPanel />
            {:else if workspace.activeTool === 'api'}
              <ApiPanel />
            {:else if workspace.activeTool === 'git'}
              <GitPanel />
            {:else if workspace.activeTool === 'files'}
              <FilesPanel />
            {:else if workspace.activeTool !== 'docker'}
              <StubPanel tool={workspace.activeTool} />
            {/if}
            <div class="h-full {workspace.activeTool === 'docker' ? '' : 'hidden'}">
              <DockerPanel />
            </div>
          </div>
        </div>

        <!-- Drag-to-resize handle -->
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div
          class="w-1 shrink-0 bg-border/40 hover:bg-primary/60 cursor-col-resize transition-colors"
          onmousedown={startSidebarResize}
        ></div>
      {/if}

      <!-- Tab area: NEVER re-mounts, always at this DOM position -->
      <div class="flex-1 min-w-0 overflow-hidden flex flex-col">
        {@render tabArea()}
      </div>

      <!-- Resizable game panel -->
      {#if gameOpen}
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div
          class="w-1 shrink-0 bg-border/40 hover:bg-primary/60 cursor-col-resize transition-colors"
          onmousedown={startGameResize}
        ></div>
        <div
          class="shrink-0 border-l overflow-auto flex items-center justify-center p-4"
          style:width="{gameWidth}px"
        >
          <TicTacToe />
        </div>
      {/if}

    </div>

  </div>
</div>

<!-- Tab area snippet (reused with/without sidebar) -->
{#snippet tabArea()}
  <div class="h-full flex flex-col overflow-hidden">

    <!-- Tab bar -->
    {#if workspace.tabs.length > 0}
      <div class="flex items-stretch border-b bg-muted/30 shrink-0 overflow-x-auto scrollbar-none">
        {#each workspace.tabs as tab (tab.id)}
          {@const isActive = workspace.activeTabId === tab.id}
          {@const isDirty = workspace.dirtyTabIds.has(tab.id)}
          {@const TabIcon = tabTypeIcons[tab.type] ?? FileText}
          <!-- Wrapper holds active styling; both buttons are siblings (no nested interactives) -->
          <div
            class="group flex items-stretch shrink-0 border-r transition-colors
              {isActive ? 'bg-background -mb-px border-b-2 border-b-primary hover:bg-muted/30' : 'hover:bg-muted/60'}"
            role="none"
            onmousedown={(e) => handleTabMouseDown(e, tab.id)}
          >
            <button
              type="button"
              onclick={() => (workspace.activeTabId = tab.id)}
              class="flex items-center gap-1.5 pl-3 pr-1 py-2 text-xs transition-colors
                {isActive
                  ? 'text-foreground'
                  : 'text-muted-foreground group-hover:text-foreground'}"
            >
              <TabIcon size={12} strokeWidth={isActive ? 2.2 : 1.5} />
              <span class="max-w-35 truncate">{tab.title}</span>
              {#if isDirty}
                <span class="w-1.5 h-1.5 rounded-full bg-primary shrink-0"></span>
              {/if}
            </button>
            <button
              type="button"
              aria-label="Close tab"
              onclick={(e) => { e.stopPropagation(); workspace.closeTab(tab.id); }}
              onkeydown={(e) => e.key === 'Enter' && workspace.closeTab(tab.id)}
              class="flex items-center justify-center pr-2 pl-0.5 rounded opacity-0 group-hover:opacity-100 transition-opacity
                {isActive ? 'text-foreground' : 'text-muted-foreground'}"
            >
              <X size={11} />
            </button>
          </div>
        {/each}
      </div>
    {/if}

    <!-- Tab content — all tabs stay mounted, only active one is visible -->
    <div class="flex-1 overflow-hidden relative">
      {#each workspace.tabs as tab (tab.id)}
        <div class="absolute inset-0 overflow-hidden {workspace.activeTabId === tab.id ? 'block' : 'hidden'}">
          {#if tab.type === 'doc'}
            <DocTab data={tab.data} tabId={tab.id} />
          {:else if tab.type === 'readme'}
            <ReadmeTab data={tab.data} tabId={tab.id} />
          {:else if tab.type === 'env-file'}
            <EnvTab data={tab.data} tabId={tab.id} />
          {:else if tab.type === 'git-diff'}
            {#if tab.data.fileKind === 'image'}
              <ImageDiffTab data={tab.data} />
            {:else}
              <DiffTab data={tab.data} />
            {/if}
          {:else if tab.type === 'git-commit'}
            <CommitTab data={tab.data} />
          {:else if tab.type === 'docker-logs'}
            <DockerLogsTab data={tab.data} tabId={tab.id} />
          {:else if tab.type === 'terminal'}
            <TerminalTab data={tab.data} tabId={tab.id} />
          {:else if tab.type === 'file-edit'}
            <FileTab data={tab.data} tabId={tab.id} />
          {/if}
        </div>
      {/each}

      {#if !workspace.activeTabId}
        <div class="h-full flex flex-col items-center justify-center gap-4 text-muted-foreground">
          <Logo class="size-48" />
          <div class="flex flex-col items-center gap-2 text-center my-4">
            <p class="text-xs text-muted-foreground mb-2">welcome to</p>
            <p class="text-5xl font-bold font-serif lowercase tracking-tight text-foreground">anide.app</p>
            {#if appVersion}
              <p class="text-xs text-muted-foreground mt-1">v{appVersion}</p>
            {/if}
          </div>
          <button
            type="button"
            onclick={openFolder}
            class="flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium rounded-md border hover:bg-muted transition-colors"
          >
            <FolderOpen size={13} />Open New Folder
          </button>
        </div>
      {/if}
    </div>

  </div>
{/snippet}

<!-- Hidden SvelteKit slot (keeps routing happy) -->
<div class="hidden">{@render children()}</div>

<style>
  .icon-pop {
    animation: iconPop 320ms cubic-bezier(0.25, 0.46, 0.45, 0.94) both;
  }
  @keyframes iconPop {
    0%   { opacity: 0; transform: scale(0.65); }
    100% { opacity: 1; transform: scale(1); }
  }
  .scrollbar-none {
    scrollbar-width: none;
  }
  .scrollbar-none::-webkit-scrollbar {
    display: none;
  }
</style>
