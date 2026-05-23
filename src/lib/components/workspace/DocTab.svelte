<script>
  // @ts-nocheck
  let { data, tabId } = $props();

  import { readProjectFile, writeProjectFile } from '$lib/commands/files.js';
  import { workspace } from '$lib/stores/workspace.svelte.js';
  import { untrack } from 'svelte';
  import MarkdownEditor from '$lib/components/MarkdownEditor.svelte';

  let editorRef = $state(null);

  $effect(() => {
    if (workspace.activeTabId === tabId) {
      requestAnimationFrame(() => editorRef?.focus());
    }
  });

  $effect(() => {
    function onWindowFocus() {
      if (workspace.activeTabId === tabId) requestAnimationFrame(() => editorRef?.focus());
    }
    window.addEventListener('focus', onWindowFocus);
    return () => window.removeEventListener('focus', onWindowFocus);
  });
  import FrontmatterEditor from '$lib/components/FrontmatterEditor.svelte';
  import { Loader2, XCircle, ChevronDown, ChevronRight, Tags } from '@lucide/svelte';
  // ChevronDown/Right used by fmButton snippet

  let projectPath = $derived(data.folderPath);
  let relPath     = $derived(data.relPath);

  let loading      = $state(true);
  let loadError    = $state('');
  let body         = $state('');
  let yamlText     = $state('');
  let originalYaml = $state('');
  let fmOpen       = $state(false);
  let bodyDirty    = $state(false);
  let fmDirty      = $derived(yamlText !== originalYaml);

  $effect(() => {
    const dirty = bodyDirty || fmDirty;
    untrack(() => workspace.setTabDirty(tabId, dirty));
  });

  function parseFrontmatter(raw) {
    const norm = raw.replace(/\r\n/g, '\n');
    if (!norm.startsWith('---\n')) return { yaml: '', body: norm };
    const rest = norm.slice(4);
    const closeMatch = rest.match(/\n---(\n|$)/);
    if (!closeMatch) return { yaml: '', body: norm };
    const closeIdx = closeMatch.index;
    const yaml = rest.slice(0, closeIdx);
    let bodyPart = rest.slice(closeIdx + closeMatch[0].length);
    if (bodyPart.startsWith('\n')) bodyPart = bodyPart.slice(1);
    return { yaml, body: bodyPart };
  }

  function serializeFrontmatter(yaml) {
    const trimmed = yaml.trim();
    if (!trimmed) return '';
    return `---\n${trimmed}\n---\n\n`;
  }

  $effect(() => {
    if (projectPath && relPath) void load();
  });

  async function load() {
    loading = true;
    loadError = '';
    try {
      const raw = await readProjectFile(projectPath, relPath);
      const { yaml, body: b } = parseFrontmatter(raw);
      yamlText = yaml;
      originalYaml = yaml;
      fmOpen = yaml.trim().length > 0;
      body = b;
    } catch (e) {
      loadError = e?.message ?? String(e);
    } finally {
      loading = false;
    }
  }

  async function handleSave(bodyMd) {
    const fm = serializeFrontmatter(yamlText);
    await writeProjectFile(projectPath, relPath, fm + bodyMd);
    originalYaml = yamlText;
  }

</script>

{#snippet fmButton()}
  <button
    type="button"
    onclick={(e) => { e.stopPropagation(); fmOpen = !fmOpen; }}
    class="flex items-center gap-1 px-2 py-1 rounded text-[11px] text-muted-foreground hover:text-foreground hover:bg-muted/40 transition-colors shrink-0"
    title="{fmOpen ? 'Hide frontmatter' : 'Show frontmatter'}"
  >
    <Tags size={11} class="shrink-0" />
    <span>FM</span>
    {#if fmDirty}<span class="w-1.5 h-1.5 rounded-full bg-amber-500 shrink-0 ml-0.5"></span>{/if}
    {#if fmOpen}<ChevronDown size={9} class="shrink-0" />{:else}<ChevronRight size={9} class="shrink-0" />{/if}
  </button>
{/snippet}

{#snippet fmPanel()}
  {#if fmOpen}
    <div class="border-b shrink-0 bg-muted/20 max-h-[45vh] overflow-y-auto">
      <div class="px-4 py-3">
        <FrontmatterEditor
          value={yamlText}
          onchange={(yaml) => { yamlText = yaml; }}
        />
        {#if fmDirty}
          <button
            type="button"
            onclick={() => { yamlText = originalYaml; }}
            class="mt-2 text-[11px] text-muted-foreground hover:text-foreground transition-colors"
          >↩ Discard FM changes</button>
        {/if}
      </div>
    </div>
  {/if}
{/snippet}

{#if loading}
  <div class="h-full flex items-center justify-center gap-2 text-muted-foreground">
    <Loader2 size={14} class="animate-spin" />
    <span class="text-xs">Loading…</span>
  </div>
{:else if loadError}
  <div class="m-8 border border-destructive bg-destructive/10 rounded-lg p-5 flex items-start gap-3">
    <XCircle class="w-4 h-4 text-destructive mt-0.5 shrink-0" />
    <pre class="text-xs text-muted-foreground whitespace-pre-wrap">{loadError}</pre>
  </div>
{:else}
  <div class="h-full flex flex-col overflow-hidden">
    <MarkdownEditor
      bind:this={editorRef}
      content={body}
      title={relPath.split('/').pop()}
      onSave={handleSave}
      onDirtyChange={(d) => { bodyDirty = d; }}
      extraDirty={fmDirty}
      headerExtra={fmButton}
      belowHeader={fmPanel}
    />
  </div>
{/if}
