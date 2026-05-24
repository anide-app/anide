<script>
  // @ts-nocheck
  import { AlertTriangle, CheckCircle } from '@lucide/svelte';
  import * as Dialog from '$lib/components/ui/dialog/index.js';

  let {
    title = 'Confirm',
    sql = '',
    summary = '',
    destructive = false,
    onConfirm,
    onCancel,
  } = $props();

  let running = $state(false);
  let open = $state(true);

  async function confirm() {
    running = true;
    try { await onConfirm(); } finally { running = false; }
  }

  function handleOpenChange(v) {
    if (!v && !running) onCancel();
  }
</script>

<Dialog.Root {open} onOpenChange={handleOpenChange}>
  <Dialog.Content
    class="max-w-lg"
    onkeydown={(e) => { if (e.key === 'Enter' && (e.ctrlKey || e.metaKey) && !running) confirm(); }}
  >
    <Dialog.Header>
      <Dialog.Title class="flex items-center gap-2">
        {#if destructive}
          <AlertTriangle size={15} class="text-destructive shrink-0" />
        {:else}
          <CheckCircle size={15} class="text-primary shrink-0" />
        {/if}
        {title}
      </Dialog.Title>
      {#if summary}
        <Dialog.Description>{summary}</Dialog.Description>
      {/if}
    </Dialog.Header>

    <div class="rounded-md border border-border overflow-hidden">
      <div class="bg-muted/40 px-3 py-1.5 border-b border-border">
        <span class="text-[11px] font-mono text-muted-foreground uppercase tracking-wide">SQL to execute</span>
      </div>
      <pre class="px-3 py-3 text-xs font-mono overflow-x-auto bg-background text-foreground whitespace-pre-wrap break-all max-h-48">{sql}</pre>
    </div>

    <Dialog.Footer>
      <button type="button" onclick={onCancel} disabled={running}
        class="px-3 py-1.5 text-xs rounded border border-border hover:bg-muted transition-colors disabled:opacity-50">
        Cancel
      </button>
      <button type="button" onclick={confirm} disabled={running}
        class="px-3 py-1.5 text-xs rounded font-medium transition-colors disabled:opacity-50 {destructive
          ? 'bg-destructive text-destructive-foreground hover:bg-destructive/90'
          : 'bg-primary text-primary-foreground hover:bg-primary/90'}">
        {running ? 'Executing…' : destructive ? 'Execute (destructive)' : 'Execute'}
      </button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
