<script>
  // @ts-nocheck
  import { onMount } from 'svelte';
  import { dbListTables, dbListColumns, dbGetRelationships } from '$lib/commands/db.js';
  import { Loader2, ZoomIn, ZoomOut, Maximize2 } from '@lucide/svelte';

  let { data } = $props();
  const { conn, db, schema, folderPath } = data;

  let loading = $state(true);
  let error = $state('');
  let tables = $state([]); // [{ name, columns, x, y }]
  let relationships = $state([]); // [{ from, fromCol, to, toCol }]

  // Pan & zoom
  let scale = $state(1);
  let panX = $state(20);
  let panY = $state(20);
  let dragging = $state(false);
  let dragStartX = 0, dragStartY = 0, panStartX = 0, panStartY = 0;

  // Dragging a table node
  let dragNode = $state(null); // { name, ox, oy }
  let containerEl = $state(null);

  const CARD_W = 200;
  const CARD_H_BASE = 32; // header
  const ROW_H = 18;
  const COLS_PER_ROW = 4;
  const H_GAP = 40;
  const V_GAP = 40;

  onMount(async () => {
    try {
      const [tableList, rels] = await Promise.all([
        dbListTables(conn, db, schema),
        dbGetRelationships(conn, db, schema).catch(() => []),
      ]);
      // Load columns for all tables in parallel
      const colResults = await Promise.all(
        tableList.map(t => dbListColumns(conn, db, schema, t.name).catch(() => []))
      );
      // Layout: simple grid
      tables = tableList.map((t, i) => ({
        name: t.name,
        table_type: t.table_type,
        columns: colResults[i] ?? [],
        x: (i % COLS_PER_ROW) * (CARD_W + H_GAP) + 20,
        y: Math.floor(i / COLS_PER_ROW) * (CARD_H_BASE + 200 + V_GAP) + 20,
      }));
      relationships = rels.map(r => ({
        from: r.from_table, fromCol: r.from_column,
        to: r.to_table, toCol: r.to_column,
      }));
    } catch (e) {
      error = e?.message ?? String(e);
    } finally {
      loading = false;
    }
  });

  function tableHeight(t) {
    return CARD_H_BASE + t.columns.length * ROW_H + 8;
  }

  // Get center-right anchor for a column in a table
  function getAnchor(tableName, colName, side = 'right') {
    const t = tables.find(t => t.name === tableName);
    if (!t) return { x: 0, y: 0 };
    const colIndex = t.columns.findIndex(c => c.name === colName);
    const y = t.y + CARD_H_BASE + (colIndex >= 0 ? colIndex * ROW_H + ROW_H / 2 : ROW_H / 2);
    return { x: side === 'right' ? t.x + CARD_W : t.x, y };
  }

  // ── Pan / zoom ──────────────────────────────────────────────────────────

  function onWheel(e) {
    e.preventDefault();
    const factor = e.deltaY < 0 ? 1.1 : 0.9;
    scale = Math.min(3, Math.max(0.2, scale * factor));
  }

  function onMouseDown(e) {
    if (e.target.closest('.node-handle')) return; // handled by node drag
    dragging = true;
    dragStartX = e.clientX; dragStartY = e.clientY;
    panStartX = panX; panStartY = panY;
  }

  function onMouseMove(e) {
    if (dragging) {
      panX = panStartX + (e.clientX - dragStartX);
      panY = panStartY + (e.clientY - dragStartY);
    }
    if (dragNode) {
      const tbl = tables.find(t => t.name === dragNode.name);
      if (tbl) {
        tbl.x = dragNode.ox + (e.clientX - dragNode.startX) / scale;
        tbl.y = dragNode.oy + (e.clientY - dragNode.startY) / scale;
        tables = [...tables];
      }
    }
  }

  function onMouseUp() { dragging = false; dragNode = null; }

  function startNodeDrag(e, name) {
    e.stopPropagation();
    const tbl = tables.find(t => t.name === name);
    if (!tbl) return;
    dragNode = { name, ox: tbl.x, oy: tbl.y, startX: e.clientX, startY: e.clientY };
  }

  function fitView() {
    if (tables.length === 0) return;
    const maxX = Math.max(...tables.map(t => t.x + CARD_W)) + 40;
    const maxY = Math.max(...tables.map(t => t.y + tableHeight(t))) + 40;
    const w = containerEl ? containerEl.clientWidth : window.innerWidth;
    const h = containerEl ? containerEl.clientHeight : window.innerHeight;
    scale = Math.min(1, Math.min(w / maxX, h / maxY));
    panX = 20; panY = 20;
  }

  // Total SVG canvas size
  const SVG_W = $derived(
    tables.length ? Math.max(...tables.map(t => t.x + CARD_W + 60)) : 1200
  );
  const SVG_H = $derived(
    tables.length ? Math.max(...tables.map(t => t.y + tableHeight(t) + 60)) : 800
  );
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="h-full flex flex-col overflow-hidden bg-muted/10">
  <!-- Toolbar -->
  <div class="flex items-center gap-1 px-3 py-1.5 border-b bg-background shrink-0 text-xs">
    <span class="text-muted-foreground">{conn} › {db}{schema ? ` › ${schema}` : ''}</span>
    <div class="flex-1"></div>
    <button type="button" onclick={() => scale = Math.min(3, scale * 1.2)}
      class="p-1 rounded hover:bg-muted text-muted-foreground"><ZoomIn size={13} /></button>
    <button type="button" onclick={() => scale = Math.max(0.2, scale * 0.8)}
      class="p-1 rounded hover:bg-muted text-muted-foreground"><ZoomOut size={13} /></button>
    <button type="button" onclick={fitView}
      class="p-1 rounded hover:bg-muted text-muted-foreground"><Maximize2 size={13} /></button>
    <span class="text-muted-foreground">{Math.round(scale * 100)}%</span>
  </div>

  <!-- Canvas -->
  {#if loading}
    <div class="flex-1 flex items-center justify-center gap-2 text-muted-foreground">
      <Loader2 size={16} class="animate-spin" /><span class="text-sm">Loading schema…</span>
    </div>
  {:else if error}
    <div class="flex-1 flex items-center justify-center text-destructive text-sm">{error}</div>
  {:else if tables.length === 0}
    <div class="flex-1 flex items-center justify-center text-muted-foreground text-sm">No tables found in {schema || db}</div>
  {:else}
    <div class="flex-1 overflow-hidden"
      bind:this={containerEl}
      onwheel={onWheel}
      onmousedown={onMouseDown}
      onmousemove={onMouseMove}
      onmouseup={onMouseUp}
      onmouseleave={onMouseUp}
      role="none"
      style:cursor={dragging ? 'grabbing' : 'grab'}
    >
      <svg
        width="100%"
        height="100%"
        style="user-select: none"
      >
        <g transform="translate({panX},{panY}) scale({scale})">
          <!-- Relationship edges -->
          {#each relationships as rel (rel.from + rel.fromCol + rel.to + rel.toCol)}
            {@const src = getAnchor(rel.from, rel.fromCol, 'right')}
            {@const dst = getAnchor(rel.to, rel.toCol, 'left')}
            {@const mx = (src.x + dst.x) / 2}
            <path
              d="M{src.x},{src.y} C{mx},{src.y} {mx},{dst.y} {dst.x},{dst.y}"
              stroke="hsl(var(--primary))"
              stroke-width="1.5"
              fill="none"
              stroke-dasharray="4,3"
              opacity="0.6"
            />
            <!-- Arrow tip -->
            <polygon
              points="{dst.x},{dst.y} {dst.x - 6},{dst.y - 4} {dst.x - 6},{dst.y + 4}"
              fill="hsl(var(--primary))"
              opacity="0.6"
            />
          {/each}

          <!-- Table cards -->
          {#each tables as t (t.name)}
            {@const h = tableHeight(t)}
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <g
              transform="translate({t.x},{t.y})"
              class="node-handle"
              onmousedown={(e) => startNodeDrag(e, t.name)}
              style:cursor="move"
            >
              <!-- Card shadow -->
              <rect width={CARD_W} height={h} rx="6" ry="6"
                fill="hsl(var(--background))" stroke="hsl(var(--border))"
                stroke-width="1" filter="url(#shadow)" />

              <!-- Header -->
              <rect width={CARD_W} height={CARD_H_BASE} rx="6" ry="6"
                fill="hsl(var(--muted))" />
              <rect y={CARD_H_BASE - 6} width={CARD_W} height="6" fill="hsl(var(--muted))" />
              <text x="10" y={CARD_H_BASE / 2 + 4} font-size="11" font-weight="600"
                fill="hsl(var(--foreground))" font-family="monospace"
              >{t.name}</text>
              {#if t.table_type?.includes('VIEW')}
                <text x={CARD_W - 8} y={CARD_H_BASE / 2 + 4} font-size="9"
                  fill="hsl(var(--muted-foreground))" text-anchor="end">VIEW</text>
              {/if}

              <!-- Columns -->
              {#each t.columns as col, ci (col.name)}
                <g transform="translate(0,{CARD_H_BASE + ci * ROW_H})">
                  <!-- Alternating row bg -->
                  {#if ci % 2 === 0}
                    <rect width={CARD_W} height={ROW_H} fill="hsl(var(--muted)/0.2)" />
                  {/if}
                  <!-- PK/key marker -->
                  {#if col.is_primary}
                    <text x="6" y={ROW_H * 0.7} font-size="8" fill="#f59e0b">🔑</text>
                  {:else if col.is_unique}
                    <text x="6" y={ROW_H * 0.7} font-size="8" fill="#8b5cf6">✦</text>
                  {:else}
                    <text x="6" y={ROW_H * 0.7} font-size="8" fill="hsl(var(--muted-foreground)/0.4)">·</text>
                  {/if}
                  <!-- Column name -->
                  <text x="18" y={ROW_H * 0.7} font-size="10" font-family="monospace"
                    fill={col.is_primary ? '#f59e0b' : col.nullable ? 'hsl(var(--muted-foreground))' : 'hsl(var(--foreground))'}
                  >{col.name.length > 18 ? col.name.slice(0, 17) + '…' : col.name}</text>
                  <!-- Type -->
                  <text x={CARD_W - 4} y={ROW_H * 0.7} font-size="9" text-anchor="end"
                    fill="hsl(var(--muted-foreground)/0.6)"
                  >{col.col_type.slice(0, 12)}</text>
                </g>
              {/each}
            </g>
          {/each}
        </g>

        <defs>
          <filter id="shadow" x="-5%" y="-5%" width="110%" height="120%">
            <feDropShadow dx="0" dy="2" stdDeviation="3" flood-opacity="0.15" />
          </filter>
        </defs>
      </svg>
    </div>
  {/if}
</div>
