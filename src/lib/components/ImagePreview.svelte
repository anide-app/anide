<script>
  // @ts-nocheck
  let { title, src } = $props();

  import { Image as ImageIcon, ZoomIn, ZoomOut, Maximize2 } from '@lucide/svelte';

  let imgEl    = $state(null);
  let naturalW = $state(0);
  let naturalH = $state(0);
  let fitMode  = $state(true);
  let zoom     = $state(1);

  function onLoad() {
    if (!imgEl) return;
    naturalW = imgEl.naturalWidth;
    naturalH = imgEl.naturalHeight;
  }

  function zoomIn() {
    fitMode = false;
    zoom = Math.min(16, parseFloat((zoom + 0.25).toFixed(2)));
  }

  function zoomOut() {
    if (fitMode) return;
    const next = parseFloat((zoom - 0.25).toFixed(2));
    if (next <= 0.1) { fitMode = true; zoom = 1; }
    else zoom = next;
  }

  function onWheel(e) {
    e.preventDefault();
    if (e.deltaY < 0) zoomIn(); else zoomOut();
  }
</script>

<div class="h-full flex flex-col overflow-hidden">
  <!-- Header -->
  <div class="flex items-center gap-2 px-4 border-b shrink-0 h-10">
    <ImageIcon size={14} class="text-muted-foreground shrink-0" />
    <span class="text-sm font-medium flex-1 truncate font-mono">{title}</span>
    {#if naturalW && naturalH}
      <span class="text-xs text-muted-foreground/60 shrink-0 font-mono">{naturalW}×{naturalH}</span>
    {/if}
    <div class="flex items-center gap-0.5 ml-2">
      <button
        type="button"
        onclick={zoomOut}
        class="h-7 w-7 flex items-center justify-center rounded text-muted-foreground
               hover:text-foreground hover:bg-muted transition-colors"
      ><ZoomOut size={13} /></button>
      <span class="text-[11px] text-muted-foreground w-10 text-center select-none">
        {fitMode ? 'fit' : `${Math.round(zoom * 100)}%`}
      </span>
      <button
        type="button"
        onclick={zoomIn}
        class="h-7 w-7 flex items-center justify-center rounded text-muted-foreground
               hover:text-foreground hover:bg-muted transition-colors"
      ><ZoomIn size={13} /></button>
      <button
        type="button"
        onclick={() => { fitMode = true; zoom = 1; }}
        title="Fit to view"
        class="h-7 w-7 flex items-center justify-center rounded transition-colors
          {fitMode ? 'text-primary' : 'text-muted-foreground hover:text-foreground hover:bg-muted'}"
      ><Maximize2 size={13} /></button>
    </div>
  </div>

  <!-- Canvas -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="flex-1 overflow-auto flex items-center justify-center p-6 checkerboard"
    onwheel={onWheel}
  >
    <img
      bind:this={imgEl}
      {src}
      alt={title}
      onload={onLoad}
      draggable="false"
      class="select-none rounded-sm shadow-md"
      class:max-w-full={fitMode}
      class:max-h-full={fitMode}
      style={fitMode
        ? 'object-fit: contain;'
        : `width: ${naturalW * zoom}px; height: ${naturalH * zoom}px;`}
    />
  </div>
</div>

<style>
  .checkerboard {
    background-color: var(--background);
    background-image:
      linear-gradient(45deg, color-mix(in oklch, var(--muted) 80%, transparent) 25%, transparent 25%),
      linear-gradient(-45deg, color-mix(in oklch, var(--muted) 80%, transparent) 25%, transparent 25%),
      linear-gradient(45deg, transparent 75%, color-mix(in oklch, var(--muted) 80%, transparent) 75%),
      linear-gradient(-45deg, transparent 75%, color-mix(in oklch, var(--muted) 80%, transparent) 75%);
    background-size: 20px 20px;
    background-position: 0 0, 0 10px, 10px -10px, -10px 0;
  }
</style>
