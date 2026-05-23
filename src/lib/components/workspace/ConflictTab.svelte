<script>
  // @ts-nocheck
  let { data, tabId } = $props();

  import { readProjectFile, writeProjectFile } from '$lib/commands/files.js';
  import { gitStageFile } from '$lib/commands/git.js';
  import { workspace } from '$lib/stores/workspace.svelte.js';
  import { GitMerge, Loader2, Check, AlertTriangle } from '@lucide/svelte';
  import { toast } from 'svelte-sonner';

  let projectPath = $derived(data.projectPath);
  let relPath     = $derived(data.relPath);

  let loading = $state(true);
  let saving  = $state(false);
  let error   = $state('');
  let sections = $state([]);

  // Parse raw file content into alternating context / conflict sections.
  function parseConflicts(content) {
    const lines = content.split('\n');
    const result = [];
    let ctx = [];
    let conflictNum = 0;
    let i = 0;

    while (i < lines.length) {
      if (lines[i].startsWith('<<<<<<<')) {
        if (ctx.length) { result.push({ type: 'context', text: ctx.join('\n') }); ctx = []; }
        const oursLabel = lines[i].slice(8).trim();
        const oursLines = [];
        i++;
        while (i < lines.length && !lines[i].startsWith('=======')) { oursLines.push(lines[i]); i++; }
        i++; // skip =======
        const theirsLines = [];
        while (i < lines.length && !lines[i].startsWith('>>>>>>>')) { theirsLines.push(lines[i]); i++; }
        const theirsLabel = i < lines.length ? lines[i].slice(8).trim() : '';
        if (i < lines.length) i++;
        result.push({
          type: 'conflict',
          num: ++conflictNum,
          oursLabel,
          theirsLabel,
          ours: oursLines.join('\n'),
          theirs: theirsLines.join('\n'),
          choice: null,  // null | 'ours' | 'theirs' | 'both'
        });
      } else {
        ctx.push(lines[i]);
        i++;
      }
    }
    if (ctx.length) result.push({ type: 'context', text: ctx.join('\n') });
    return result;
  }

  async function load() {
    loading = true; error = '';
    try {
      const content = await readProjectFile(projectPath, relPath);
      sections = parseConflicts(content);
    } catch (e) {
      error = e?.message ?? String(e);
    } finally {
      loading = false;
    }
  }

  $effect(() => { if (projectPath && relPath) void load(); });

  let conflicts     = $derived(sections.filter(s => s.type === 'conflict'));
  let resolvedCount = $derived(conflicts.filter(s => s.choice !== null).length);
  let allResolved   = $derived(conflicts.length > 0 && resolvedCount === conflicts.length);

  function reconstruct() {
    return sections.map(s => {
      if (s.type === 'context') return s.text;
      if (s.choice === 'ours')   return s.ours;
      if (s.choice === 'theirs') return s.theirs;
      if (s.choice === 'both')   return [s.ours, s.theirs].filter(v => v !== '').join('\n');
      return `<<<<<<< ${s.oursLabel}\n${s.ours}\n=======\n${s.theirs}\n>>>>>>> ${s.theirsLabel}`;
    }).join('\n');
  }

  async function saveAndStage() {
    if (!allResolved || saving) return;
    saving = true;
    try {
      await writeProjectFile(projectPath, relPath, reconstruct());
      await gitStageFile(projectPath, relPath);
      workspace.closeTab(tabId);
      toast.success(`${relPath.split('/').pop()} resolved & staged`);
    } catch (e) {
      toast.error(e?.message ?? String(e));
    } finally {
      saving = false;
    }
  }
</script>

<div class="h-full flex flex-col overflow-hidden">

  <!-- Header -->
  <div class="flex items-center gap-2 px-4 border-b shrink-0 h-10">
    <GitMerge size={14} class="text-destructive shrink-0" />
    <span class="text-sm font-medium truncate flex-1 min-w-0">{relPath.split('/').pop()}</span>

    {#if !loading && conflicts.length > 0}
      <span class="text-xs text-muted-foreground shrink-0">
        {resolvedCount} / {conflicts.length} resolved
      </span>
    {/if}

    <button
      type="button"
      onclick={saveAndStage}
      disabled={!allResolved || saving}
      class="flex items-center gap-1.5 px-2.5 py-1 rounded text-xs font-medium shrink-0
             bg-primary text-primary-foreground hover:bg-primary/90 transition-colors
             disabled:opacity-40 disabled:cursor-not-allowed"
    >
      {#if saving}
        <Loader2 size={12} class="animate-spin" />Saving…
      {:else}
        <Check size={12} />Save & Stage
      {/if}
    </button>
  </div>

  <!-- Body -->
  {#if loading}
    <div class="flex-1 flex items-center justify-center gap-2 text-muted-foreground">
      <Loader2 size={14} class="animate-spin" />
      <span class="text-sm">Loading…</span>
    </div>
  {:else if error}
    <div class="flex-1 flex items-center justify-center gap-2 text-destructive px-8">
      <AlertTriangle size={14} class="shrink-0" />
      <span class="text-sm break-all">{error}</span>
    </div>
  {:else}
    <div class="flex-1 overflow-y-auto">
      {#each sections as section, i}

        {#if section.type === 'context'}
          {#if section.text.trim()}
            <pre class="px-5 py-1.5 text-[12px] font-mono leading-relaxed text-muted-foreground/60
                        whitespace-pre-wrap break-all select-text">{section.text}</pre>
          {/if}

        {:else}
          <!-- Conflict block -->
          <div class="border-y border-destructive/25 my-1
                      {section.choice ? 'bg-muted/20' : 'bg-destructive/5'}">

            <!-- Conflict header -->
            <div class="flex items-center gap-2 px-4 py-1.5 border-b border-destructive/20
                        {section.choice ? 'bg-muted/30' : 'bg-destructive/10'}">
              {#if section.choice}
                <Check size={11} class="text-green-600 dark:text-green-400 shrink-0" />
              {:else}
                <AlertTriangle size={11} class="text-destructive shrink-0" />
              {/if}
              <span class="text-[11px] font-medium {section.choice ? 'text-muted-foreground' : 'text-destructive'}">
                Conflict {section.num} of {conflicts.length}
              </span>
              {#if section.choice}
                <span class="text-[10px] text-muted-foreground ml-1">
                  — {section.choice === 'ours' ? 'accepted ours' : section.choice === 'theirs' ? 'accepted theirs' : 'kept both'}
                </span>
              {/if}
            </div>

            <!-- Two-column diff -->
            <div class="grid grid-cols-2 divide-x divide-border min-h-0">

              <!-- Ours -->
              <div class="min-w-0 transition-opacity {section.choice === 'theirs' ? 'opacity-30' : ''}
                          {section.choice === 'ours' || section.choice === 'both' ? 'bg-green-500/8' : ''}">
                <div class="px-3 py-1 text-[10px] font-sans font-medium text-green-700 dark:text-green-400
                            border-b border-border/40 bg-green-500/5 flex items-center gap-1.5">
                  <span class="opacity-60">HEAD</span>
                  <span class="opacity-40">·</span>
                  <span class="truncate">{section.oursLabel}</span>
                </div>
                <pre class="px-3 py-2.5 text-[12px] font-mono leading-relaxed
                            text-green-800 dark:text-green-300 whitespace-pre-wrap break-all select-text
                            {!section.ours ? 'text-muted-foreground/40 italic' : ''}">{section.ours || '(empty)'}</pre>
              </div>

              <!-- Theirs -->
              <div class="min-w-0 transition-opacity {section.choice === 'ours' ? 'opacity-30' : ''}
                          {section.choice === 'theirs' || section.choice === 'both' ? 'bg-blue-500/8' : ''}">
                <div class="px-3 py-1 text-[10px] font-sans font-medium text-blue-700 dark:text-blue-400
                            border-b border-border/40 bg-blue-500/5 flex items-center gap-1.5">
                  <span class="opacity-60">Incoming</span>
                  <span class="opacity-40">·</span>
                  <span class="truncate">{section.theirsLabel}</span>
                </div>
                <pre class="px-3 py-2.5 text-[12px] font-mono leading-relaxed
                            text-blue-800 dark:text-blue-300 whitespace-pre-wrap break-all select-text
                            {!section.theirs ? 'text-muted-foreground/40 italic' : ''}">{section.theirs || '(empty)'}</pre>
              </div>
            </div>

            <!-- Action bar -->
            <div class="flex items-center gap-2 px-4 py-2 border-t border-border/30 bg-muted/10">
              <button
                type="button"
                onclick={() => { sections[i].choice = sections[i].choice === 'ours' ? null : 'ours'; }}
                class="px-2.5 py-1 rounded text-[11px] font-sans font-medium transition-colors
                  {section.choice === 'ours'
                    ? 'bg-green-600 text-white'
                    : 'bg-muted hover:bg-green-500/15 text-muted-foreground hover:text-green-700 dark:hover:text-green-300'}"
              >Accept Ours</button>

              <button
                type="button"
                onclick={() => { sections[i].choice = sections[i].choice === 'theirs' ? null : 'theirs'; }}
                class="px-2.5 py-1 rounded text-[11px] font-sans font-medium transition-colors
                  {section.choice === 'theirs'
                    ? 'bg-blue-600 text-white'
                    : 'bg-muted hover:bg-blue-500/15 text-muted-foreground hover:text-blue-700 dark:hover:text-blue-300'}"
              >Accept Theirs</button>

              <button
                type="button"
                onclick={() => { sections[i].choice = sections[i].choice === 'both' ? null : 'both'; }}
                class="px-2.5 py-1 rounded text-[11px] font-sans font-medium transition-colors
                  {section.choice === 'both'
                    ? 'bg-primary text-primary-foreground'
                    : 'bg-muted hover:bg-muted/80 text-muted-foreground hover:text-foreground'}"
              >Keep Both</button>
            </div>
          </div>
        {/if}

      {/each}
    </div>
  {/if}

</div>
