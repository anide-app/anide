<script>
  // @ts-nocheck
  import { onMount } from 'svelte';
  import { EditorView, keymap, lineNumbers } from '@codemirror/view';
  import { EditorState } from '@codemirror/state';
  import { defaultKeymap, historyKeymap, history } from '@codemirror/commands';
  import { syntaxHighlighting, defaultHighlightStyle } from '@codemirror/language';
  import { sql, PostgreSQL } from '@codemirror/lang-sql';
  import { dbQueryPage, dbSaveQuery } from '$lib/commands/db.js';
  import { workspace } from '$lib/stores/workspace.svelte.js';
  import { Input } from '$lib/components/ui/input/index.js';
  import * as Dialog from '$lib/components/ui/dialog/index.js';
  import { Play, Save, Loader2, Download } from '@lucide/svelte';

  let { data } = $props();
  const conn        = data.conn;
  const folderPath  = data.folderPath;
  // Read at save time via data so we always use the live prop, not a stale captured const.
  const queryCollection = $derived(data.queryCollection ?? null);

  let editorEl  = $state(null);
  let view;
  let queryText = $state(data.initialSql ?? 'SELECT 1;');

  let result  = $state(null);
  let running = $state(false);
  let error   = $state('');

  let saveDialogOpen = $state(false);
  let saveForm = $state({ name: data.queryName ?? '', description: data.queryDescription ?? '' });
  // tracks filename of the last-saved query (for overwrite on re-save)
  let queryFileName = $state(data.queryFileName ?? null);

  const PAGE_SIZE   = 200;
  let loadingMore   = $state(false);
  let currentOffset = $state(0);
  let hasMore       = $state(false);
  let lastSql       = $state('');

  // ── Virtual scroll ────────────────────────────────────────────────────────────
  const ROW_H    = 28;
  const OVERSCAN = 8;

  let scrollEl  = $state(null);
  let scrollTop = $state(0);
  let viewH     = $state(300);

  const qRows = $derived(result?.rows ?? []);
  const qCols = $derived(result?.columns ?? []);

  const vStart       = $derived(Math.max(0, Math.floor(scrollTop / ROW_H) - OVERSCAN));
  const vEnd         = $derived(Math.min(qRows.length, Math.ceil((scrollTop + viewH) / ROW_H) + OVERSCAN));
  const visibleQRows = $derived(qRows.slice(vStart, vEnd));
  const qPadTop      = $derived(vStart * ROW_H);
  const qPadBottom   = $derived(Math.max(0, (qRows.length - vEnd) * ROW_H));

  $effect(() => {
    if (!scrollEl) return;
    const el = scrollEl;
    const updateSize = () => { viewH = el.clientHeight; };
    const onScroll = () => {
      scrollTop = el.scrollTop;
      if (hasMore && !loadingMore && !running &&
          el.scrollHeight - el.scrollTop - el.clientHeight < 300) loadMore();
    };
    const ro = new ResizeObserver(updateSize);
    updateSize();
    el.addEventListener('scroll', onScroll, { passive: true });
    ro.observe(el);
    return () => { el.removeEventListener('scroll', onScroll); ro.disconnect(); };
  });

  // ── Column width locking + resizing ──────────────────────────────────────────
  let colWidths = $state(null);
  let theadEl   = $state(null);

  $effect(() => { qCols; colWidths = null; });

  $effect(() => {
    if (!result || qRows.length === 0 || colWidths !== null || !theadEl) return;
    requestAnimationFrame(() => {
      if (!theadEl || colWidths !== null) return;
      const widths = Array.from(theadEl.querySelectorAll('th'))
        .map(th => Math.min(th.getBoundingClientRect().width, 256));
      if (widths.length > 0 && widths.every(w => w > 0)) colWidths = widths;
    });
  });

  function onResizeStart(e, colIdx) {
    if (!colWidths) return;
    e.preventDefault();
    const startX = e.clientX;
    const startW = colWidths[colIdx];
    const onMove = (ev) => {
      const next = [...colWidths];
      next[colIdx] = Math.max(40, startW + (ev.clientX - startX));
      colWidths = next;
    };
    const onUp = () => {
      window.removeEventListener('mousemove', onMove);
      window.removeEventListener('mouseup', onUp);
    };
    window.addEventListener('mousemove', onMove);
    window.addEventListener('mouseup', onUp);
  }

  // ── Editor setup ──────────────────────────────────────────────────────────────
  const appTheme = EditorView.theme({
    '&': { height: '100%' },
    '.cm-scroller': {
      overflow: 'auto',
      fontFamily: "'Geist Mono', ui-monospace, monospace",
      fontSize: '12px',
      lineHeight: '1.65',
    },
    '.cm-content': { padding: '8px 4px', caretColor: 'var(--primary)' },
    '.cm-gutters': {
      backgroundColor: 'var(--muted)',
      color: 'var(--muted-foreground)',
      border: 'none',
      borderRight: '1px solid var(--border)',
    },
    '.cm-lineNumbers .cm-gutterElement': { padding: '0 10px 0 6px' },
    '.cm-activeLine': { backgroundColor: 'color-mix(in oklch, var(--foreground) 4%, transparent)' },
    '.cm-activeLineGutter': { backgroundColor: 'color-mix(in oklch, var(--foreground) 6%, transparent)' },
    '&.cm-focused .cm-selectionBackground, .cm-selectionBackground': {
      backgroundColor: 'color-mix(in oklch, var(--primary) 20%, transparent) !important',
    },
    '.cm-cursor, .cm-dropCursor': { borderLeftColor: 'var(--primary)' },
    '.cm-matchingBracket': { backgroundColor: 'color-mix(in oklch, var(--primary) 15%, transparent)' },
  }, { dark: false });

  onMount(() => {
    view = new EditorView({
      state: EditorState.create({
        doc: queryText,
        extensions: [
          history(),
          lineNumbers(),
          syntaxHighlighting(defaultHighlightStyle),
          appTheme,
          sql({ dialect: PostgreSQL }),
          keymap.of([
            ...defaultKeymap,
            ...historyKeymap,
            { key: 'Ctrl-Enter', run: () => { runQuery(); return true; } },
            { key: 'Mod-Enter', run: () => { runQuery(); return true; } },
          ]),
          EditorView.updateListener.of(u => {
            if (u.docChanged) queryText = u.state.doc.toString();
          }),
        ],
      }),
      parent: editorEl,
    });
    return () => view?.destroy();
  });

  // ── Query execution ───────────────────────────────────────────────────────────
  async function runQuery() {
    const sqlToRun = getSelectedOrAll();
    if (!sqlToRun.trim()) return;
    running = true; error = ''; result = null;
    lastSql = sqlToRun;
    currentOffset = 0;
    colWidths = null;
    scrollTop = 0;
    if (scrollEl) scrollEl.scrollTop = 0;
    try {
      const r = await dbQueryPage(conn, sqlToRun, 0, PAGE_SIZE);
      result = r;
      currentOffset = r.rows.length;
      hasMore = r.rows.length === PAGE_SIZE;
    } catch (e) {
      error = e?.message ?? String(e);
    } finally {
      running = false;
    }
  }

  async function loadMore() {
    if (!hasMore || loadingMore) return;
    loadingMore = true;
    try {
      const r = await dbQueryPage(conn, lastSql, currentOffset, PAGE_SIZE);
      result = { ...result, rows: [...result.rows, ...r.rows] };
      currentOffset += r.rows.length;
      hasMore = r.rows.length === PAGE_SIZE;
    } catch (e) { error = e?.message ?? String(e); }
    finally { loadingMore = false; }
  }

  function getSelectedOrAll() {
    if (!view) return queryText;
    const sel = view.state.selection.main;
    if (!sel.empty) return view.state.doc.sliceString(sel.from, sel.to);
    return view.state.doc.toString();
  }

  // ── Save query ────────────────────────────────────────────────────────────────
  function openSaveDialog() {
    // Pre-fill with existing name if re-saving
    saveDialogOpen = true;
  }

  async function saveQuery() {
    if (!saveForm.name.trim()) return;
    const collection = data.queryCollection ?? null;
    try {
      const fileName = await dbSaveQuery(folderPath, conn, { name: saveForm.name, description: saveForm.description, sql: queryText, collection });
      saveDialogOpen = false;
      queryFileName = fileName;
      // Update tab title to match query name
      if (data.tabId) workspace.renameTab(data.tabId, saveForm.name);
      // Tell DbPanel to refresh the queries list
      window.dispatchEvent(new CustomEvent('db-query-saved', { detail: { conn } }));
    } catch (e) { error = e?.message ?? String(e); }
  }

  // ── Export ────────────────────────────────────────────────────────────────────
  function exportCSV() {
    if (!result) return;
    const header = result.columns.join(',');
    const body = result.rows.map(r => r.map(v => {
      if (v === null) return 'NULL';
      const s = String(v);
      return s.includes(',') || s.includes('"') ? `"${s.replace(/"/g, '""')}"` : s;
    }).join(',')).join('\n');
    const blob = new Blob([header + '\n' + body], { type: 'text/csv' });
    const a = document.createElement('a');
    a.href = URL.createObjectURL(blob);
    a.download = `${saveForm.name || 'query-result'}.csv`;
    a.click();
  }
</script>

<div class="h-full flex flex-col overflow-hidden">
  <!-- Toolbar -->
  <div class="flex items-center gap-1 px-2 py-1.5 border-b shrink-0">
    <span class="text-xs text-muted-foreground mr-1">{conn}</span>
    <button type="button" onclick={runQuery} disabled={running}
      class="flex items-center gap-1 px-2 py-1 text-xs rounded bg-primary text-primary-foreground hover:bg-primary/90 transition-colors disabled:opacity-50">
      {#if running}<Loader2 size={11} class="animate-spin" />{:else}<Play size={11} />{/if}
      Run
    </button>
    <button type="button" onclick={openSaveDialog}
      class="flex items-center gap-1 px-2 py-1 text-xs rounded border border-border hover:bg-muted transition-colors">
      <Save size={11} />{queryFileName ? 'Save' : 'Save…'}
    </button>
    <div class="flex-1"></div>
    {#if result}
      <span class="text-xs text-muted-foreground">{qRows.length}{hasMore ? '+' : ''} rows · {result.duration_ms}ms</span>
      <button type="button" onclick={exportCSV}
        class="flex items-center gap-1 px-2 py-1 text-xs rounded border border-border hover:bg-muted transition-colors">
        <Download size={11} />CSV
      </button>
    {/if}
  </div>

  <!-- Split: editor + results -->
  <div class="flex-1 flex flex-col overflow-hidden">
    <!-- Editor -->
    <div class="h-48 border-b shrink-0 overflow-hidden" bind:this={editorEl}></div>

    <!-- Results -->
    <div class="flex-1 overflow-auto" bind:this={scrollEl}>
      {#if error}
        <div class="px-4 py-3 text-xs text-destructive bg-destructive/10">{error}</div>
      {:else if result}
        {#if qRows.length === 0}
          <div class="flex items-center justify-center h-20 text-xs text-muted-foreground">Query returned no rows</div>
        {:else}
          <table
            class="w-full border-collapse text-xs"
            style:table-layout={colWidths ? 'fixed' : 'auto'}
          >
            {#if colWidths}
              <colgroup>
                {#each colWidths as w}
                  <col style:width="{w}px" />
                {/each}
              </colgroup>
            {/if}
            <thead class="sticky top-0 z-10 bg-background border-b border-border" bind:this={theadEl}>
              <tr>
                {#each qCols as col, i (col)}
                  <th class="px-2 py-1.5 text-left font-medium text-muted-foreground border-r border-border whitespace-nowrap min-w-16 relative">
                    <span class="block truncate pr-2">{col}</span>
                    {#if colWidths}
                      <div
                        class="absolute right-0 top-0 h-full w-1.5 cursor-col-resize hover:bg-primary/40 z-10"
                        role="separator"
                        onmousedown={(e) => onResizeStart(e, i)}
                      ></div>
                    {/if}
                  </th>
                {/each}
              </tr>
            </thead>
            <tbody>
              {#if qPadTop > 0}
                <tr style:height="{qPadTop}px" aria-hidden="true">
                  <td colspan={qCols.length}></td>
                </tr>
              {/if}

              {#each visibleQRows as row, i (vStart + i)}
                <tr class="border-b border-border/40 hover:bg-muted/20" style:height="{ROW_H}px">
                  {#each row as val}
                    <td class="px-2 py-0 border-r border-border/40 overflow-hidden align-middle">
                      {#if val === null}
                        <span class="italic text-muted-foreground/50 text-[11px]">NULL</span>
                      {:else}
                        <span class="truncate block">{typeof val === 'object' ? JSON.stringify(val) : String(val)}</span>
                      {/if}
                    </td>
                  {/each}
                </tr>
              {/each}

              {#if qPadBottom > 0}
                <tr style:height="{qPadBottom}px" aria-hidden="true">
                  <td colspan={qCols.length}></td>
                </tr>
              {/if}
            </tbody>
          </table>
          {#if loadingMore}
            <div class="flex items-center justify-center py-2 text-muted-foreground">
              <Loader2 size={12} class="animate-spin" />
            </div>
          {/if}
        {/if}
      {:else if !running}
        <div class="flex items-center justify-center h-20 text-xs text-muted-foreground">
          Press Ctrl+Enter to run query
        </div>
      {/if}
    </div>
  </div>
</div>

<!-- Save query dialog -->
<Dialog.Root bind:open={saveDialogOpen}>
  <Dialog.Content class="max-w-sm">
    <Dialog.Header>
      <Dialog.Title>{queryFileName ? 'Save Query' : 'Save Query As…'}</Dialog.Title>
      {#if queryFileName}
        <Dialog.Description>Changes will overwrite the existing saved query.</Dialog.Description>
      {/if}
    </Dialog.Header>
    <div class="flex flex-col gap-3 py-2">
      <div class="flex flex-col gap-1.5">
        <label class="text-xs text-muted-foreground">Name</label>
        <Input bind:value={saveForm.name} placeholder="My Query" class="h-8 text-xs" />
      </div>
      <div class="flex flex-col gap-1.5">
        <label class="text-xs text-muted-foreground">Description (optional)</label>
        <Input bind:value={saveForm.description} placeholder="What does this query do?" class="h-8 text-xs" />
      </div>
      <div class="text-xs text-muted-foreground">
        Saves to: <span class="font-mono text-foreground">{conn}/queries{data.queryCollection ? `/${data.queryCollection}` : ''}</span>
      </div>
    </div>
    <Dialog.Footer>
      <button type="button" onclick={() => (saveDialogOpen = false)}
        class="px-3 py-1.5 text-xs rounded border border-border hover:bg-muted transition-colors">Cancel</button>
      <button type="button" onclick={saveQuery} disabled={!saveForm.name}
        class="px-3 py-1.5 text-xs rounded bg-primary text-primary-foreground hover:bg-primary/90 disabled:opacity-50 transition-colors">Save</button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
