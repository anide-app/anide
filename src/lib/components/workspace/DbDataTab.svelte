<script>
  // @ts-nocheck
  import { onMount } from 'svelte';
  import {
    dbQueryPage, dbCountTable, dbUpdateRow, dbInsertRow,
    dbDeleteRows, dbPreviewInsert, dbPreviewDelete, dbListColumns,
  } from '$lib/commands/db.js';
  import SqlConfirmModal from '$lib/components/db/SqlConfirmModal.svelte';
  import { Checkbox } from '$lib/components/ui/checkbox/index.js';
  import { Input } from '$lib/components/ui/input/index.js';
  import { RefreshCw, Hash, Plus, Trash2, Download, Loader2, X } from '@lucide/svelte';

  let { data } = $props();
  const { conn, db, schema, table } = data;

  const PAGE_SIZE = 200;
  const ROW_H = 28;
  const OVERSCAN = 8;

  let rows       = $state([]);
  let columns    = $state([]);
  let offset     = $state(0);
  let totalCount = $state(null);
  let loading    = $state(true);
  let loadingMore = $state(false);
  let hasMore    = $state(true);
  let error      = $state('');

  let colMetas = $state([]);
  let pkCol    = $derived(colMetas.find(c => c.is_primary)?.name ?? colMetas[0]?.name ?? null);

  let selectedRows = $state(new Set());
  let confirmModal = $state(null);

  // ── Sidebar ──────────────────────────────────────────────────────────────────
  let sidebarMode   = $state(null); // null | 'edit' | 'add'
  let sidebarRow    = $state(null);
  let sidebarValues = $state({});

  function openRow(rowIndex) {
    sidebarMode = 'edit';
    sidebarRow = rowIndex;
    sidebarValues = Object.fromEntries(columns.map((c, i) => {
      const v = rows[rowIndex][i];
      return [c, v === null ? '' : typeof v === 'object' ? JSON.stringify(v) : String(v)];
    }));
  }

  function openAddRow() {
    sidebarMode = 'add';
    sidebarRow = null;
    sidebarValues = Object.fromEntries(colMetas.map(c => [c.name, '']));
  }

  function closeSidebar() { sidebarMode = null; sidebarRow = null; sidebarValues = {}; }

  function sqlLiteral(val) {
    if (val === null || val === undefined) return 'NULL';
    if (typeof val === 'boolean') return val ? '1' : '0';
    if (typeof val === 'number') return String(val);
    return `'${String(val).replace(/'/g, "''")}'`;
  }

  async function commitSidebarEdit() {
    if (!pkCol) { closeSidebar(); return; }
    const pkVal = rows[sidebarRow][columns.indexOf(pkCol)];
    const changes = columns
      .map((col, i) => ({ col, orig: rows[sidebarRow][i], newVal: parseValue(String(sidebarValues[col] ?? '')) }))
      .filter(({ col, orig, newVal }) => col !== pkCol && JSON.stringify(orig) !== JSON.stringify(newVal));
    if (changes.length === 0) { closeSidebar(); return; }
    const setStr = changes.map(({ col, newVal }) => `"${col}" = ${sqlLiteral(newVal)}`).join(',\n    ');
    const sql = `UPDATE "${table}"\nSET ${setStr}\nWHERE "${pkCol}" = ${sqlLiteral(pkVal)};`;
    confirmModal = {
      title: 'Confirm Update', sql,
      summary: `Updating ${changes.length} field(s) in "${table}"`,
      destructive: false,
      onConfirm: async () => {
        for (const { col, newVal } of changes) {
          await dbUpdateRow(conn, { database: db, schema, table, pk_column: pkCol, pk_value: pkVal, column: col, new_value: newVal });
        }
        confirmModal = null;
        const updated = [...rows[sidebarRow]];
        for (const { col, newVal } of changes) { updated[columns.indexOf(col)] = newVal; }
        rows[sidebarRow] = updated;
        closeSidebar();
      },
    };
  }

  async function commitSidebarAdd() {
    const colNames = colMetas.map(c => c.name);
    const values = colNames.map(c => parseValue(String(sidebarValues[c] ?? '')));
    const op = { database: db, schema, table, columns: colNames, values };
    try {
      const sql = await dbPreviewInsert(op);
      confirmModal = {
        title: 'Confirm Insert', sql,
        summary: `Inserting 1 row into "${table}"`,
        destructive: false,
        onConfirm: async () => {
          await dbInsertRow(conn, op);
          confirmModal = null;
          closeSidebar();
          await load();
        },
      };
    } catch (e) { error = e?.message ?? String(e); }
  }

  // ── Virtual scroll ────────────────────────────────────────────────────────────
  let scrollEl  = $state(null);
  let scrollTop = $state(0);
  let viewH     = $state(500);

  const vStart     = $derived(Math.max(0, Math.floor(scrollTop / ROW_H) - OVERSCAN));
  const vEnd       = $derived(Math.min(rows.length, Math.ceil((scrollTop + viewH) / ROW_H) + OVERSCAN));
  const visibleRows = $derived(rows.slice(vStart, vEnd));
  const padTop     = $derived(vStart * ROW_H);
  const padBottom  = $derived(Math.max(0, (rows.length - vEnd) * ROW_H));

  $effect(() => {
    if (!scrollEl) return;
    const el = scrollEl;
    const updateSize = () => { viewH = el.clientHeight; };
    const onScroll = () => {
      scrollTop = el.scrollTop;
      if (hasMore && !loadingMore && !loading &&
          el.scrollHeight - el.scrollTop - el.clientHeight < 500) loadMore();
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

  $effect(() => { columns; colWidths = null; });

  $effect(() => {
    if (loading || rows.length === 0 || colWidths !== null || !theadEl) return;
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

  // ── Data loading ──────────────────────────────────────────────────────────────
  const baseSQL = `SELECT * FROM "${schema}"."${table}"`;

  async function load(reset = true) {
    if (reset) { offset = 0; rows = []; hasMore = true; selectedRows = new Set(); closeSidebar(); }
    loading = true; error = '';
    try {
      const r = await dbQueryPage(conn, baseSQL, reset ? 0 : offset, PAGE_SIZE);
      columns = r.columns;
      rows = reset ? r.rows : [...rows, ...r.rows];
      hasMore = r.rows.length === PAGE_SIZE;
      offset = rows.length;
    } catch (e) { error = e?.message ?? String(e); }
    finally { loading = false; }
  }

  async function loadMore() {
    if (!hasMore || loadingMore) return;
    loadingMore = true;
    try {
      const r = await dbQueryPage(conn, baseSQL, offset, PAGE_SIZE);
      rows = [...rows, ...r.rows];
      hasMore = r.rows.length === PAGE_SIZE;
      offset = rows.length;
    } catch (e) { error = e?.message ?? String(e); }
    finally { loadingMore = false; }
  }

  async function loadColumns() {
    try { colMetas = await dbListColumns(conn, db, schema, table); } catch {}
  }

  onMount(() => { Promise.all([load(), loadColumns()]); });

  // ── Count ─────────────────────────────────────────────────────────────────────
  async function fetchCount() {
    try { totalCount = await dbCountTable(conn, db, schema, table); }
    catch (e) { totalCount = `Error: ${e?.message ?? e}`; }
  }

  // ── Delete ────────────────────────────────────────────────────────────────────
  async function deleteSelected() {
    if (!pkCol) return;
    const pkValues = [...selectedRows].map(i => rows[i][columns.indexOf(pkCol)]);
    const op = { database: db, schema, table, pk_column: pkCol, pk_values: pkValues };
    try {
      const sql = await dbPreviewDelete(op);
      confirmModal = {
        title: `Delete ${pkValues.length} row${pkValues.length > 1 ? 's' : ''}`, sql,
        summary: `Deleting ${pkValues.length} row(s) from "${table}"`,
        destructive: true,
        onConfirm: async () => {
          await dbDeleteRows(conn, op);
          confirmModal = null;
          selectedRows = new Set();
          await load();
        },
      };
    } catch (e) { error = e?.message ?? String(e); }
  }

  // ── Export ────────────────────────────────────────────────────────────────────
  function exportCSV() {
    const header = columns.join(',');
    const body = rows.map(r => r.map(v => {
      if (v === null) return 'NULL';
      const s = String(v);
      return s.includes(',') || s.includes('"') || s.includes('\n') ? `"${s.replace(/"/g, '""')}"` : s;
    }).join(',')).join('\n');
    const blob = new Blob([header + '\n' + body], { type: 'text/csv' });
    const a = document.createElement('a');
    a.href = URL.createObjectURL(blob);
    a.download = `${table}.csv`;
    a.click();
  }

  // ── Helpers ───────────────────────────────────────────────────────────────────
  function parseValue(s) {
    if (!s || s.toUpperCase() === 'NULL') return null;
    if (s === 'true') return true;
    if (s === 'false') return false;
    const n = Number(s);
    if (!isNaN(n) && s.trim() !== '') return n;
    return s;
  }

  function displayValue(v) {
    if (v === null || v === undefined) return null;
    if (typeof v === 'object') return JSON.stringify(v);
    return String(v);
  }

  function toggleRowSelection(i, e) {
    e?.stopPropagation();
    const next = new Set(selectedRows);
    if (next.has(i)) next.delete(i); else next.add(i);
    selectedRows = next;
  }

  function toggleAllRows() {
    selectedRows = selectedRows.size === rows.length
      ? new Set()
      : new Set(rows.map((_, i) => i));
  }

  const allSelected  = $derived(rows.length > 0 && selectedRows.size === rows.length);
  const someSelected = $derived(selectedRows.size > 0 && selectedRows.size < rows.length);
</script>

<div class="h-full flex flex-col overflow-hidden text-xs">
  <!-- Toolbar -->
  <div class="flex items-center gap-2 px-3 py-1.5 border-b shrink-0 bg-background">
    <div class="flex items-center gap-1 text-muted-foreground">
      <span>{conn}</span>
      <span class="opacity-40">›</span><span>{db}</span>
      {#if schema && schema !== db}<span class="opacity-40">›</span><span>{schema}</span>{/if}
      <span class="opacity-40">›</span><span class="text-foreground font-medium">{table}</span>
    </div>
    <div class="flex-1"></div>
    <button type="button" onclick={fetchCount}
      class="flex items-center gap-1 px-2 py-1 rounded border border-border hover:bg-muted transition-colors">
      <Hash size={11} />{totalCount !== null ? totalCount : 'Count'}
    </button>
    {#if selectedRows.size > 0}
      <button type="button" onclick={deleteSelected}
        class="flex items-center gap-1 px-2 py-1 rounded text-destructive border border-destructive/30 hover:bg-destructive/10 transition-colors">
        <Trash2 size={11} />Delete {selectedRows.size}
      </button>
    {/if}
    <button type="button" onclick={openAddRow}
      class="flex items-center gap-1 px-2 py-1 rounded border border-border hover:bg-muted transition-colors">
      <Plus size={11} />Add Row
    </button>
    <button type="button" onclick={exportCSV}
      class="flex items-center gap-1 px-2 py-1 rounded border border-border hover:bg-muted transition-colors">
      <Download size={11} />CSV
    </button>
    <button type="button" onclick={() => load()} title="Refresh"
      class="p-1 rounded hover:bg-muted transition-colors text-muted-foreground">
      <RefreshCw size={12} class={loading ? 'animate-spin' : ''} />
    </button>
  </div>

  {#if error}
    <div class="px-3 py-2 text-destructive bg-destructive/10 border-b shrink-0">{error}</div>
  {/if}

  <!-- Main area: table + sidebar -->
  <div class="flex-1 flex flex-row overflow-hidden">
    <!-- Virtualized table -->
    <div class="flex-1 overflow-auto min-w-0" bind:this={scrollEl}>
      {#if loading && rows.length === 0}
        <div class="flex items-center justify-center h-32 gap-2 text-muted-foreground">
          <Loader2 size={16} class="animate-spin" /><span>Loading…</span>
        </div>
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
              <th class="w-8 px-2 py-1.5 border-r border-border text-center">
                <Checkbox
                  checked={allSelected ? true : someSelected ? 'indeterminate' : false}
                  onCheckedChange={toggleAllRows}
                />
              </th>
              {#each columns as col, i (col)}
                <th class="px-2 py-1.5 text-left font-medium text-muted-foreground border-r border-border whitespace-nowrap min-w-16 relative">
                  <span class="block truncate pr-2">{col}</span>
                  {#if colWidths}
                    <div
                      class="absolute right-0 top-0 h-full w-1.5 cursor-col-resize hover:bg-primary/40 z-10"
                      role="separator"
                      onmousedown={(e) => onResizeStart(e, i + 1)}
                    ></div>
                  {/if}
                </th>
              {/each}
            </tr>
          </thead>
          <tbody>
            {#if padTop > 0}
              <tr style:height="{padTop}px" aria-hidden="true">
                <td colspan={columns.length + 1}></td>
              </tr>
            {/if}

            {#each visibleRows as row, i (vStart + i)}
              {@const rowIndex = vStart + i}
              {@const isSelected = selectedRows.has(rowIndex)}
              {@const isActive = sidebarMode === 'edit' && sidebarRow === rowIndex}
              <tr
                style:height="{ROW_H}px"
                class="border-b border-border/40 transition-colors cursor-pointer {isActive ? 'bg-primary/10' : isSelected ? 'bg-primary/5' : 'hover:bg-muted/20'}"
                onclick={() => openRow(rowIndex)}
              >
                <td
                  class="w-8 px-2 py-0 border-r border-border/40 text-center align-middle"
                  onclick={(e) => toggleRowSelection(rowIndex, e)}
                  role="gridcell"
                >
                  <Checkbox checked={isSelected} onCheckedChange={() => {}} />
                </td>
                {#each columns as col, colIndex (col)}
                  {@const val = row[colIndex]}
                  <td class="px-0 py-0 border-r border-border/40 overflow-hidden align-middle" role="gridcell">
                    {#if val === null}
                      <span class="italic text-muted-foreground/40 text-[10px] px-2">NULL</span>
                    {:else}
                      <span class="truncate block px-2">{displayValue(val)}</span>
                    {/if}
                  </td>
                {/each}
              </tr>
            {/each}

            {#if padBottom > 0}
              <tr style:height="{padBottom}px" aria-hidden="true">
                <td colspan={columns.length + 1}></td>
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
    </div>

    <!-- Right sidebar -->
    {#if sidebarMode}
      <div class="w-72 border-l bg-background flex flex-col shrink-0 overflow-hidden">
        <div class="flex items-center justify-between px-3 py-2 border-b shrink-0">
          <span class="text-xs font-medium">{sidebarMode === 'add' ? 'Add Row' : 'Edit Row'}</span>
          <button type="button" onclick={closeSidebar}
            class="p-0.5 rounded hover:bg-muted transition-colors text-muted-foreground">
            <X size={14} />
          </button>
        </div>
        <div class="flex-1 overflow-y-auto px-3 py-3 flex flex-col gap-3">
          {#each colMetas as col (col.name)}
            <div class="flex flex-col gap-1">
              <label class="text-[11px] flex items-center gap-1.5">
                <span class="font-medium text-foreground/80">{col.name}</span>
                {#if col.is_primary}<span class="text-yellow-500 text-[10px]">PK</span>{/if}
                <span class="text-[10px] text-muted-foreground/60">{col.col_type}</span>
              </label>
              <Input
                type="text"
                placeholder={col.nullable ? 'NULL' : '(required)'}
                bind:value={sidebarValues[col.name]}
                disabled={sidebarMode === 'edit' && col.is_primary}
                class="h-7 text-xs font-mono"
              />
            </div>
          {/each}
        </div>
        <div class="border-t px-3 py-2 flex justify-end gap-2 shrink-0">
          <button type="button" onclick={closeSidebar}
            class="px-3 py-1.5 text-xs rounded border border-border hover:bg-muted transition-colors">
            Cancel
          </button>
          <button type="button"
            onclick={sidebarMode === 'add' ? commitSidebarAdd : commitSidebarEdit}
            class="px-3 py-1.5 text-xs rounded bg-primary text-primary-foreground hover:bg-primary/90 transition-colors">
            {sidebarMode === 'add' ? 'Insert' : 'Save'}
          </button>
        </div>
      </div>
    {/if}
  </div>
</div>

<!-- SQL confirm modal -->
{#if confirmModal}
  <SqlConfirmModal
    title={confirmModal.title}
    sql={confirmModal.sql}
    summary={confirmModal.summary}
    destructive={confirmModal.destructive}
    onConfirm={confirmModal.onConfirm}
    onCancel={() => confirmModal = null}
  />
{/if}
