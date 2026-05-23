<script>
  // @ts-nocheck
  import { invoke } from '@tauri-apps/api/core';
  import { workspace } from '$lib/stores/workspace.svelte.js';
  import {
    gitBranches, gitCheckoutBranch, gitCreateBranch, gitFetch, gitRemoteStatus,
    gitPush, gitPull, gitPublishBranch, gitDeleteBranch, gitStashPop,
  } from '$lib/commands/git.js';
  import { ArrowDown as ArrowDownIcon } from '@lucide/svelte';
  import { Button } from '$lib/components/ui/button/index.js';
  import { Input } from '$lib/components/ui/input/index.js';
  import { ScrollArea } from '$lib/components/ui/scroll-area/index.js';
  import * as Popover from '$lib/components/ui/popover/index.js';
  import * as Dialog from '$lib/components/ui/dialog/index.js';
  import GitChanges from './GitChanges.svelte';
  import GitHistory from './GitHistory.svelte';
  import {
    GitBranch, RefreshCw, ArrowUp, ArrowDown, Plus, Check, Loader2, ChevronDown, Trash2,
  } from '@lucide/svelte';

  let activeTab = $state('changes');
  let projectPath = $derived(workspace.folderPath);

  // Incremented after any state-changing operation to trigger child reloads
  let refreshTick = $state(0);
  function bumpRefresh() { refreshTick++; }

  // Branch state
  let branchList = $state({ current: '', isDetached: false, branches: [] });
  let remoteStatus = $state({ ahead: 0, behind: 0, remoteName: null, remoteBranch: null });
  let branchOpen = $state(false);
  let branchSearch = $state('');
  let fetching = $state(false);
  let fetchError = $state('');
  let pulling = $state(false);
  let pullError = $state('');
  let pushing = $state(false);
  let pushError = $state('');
  let publishing = $state(false);
  let publishError = $state('');
  let pushBehindOpen = $state(false); // warn dialog: need to pull first

  // Stash-and-pull dialog (shown when pull fails due to local changes)
  let pullStashOpen = $state(false);
  let pullStashBusy = $state(false);
  let pullStashError = $state('');

  // Stash dialog (branch switching)
  let stashOpen = $state(false);
  let stashTargetBranch = $state('');
  let stashError = $state('');
  let stashBusy = $state(false);

  // Delete branch
  let deletingBranch = $state('');  // branch name being confirmed
  let deleteError = $state('');
  let deleteBusy = $state(false);

  // New branch dialog
  let newBranchOpen = $state(false);
  let newBranchName = $state('');
  let newBranchBase = $state('');
  let fromOpen = $state(false);
  let fromSearch = $state('');
  let creatingBranch = $state(false);
  let createBranchError = $state('');

  $effect(() => {
    workspace.gitRefreshTick; // reload branches/remote on external git changes (watcher)
    if (projectPath) {
      void loadBranches();
      void loadRemoteStatus();
    }
  });

  async function loadBranches() {
    try { branchList = await gitBranches(projectPath); }
    catch (e) { console.error('branches', e); }
  }

  async function loadRemoteStatus() {
    try { remoteStatus = await gitRemoteStatus(projectPath); }
    catch (e) { /* remote may not be configured */ }
  }

  async function handleFetch() {
    fetching = true; fetchError = '';
    try { await gitFetch(projectPath); await loadRemoteStatus(); }
    catch (e) { fetchError = e?.message ?? String(e); }
    finally { fetching = false; }
  }

  async function handlePull() {
    pulling = true; pullError = '';
    try {
      await gitPull(projectPath);
      await loadRemoteStatus();
      await loadBranches();
      bumpRefresh();
    } catch (e) {
      const msg = e?.message ?? String(e);
      if (msg.includes('local changes') || msg.includes('overwritten by merge') || msg.includes('Please commit') || msg.includes('Please move or remove')) {
        pullStashOpen = true; pullStashError = '';
      } else {
        pullError = msg;
      }
    }
    finally { pulling = false; }
  }

  async function handleStashAndPull() {
    pullStashBusy = true; pullStashError = '';
    try {
      await invoke('git_stash', { projectPath });
      try {
        await gitPull(projectPath);
      } catch (pullErr) {
        // Pull failed after stash — try to restore stashed changes so user loses nothing
        try {
          await gitStashPop(projectPath);
        } catch (stashErr) {
          throw new Error(`Pull failed: ${pullErr?.message ?? pullErr}\n\nAlso failed to restore stash: ${stashErr?.message ?? stashErr}`);
        }
        throw pullErr;
      }
      await gitStashPop(projectPath);
      pullStashOpen = false;
      await loadRemoteStatus();
      await loadBranches();
      bumpRefresh();
    } catch (e) {
      pullStashError = e?.message ?? String(e);
    } finally {
      pullStashBusy = false;
    }
  }

  async function handlePush() {
    // Block push when behind — pushing over divergent history risks overwriting remote work
    if (remoteStatus.behind > 0) { pushBehindOpen = true; return; }
    pushing = true; pushError = '';
    try { await gitPush(projectPath); await loadRemoteStatus(); }
    catch (e) { pushError = e?.message ?? String(e); }
    finally { pushing = false; }
  }

  async function handlePublish() {
    publishing = true; publishError = '';
    try {
      await gitPublishBranch(projectPath, branchList.current);
      await loadRemoteStatus();
      bumpRefresh();
    } catch (e) { publishError = e?.message ?? String(e); }
    finally { publishing = false; }
  }

  function onChildCommit() {
    bumpRefresh();
    void loadRemoteStatus();
  }

  async function handleCheckout(name) {
    branchOpen = false;
    try {
      await gitCheckoutBranch(projectPath, name);
      await loadBranches();
      await loadRemoteStatus();
    } catch (e) {
      const msg = e?.message ?? String(e);
      stashTargetBranch = name;
      stashError = msg.includes('local changes') || msg.includes('overwritten') || msg.includes('Please commit') ? '' : msg;
      stashOpen = true;
    }
  }

  async function handleStashAndSwitch() {
    stashBusy = true; stashError = '';
    try {
      await invoke('git_stash', { projectPath });
      await gitCheckoutBranch(projectPath, stashTargetBranch);
      stashOpen = false;
      await loadBranches(); await loadRemoteStatus();
    } catch (e) { stashError = e?.message ?? String(e); }
    finally { stashBusy = false; }
  }

  async function handleBringChanges() {
    stashBusy = true; stashError = '';
    try {
      await invoke('git_checkout_force', { projectPath, branch: stashTargetBranch });
      stashOpen = false;
      await loadBranches(); await loadRemoteStatus();
    } catch (e) { stashError = e?.message ?? String(e); }
    finally { stashBusy = false; }
  }

  async function handleDeleteBranch(name, force = false) {
    deleteBusy = true;
    deleteError = '';
    try {
      await gitDeleteBranch(projectPath, name, force);
      deletingBranch = '';
      await loadBranches();
    } catch (e) {
      const msg = e?.message ?? String(e);
      deleteError = (msg.includes('not fully merged') || msg.includes('not fully-merged'))
        ? 'unmerged'
        : msg;
    } finally {
      deleteBusy = false;
    }
  }

  function sanitizeBranchName(raw) {
    return raw
      .trim()
      .replace(/\s+/g, '-')               // spaces → hyphens
      .replace(/[~^:?*[\\]+/g, '')        // strip git-invalid chars
      .replace(/\.{2,}/g, '.')            // no consecutive dots
      .replace(/^[.\-]+|[.\-]+$/g, '')    // no leading/trailing dots or hyphens
      .replace(/-{2,}/g, '-')             // collapse consecutive hyphens
      .replace(/\/+/g, '/');              // collapse consecutive slashes
  }

  let sanitizedBranchName = $derived(sanitizeBranchName(newBranchName));
  let branchNameChanged = $derived(
    newBranchName.trim() !== '' && sanitizedBranchName !== newBranchName.trim()
  );

  async function handleCreateBranch() {
    if (!sanitizedBranchName) return;
    creatingBranch = true; createBranchError = '';
    try {
      await gitCreateBranch(projectPath, sanitizedBranchName, newBranchBase || undefined);
      await handleCheckout(sanitizedBranchName);
      newBranchOpen = false; newBranchName = ''; newBranchBase = '';
    } catch (e) { createBranchError = e?.message ?? String(e); }
    finally { creatingBranch = false; }
  }

  // Filter out HEAD pointers (origin/HEAD etc.) — they're refs, not real branches
  let allBranches = $derived(branchList.branches.filter(b => !b.name.endsWith('/HEAD')));

  let filteredBranches = $derived(
    allBranches.filter(b => !branchSearch || b.name.toLowerCase().includes(branchSearch.toLowerCase()))
  );

  // Branches available as "From" base when creating new branch
  let fromBranches = $derived(
    allBranches.filter(b => !fromSearch || b.name.toLowerCase().includes(fromSearch.toLowerCase()))
  );

  function openDiff(file, staged) {
    const tabId = `git-diff::${file.path}::${staged ? 'staged' : 'unstaged'}`;
    workspace.openTab({
      id: tabId,
      type: 'git-diff',
      title: file.path.split('/').pop(),
      data: { relPath: file.path, staged, folderPath: projectPath, fileKind: file.fileKind },
    });
  }

  function openCommit(commit) {
    workspace.openTab({
      id: `git-commit::${commit.hash}`,
      type: 'git-commit',
      title: commit.shortHash,
      data: { commit, folderPath: projectPath },
    });
  }
</script>

<div class="h-full flex flex-col overflow-hidden">

  <!-- Branch header -->
  <div class="px-2 py-2 border-b shrink-0 space-y-1.5">
    <!-- Branch selector -->
    <Popover.Root bind:open={branchOpen}>
      <Popover.Trigger>
        {#snippet child({ props })}
          <button
            {...props}
            type="button"
            class="w-full flex items-center gap-1.5 px-2 py-1.5 rounded-md border hover:bg-muted/50 transition-colors text-xs"
          >
            <GitBranch size={12} class="shrink-0 text-muted-foreground" />
            <span class="flex-1 text-left truncate font-medium">
              {branchList.current || '—'}
            </span>
            <ChevronDown size={11} class="shrink-0 text-muted-foreground" />
          </button>
        {/snippet}
      </Popover.Trigger>
      <Popover.Content class="w-64 p-0" align="start">
        <div class="p-2 border-b">
          <Input
            placeholder="Filter branches..."
            bind:value={branchSearch}
            class="h-7 text-xs"
            autofocus
          />
        </div>
        <ScrollArea class="max-h-56">
          <div class="p-1">
            {#each filteredBranches as b (b.name)}
              {@const isCurrentOrRemote = b.isRemote || b.name === branchList.current}
              {@const isDeleting = deletingBranch === b.name}
              <div class="group/branch flex items-center rounded text-xs hover:bg-muted transition-colors" role="none">
                <!-- Checkout button (whole row except delete zone) -->
                <button
                  type="button"
                  class="flex-1 flex items-center gap-2 px-2 py-1.5 min-w-0"
                  onclick={() => { deletingBranch = ''; deleteError = ''; handleCheckout(b.name); }}
                >
                  <GitBranch size={11} class="shrink-0 text-muted-foreground" />
                  <span class="flex-1 truncate {b.isRemote ? 'text-muted-foreground' : ''}">{b.name}</span>
                  {#if b.name === branchList.current}
                    <Check size={11} class="shrink-0 text-primary" />
                  {/if}
                </button>

                <!-- Delete zone (local non-current branches only) -->
                {#if !isCurrentOrRemote}
                  {#if isDeleting}
                    {#if deleteError === 'unmerged'}
                      <div class="flex items-center gap-1 pr-1.5 shrink-0">
                        <span class="text-[10px] text-muted-foreground">Unmerged.</span>
                        <button
                          type="button"
                          class="text-[10px] text-destructive font-medium hover:underline disabled:opacity-50"
                          onclick={() => handleDeleteBranch(b.name, true)}
                          disabled={deleteBusy}
                        >Force?</button>
                        <button
                          type="button"
                          class="text-[10px] text-muted-foreground hover:text-foreground"
                          onclick={() => { deletingBranch = ''; deleteError = ''; }}
                        >Cancel</button>
                      </div>
                    {:else if deleteError}
                      <div class="flex items-center gap-1 pr-1.5 shrink-0">
                        <span class="text-[10px] text-destructive truncate max-w-24" title={deleteError}>Error</span>
                        <button
                          type="button"
                          class="text-[10px] text-muted-foreground hover:text-foreground"
                          onclick={() => { deletingBranch = ''; deleteError = ''; }}
                        >✕</button>
                      </div>
                    {:else}
                      <div class="flex items-center gap-1 pr-1.5 shrink-0">
                        <button
                          type="button"
                          class="text-[10px] text-destructive font-medium hover:underline disabled:opacity-50"
                          onclick={() => handleDeleteBranch(b.name)}
                          disabled={deleteBusy}
                        >{deleteBusy ? '…' : 'Delete?'}</button>
                        <button
                          type="button"
                          class="text-[10px] text-muted-foreground hover:text-foreground"
                          onclick={() => { deletingBranch = ''; deleteError = ''; }}
                        >Cancel</button>
                      </div>
                    {/if}
                  {:else}
                    <button
                      type="button"
                      aria-label="Delete {b.name}"
                      class="opacity-0 group-hover/branch:opacity-100 p-1 mr-1 rounded text-muted-foreground hover:text-destructive transition-colors shrink-0"
                      onclick={(e) => { e.stopPropagation(); deletingBranch = b.name; deleteError = ''; }}
                    >
                      <Trash2 size={11} />
                    </button>
                  {/if}
                {/if}
              </div>
            {/each}
          </div>
        </ScrollArea>
        <div class="p-1 border-t">
          <button
            type="button"
            class="w-full flex items-center gap-2 px-2 py-1.5 rounded text-xs hover:bg-muted transition-colors text-muted-foreground hover:text-foreground"
            onclick={() => { branchOpen = false; newBranchOpen = true; newBranchBase = branchList.current; fromSearch = ''; }}
          >
            <Plus size={11} class="shrink-0" />New branch
          </button>
        </div>
      </Popover.Content>
    </Popover.Root>

    <!-- Remote status row -->
    <div class="flex items-center gap-1.5 min-w-0">
      {#if remoteStatus.remoteName}
        <!-- Has remote: show ahead/behind + pull + push + fetch -->
        <span class="text-[10px] text-muted-foreground opacity-60 shrink-0">{remoteStatus.remoteName}</span>
        <div class="flex items-center gap-1.5 text-[10px] text-muted-foreground">
          {#if remoteStatus.ahead > 0}
            <span class="flex items-center gap-0.5 text-blue-400"><ArrowUp size={10} />{remoteStatus.ahead}</span>
          {/if}
          {#if remoteStatus.behind > 0}
            <span class="flex items-center gap-0.5 text-amber-400"><ArrowDown size={10} />{remoteStatus.behind}</span>
          {/if}
        </div>
        <div class="flex-1"></div>
        {#if fetchError || pushError || pullError}
          <span
            class="text-[10px] text-destructive truncate max-w-32 cursor-help"
            title={fetchError || pushError || pullError}
          >{(fetchError || pushError || pullError).split('\n')[0].slice(0, 48) || 'failed'}</span>
        {/if}
        {#if remoteStatus.behind > 0}
          <button
            type="button"
            title="Pull {remoteStatus.behind} commit{remoteStatus.behind !== 1 ? 's' : ''} from {remoteStatus.remoteName}"
            onclick={handlePull}
            disabled={pulling}
            class="flex items-center gap-1 text-[10px] text-amber-400 hover:text-amber-300 transition-colors disabled:opacity-50"
          >
            {#if pulling}<Loader2 size={9} class="animate-spin" />{:else}<ArrowDown size={10} />{/if}
            {pulling ? 'Pulling...' : 'Pull'}
          </button>
        {/if}
        {#if remoteStatus.ahead > 0}
          <button
            type="button"
            title={remoteStatus.behind > 0 ? 'Pull first before pushing' : `Push ${remoteStatus.ahead} commit${remoteStatus.ahead !== 1 ? 's' : ''}`}
            onclick={handlePush}
            disabled={pushing || pulling}
            class="flex items-center gap-1 text-[10px] transition-colors disabled:opacity-50
              {remoteStatus.behind > 0
                ? 'text-muted-foreground/40 cursor-not-allowed'
                : 'text-muted-foreground hover:text-foreground'}"
          >
            {#if pushing}<Loader2 size={9} class="animate-spin" />{:else}<ArrowUp size={10} />{/if}
            Push
          </button>
        {/if}
        <button
          type="button"
          title="Fetch origin"
          onclick={handleFetch}
          disabled={fetching || pulling}
          class="flex items-center gap-1 text-[10px] text-muted-foreground hover:text-foreground transition-colors disabled:opacity-50"
        >
          <RefreshCw size={10} class={fetching ? 'animate-spin' : ''} />
          {fetching ? 'Fetching...' : 'Fetch'}
        </button>
      {:else}
        <!-- No remote: show publish button -->
        <div class="flex-1"></div>
        {#if publishError}
          <span class="text-[10px] text-destructive truncate max-w-28" title={publishError}>publish failed</span>
        {/if}
        <button
          type="button"
          onclick={handlePublish}
          disabled={publishing || !branchList.current}
          class="flex items-center gap-1 text-[10px] text-muted-foreground hover:text-foreground transition-colors disabled:opacity-50"
        >
          {#if publishing}<Loader2 size={9} class="animate-spin" />{:else}<ArrowUp size={10} />{/if}
          {publishing ? 'Publishing...' : 'Publish branch'}
        </button>
      {/if}
    </div>
  </div>

  <!-- Sub-tabs -->
  <div class="flex border-b shrink-0">
    {#each [['changes', 'Changes'], ['history', 'History']] as [id, label]}
      <button
        type="button"
        onclick={() => (activeTab = id)}
        class="flex-1 py-1.5 text-xs font-medium transition-colors
          {activeTab === id
            ? 'text-foreground border-b-2 border-primary -mb-px'
            : 'text-muted-foreground hover:text-foreground'}"
      >
        {label}
      </button>
    {/each}
  </div>

  <!-- Tab content -->
  <div class="flex-1 overflow-hidden">
    <div class="h-full {activeTab === 'changes' ? 'block' : 'hidden'}">
      <GitChanges
        {projectPath}
        currentBranch={branchList.current}
        onOpenDiff={openDiff}
        onCommit={onChildCommit}
        {refreshTick}
      />
    </div>
    <div class="h-full {activeTab === 'history' ? 'block' : 'hidden'}">
      <GitHistory {projectPath} onOpenCommit={openCommit} {refreshTick} />
    </div>
  </div>

</div>

<!-- New branch dialog -->
<Dialog.Root bind:open={newBranchOpen}>
  <Dialog.Content class="sm:max-w-sm">
    <Dialog.Header>
      <Dialog.Title>New Branch</Dialog.Title>
    </Dialog.Header>
    <div class="space-y-3 py-1">
      <div class="space-y-1.5">
        <label class="text-xs font-medium">Branch name</label>
        <Input
          placeholder="feature/my-branch"
          bind:value={newBranchName}
          class="text-sm"
          autofocus
          onkeydown={(e) => e.key === 'Enter' && handleCreateBranch()}
        />
        {#if branchNameChanged}
          <p class="text-[11px] text-muted-foreground flex items-center gap-1">
            Will be created as
            <span class="font-mono bg-muted px-1 py-0.5 rounded text-foreground">{sanitizedBranchName}</span>
          </p>
        {/if}
      </div>
      <div class="space-y-1.5">
        <label class="text-xs font-medium">From</label>
        <!-- Popover-based branch picker -->
        <Popover.Root bind:open={fromOpen}>
          <Popover.Trigger>
            {#snippet child({ props })}
              <button
                {...props}
                type="button"
                class="w-full flex items-center gap-2 px-3 h-9 rounded-md border bg-background text-sm hover:bg-muted/50 transition-colors"
              >
                <GitBranch size={13} class="shrink-0 text-muted-foreground" />
                <span class="flex-1 text-left truncate">{newBranchBase || 'Select base branch…'}</span>
                <ChevronDown size={13} class="shrink-0 text-muted-foreground" />
              </button>
            {/snippet}
          </Popover.Trigger>
          <Popover.Content class="w-72 p-0" align="start">
            <div class="p-2 border-b">
              <Input
                placeholder="Filter..."
                bind:value={fromSearch}
                class="h-7 text-xs"
                autofocus
              />
            </div>
            <ScrollArea class="max-h-48">
              <div class="p-1">
                {#each fromBranches.filter(b => !b.isRemote) as b (b.name)}
                  <button
                    type="button"
                    class="w-full flex items-center gap-2 px-2 py-1.5 rounded text-sm hover:bg-muted transition-colors"
                    onclick={() => { newBranchBase = b.name; fromOpen = false; }}
                  >
                    <GitBranch size={12} class="shrink-0 text-muted-foreground" />
                    <span class="flex-1 truncate">{b.name}</span>
                    {#if b.name === newBranchBase}<Check size={11} class="shrink-0 text-primary" />{/if}
                  </button>
                {/each}
                {#if fromBranches.some(b => b.isRemote)}
                  <div class="px-2 py-1 text-[10px] text-muted-foreground uppercase tracking-wide mt-1">Remote</div>
                  {#each fromBranches.filter(b => b.isRemote) as b (b.name)}
                    <button
                      type="button"
                      class="w-full flex items-center gap-2 px-2 py-1.5 rounded text-sm hover:bg-muted transition-colors"
                      onclick={() => { newBranchBase = b.name; fromOpen = false; }}
                    >
                      <GitBranch size={12} class="shrink-0 text-muted-foreground opacity-60" />
                      <span class="flex-1 truncate text-muted-foreground">{b.name}</span>
                      {#if b.name === newBranchBase}<Check size={11} class="shrink-0 text-primary" />{/if}
                    </button>
                  {/each}
                {/if}
              </div>
            </ScrollArea>
          </Popover.Content>
        </Popover.Root>
      </div>
      {#if createBranchError}
        <p class="text-xs text-destructive">{createBranchError}</p>
      {/if}
    </div>
    <Dialog.Footer class="gap-2">
      <Button variant="outline" onclick={() => { newBranchOpen = false; createBranchError = ''; }}>Cancel</Button>
      <Button onclick={handleCreateBranch} disabled={!sanitizedBranchName || creatingBranch}>
        {#if creatingBranch}<Loader2 size={13} class="mr-1.5 animate-spin" />{/if}
        Create branch
      </Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>

<!-- Stash-and-pull dialog -->
<Dialog.Root bind:open={pullStashOpen}>
  <Dialog.Content class="sm:max-w-sm">
    <Dialog.Header>
      <Dialog.Title>Uncommitted changes block pull</Dialog.Title>
      <Dialog.Description>
        Git can't pull because your local changes to one or more files would be overwritten.
        Stash your changes, pull, then restore them automatically.
      </Dialog.Description>
    </Dialog.Header>
    {#if pullStashError}
      <p class="text-xs text-destructive bg-destructive/10 border border-destructive/20 rounded px-3 py-2 whitespace-pre-wrap break-all">{pullStashError}</p>
    {/if}
    <Dialog.Footer class="flex-col gap-2 sm:flex-col">
      <Button class="w-full" onclick={handleStashAndPull} disabled={pullStashBusy}>
        {#if pullStashBusy}<Loader2 size={13} class="mr-1.5 animate-spin" />{/if}
        Stash, pull &amp; restore
      </Button>
      <Button variant="ghost" class="w-full" onclick={() => { pullStashOpen = false; pullStashError = ''; }} disabled={pullStashBusy}>
        Cancel
      </Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>

<!-- Pull-before-push warning dialog -->
<Dialog.Root bind:open={pushBehindOpen}>
  <Dialog.Content class="sm:max-w-sm">
    <Dialog.Header>
      <Dialog.Title>Pull before pushing</Dialog.Title>
      <Dialog.Description>
        Your branch is {remoteStatus.behind} commit{remoteStatus.behind !== 1 ? 's' : ''} behind
        <span class="font-mono font-medium text-foreground">{remoteStatus.remoteName}</span>.
        Push over divergent history can overwrite remote work. Pull first to merge or resolve conflicts, then push.
      </Dialog.Description>
    </Dialog.Header>
    <Dialog.Footer class="flex-col gap-2 sm:flex-col">
      <Button class="w-full" onclick={async () => { pushBehindOpen = false; await handlePull(); }}>
        {#if pulling}<Loader2 size={13} class="mr-1.5 animate-spin" />{/if}
        Pull now
      </Button>
      <Button variant="ghost" class="w-full" onclick={() => { pushBehindOpen = false; }}>Cancel</Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>

<!-- Stash / bring changes dialog -->
<Dialog.Root bind:open={stashOpen}>
  <Dialog.Content class="sm:max-w-sm">
    <Dialog.Header>
      <Dialog.Title>Uncommitted changes</Dialog.Title>
      <Dialog.Description>
        You have local changes. What would you like to do before switching to
        <span class="font-mono font-medium text-foreground">{stashTargetBranch}</span>?
      </Dialog.Description>
    </Dialog.Header>
    {#if stashError}
      <p class="text-xs text-destructive bg-destructive/10 border border-destructive/20 rounded px-3 py-2">{stashError}</p>
    {/if}
    <Dialog.Footer class="flex-col gap-2 sm:flex-col">
      <Button class="w-full" onclick={handleBringChanges} disabled={stashBusy}>
        {#if stashBusy}<Loader2 size={13} class="mr-1.5 animate-spin" />{/if}
        Bring my changes to {stashTargetBranch}
      </Button>
      <Button variant="outline" class="w-full" onclick={handleStashAndSwitch} disabled={stashBusy}>
        {#if stashBusy}<Loader2 size={13} class="mr-1.5 animate-spin" />{/if}
        Stash changes &amp; switch
      </Button>
      <Button variant="ghost" class="w-full" onclick={() => { stashOpen = false; stashError = ''; }} disabled={stashBusy}>
        Cancel
      </Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
