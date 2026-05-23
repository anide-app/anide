<script>
  // @ts-nocheck
  let { data, tabId } = $props();

  import { readProjectFile, writeProjectFile, readProjectFileB64 } from '$lib/commands/files.js';
  import { workspace } from '$lib/stores/workspace.svelte.js';
  import { Loader2 } from '@lucide/svelte';
  import { untrack } from 'svelte';
  import FileEditor from '$lib/components/FileEditor.svelte';
  import ImagePreview from '$lib/components/ImagePreview.svelte';

  let editorRef = $state(null);

  $effect(() => {
    if (workspace.activeTabId === tabId && !isImage) {
      requestAnimationFrame(() => editorRef?.focus());
    }
  });

  $effect(() => {
    function onWindowFocus() {
      if (workspace.activeTabId === tabId && !isImage) requestAnimationFrame(() => editorRef?.focus());
    }
    window.addEventListener('focus', onWindowFocus);
    return () => window.removeEventListener('focus', onWindowFocus);
  });

  const IMAGE_EXTS = new Set([
    'png','jpg','jpeg','gif','svg','webp','bmp','ico','tiff','tif','avif','heic',
  ]);
  const MIME = {
    png: 'image/png', jpg: 'image/jpeg', jpeg: 'image/jpeg',
    gif: 'image/gif', svg: 'image/svg+xml', webp: 'image/webp',
    bmp: 'image/bmp', ico: 'image/x-icon', tiff: 'image/tiff',
    tif: 'image/tiff', avif: 'image/avif', heic: 'image/heic',
  };

  let projectPath = $derived(data.projectPath);
  let relPath     = $derived(data.relPath);
  let language    = $derived(data.language);
  let title       = $derived(relPath ?? '');
  let ext         = $derived(relPath?.split('.').pop()?.toLowerCase() ?? '');
  let isImage     = $derived(IMAGE_EXTS.has(ext));

  let externalChangeTick = $derived(workspace.fileChangeTicks[relPath] ?? 0);

  // ── Image loading ──────────────────────────────────────────────────────────
  let imgSrc     = $state('');
  let imgLoading = $state(false);

  $effect(() => {
    if (!isImage || !projectPath || !relPath) return;
    externalChangeTick; // track for reload on external change
    imgLoading = true;
    imgSrc = '';
    const p = projectPath, r = relPath, e = ext;
    untrack(() => readProjectFileB64(p, r))
      .then(b64 => {
        const mime = MIME[e] ?? 'image/octet-stream';
        imgSrc = `data:${mime};base64,${b64}`;
      })
      .catch(() => { imgSrc = ''; })
      .finally(() => { imgLoading = false; });
  });

  // ── File editor ────────────────────────────────────────────────────────────
  const load = () => readProjectFile(projectPath, relPath);
  const save = (content) => writeProjectFile(projectPath, relPath, content);
</script>

{#if isImage}
  {#if imgLoading}
    <div class="h-full flex items-center justify-center text-muted-foreground">
      <Loader2 size={16} class="animate-spin" />
    </div>
  {:else if imgSrc}
    <ImagePreview {title} src={imgSrc} />
  {/if}
{:else}
  <FileEditor
    bind:this={editorRef}
    {title}
    {load}
    {save}
    language={language}
    {externalChangeTick}
    onDirtyChange={(d) => workspace.setTabDirty(tabId, d)}
  />
{/if}
