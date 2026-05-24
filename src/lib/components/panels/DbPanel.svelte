<script>
  // @ts-nocheck
  import { onMount } from 'svelte';
  import { workspace } from '$lib/stores/workspace.svelte.js';
  import { listEnvFiles, readEnvFile } from '$lib/commands/env.js';
  import {
    dbListConnections, dbConnect, dbDisconnect, dbGetTreeStructure,
    dbListDatabases, dbListSchemas, dbListTables, dbListColumns,
    dbListIndexes, dbListViews, dbListFunctions, dbSaveConnection, dbDeleteConnection,
    dbTestConnection, dbListQueries,
    dbCreateQueryCollection, dbRenameQuery, dbDuplicateQuery,
    dbDeleteQuery, dbDeleteQueryCollection, dbDuplicateQueryCollection, dbRenameQueryCollection,
  } from '$lib/commands/db.js';
  import { Input } from '$lib/components/ui/input/index.js';
  import { Textarea } from '$lib/components/ui/textarea/index.js';
  import { Checkbox } from '$lib/components/ui/checkbox/index.js';
  import * as Dialog from '$lib/components/ui/dialog/index.js';
  import * as AlertDialog from '$lib/components/ui/alert-dialog/index.js';
  import * as DropdownMenu from '$lib/components/ui/dropdown-menu/index.js';
  import {
    Database, Plus, RefreshCw, ChevronRight, ChevronDown, Table, Layers,
    Eye, Zap, AlertCircle, Circle, CheckCircle2, Loader2, Settings, Trash2,
    MoreHorizontal, TestTube, ChevronDown as ChevDown, FileText, Folder,
  } from '@lucide/svelte';

  const folderPath = $derived(workspace.folderPath);

  let connections = $state([]);
  let activeConn = $state(null);
  let loadingConn = $state(null);
  let connError = $state('');
  let loadingList = $state(true);

  let expanded = $state(new Set());
  let cache = $state({});
  let loadingNodes = $state(new Set());

  // Connection form dialog
  let showConnForm = $state(false);
  let editingConn = $state(null);
  let formData = $state(defaultForm());
  let formError = $state('');
  let formTesting = $state(false);
  let formTestResult = $state(null);

  // Delete confirm dialog
  let deleteTarget = $state(null); // connection name to delete

  // Connection context menu
  let menuConn = $state(null);
  let menuPos = $state({ x: 0, y: 0 });

  // Query tree context menu + inline rename
  let qMenu = $state(null); // { type: 'group'|'collection'|'query', connName, item?, x, y }
  let renamingQuery      = $state(null); // { connName, file_name, collection, value }
  let renamingCollection = $state(null); // { connName, path, value }

  // Query dialogs
  let addCollectionDialog     = $state({ open: false, connName: null, value: '', parentPath: null });
  let deleteCollectionConfirm = $state({ open: false, connName: null, item: null });

  function defaultForm() {
    return { name: '', conn_type: 'postgresql', host: 'localhost', port: '5432', database: '', username: '', password: '', ssl: false, path: '', connection_string: '', notes: '' };
  }

  const CONN_TYPES = [
    { value: 'postgresql', label: 'PostgreSQL' },
    { value: 'mysql', label: 'MySQL / MariaDB' },
    { value: 'sqlite', label: 'SQLite' },
    { value: 'mongodb', label: 'MongoDB' },
  ];

  const CONN_TYPE_DEFAULTS = {
    postgresql: { port: '5432', host: 'localhost' },
    mysql: { port: '3306', host: 'localhost' },
    mariadb: { port: '3306', host: 'localhost' },
    sqlite: { port: '', host: '' },
    mongodb: { port: '27017', host: 'localhost' },
  };

  async function loadConnections() {
    if (!folderPath) return;
    loadingList = true;
    try {
      connections = await dbListConnections(folderPath);
      if (connections.length === 1 && !activeConn) activeConn = connections[0].name;
    } catch (e) {
      console.error(e);
    } finally {
      loadingList = false;
    }
  }

  onMount(() => {
    loadConnections();
    const onQuerySaved = (e) => {
      invalidateQueriesCache(e.detail.conn);
    };
    window.addEventListener('db-query-saved', onQuerySaved);
    return () => window.removeEventListener('db-query-saved', onQuerySaved);
  });

  // ── Env vars ──────────────────────────────────────────────────────────────

  async function buildEnvVars() {
    const vars = {};
    try {
      const files = await listEnvFiles(folderPath);
      for (const f of [...files].reverse()) {
        try {
          const result = await readEnvFile(folderPath, f.relPath);
          for (const line of result.content.split('\n')) {
            const stripped = line.replace(/^export\s+/, '').trim();
            if (!stripped || stripped.startsWith('#')) continue;
            const eq = stripped.indexOf('=');
            if (eq === -1) continue;
            const key = stripped.slice(0, eq).trim();
            let val = stripped.slice(eq + 1).trim();
            if ((val.startsWith('"') && val.endsWith('"')) || (val.startsWith("'") && val.endsWith("'")))
              val = val.slice(1, -1);
            vars[key] = val;
          }
        } catch {}
      }
    } catch {}
    return vars;
  }

  // ── Connect / disconnect ──────────────────────────────────────────────────

  async function selectConnection(name) {
    activeConn = name;
    const conn = connections.find(c => c.name === name);
    if (!conn?.connected) {
      loadingConn = name;
      connError = '';
      try {
        const vars = await buildEnvVars();
        await dbConnect(folderPath, name, { vars });
        await loadConnections();
      } catch (e) {
        connError = e?.message ?? String(e);
      } finally {
        loadingConn = null;
      }
    }
  }

  async function disconnect(name) {
    try {
      await dbDisconnect(name);
      await loadConnections();
      cache = {};
      expanded = new Set();
    } catch (e) { console.error(e); }
  }

  // ── Tree ──────────────────────────────────────────────────────────────────

  function cacheKey(...parts) { return parts.join('/'); }

  async function loadChildren(type, ...parts) {
    const key = cacheKey(type, ...parts);
    if (cache[key] || loadingNodes.has(key)) return;
    const next = new Set(loadingNodes); next.add(key); loadingNodes = next;
    try {
      const [conn, db, schema, table] = parts;
      let data;
      if (type === 'databases')    data = await dbListDatabases(conn);
      if (type === 'schemas')      data = await dbListSchemas(conn, db);
      if (type === 'tables')       data = await dbListTables(conn, db, schema);
      if (type === 'columns')      data = await dbListColumns(conn, db, schema, table);
      if (type === 'indexes')      data = await dbListIndexes(conn, db, schema, table);
      if (type === 'views')        data = await dbListViews(conn, db, schema);
      if (type === 'functions')    data = await dbListFunctions(conn, db, schema);
      if (type === 'queries')      data = await dbListQueries(folderPath, conn);
      cache = { ...cache, [key]: data ?? [] };
    } catch (e) {
      cache = { ...cache, [key]: { error: e?.message ?? String(e) } };
    } finally {
      const next = new Set(loadingNodes); next.delete(key); loadingNodes = next;
    }
  }

  function toggle(nodeKey) {
    const next = new Set(expanded);
    if (next.has(nodeKey)) { next.delete(nodeKey); } else { next.add(nodeKey); }
    expanded = next;
  }

  // ── Tab opening ───────────────────────────────────────────────────────────

  function openDataTab(conn, db, schema, table) {
    workspace.openTab({
      id: `db-data:${conn}/${db}/${schema}/${table}`,
      type: 'db-data',
      title: table,
      data: { conn, db, schema, table, folderPath },
    });
  }

  function openQueryTab(conn, collection = null) {
    const id = `db-query:${conn}:${Date.now()}`;
    workspace.openTab({ id, type: 'db-query', title: `Query (${conn})`, data: { conn, folderPath, tabId: id, queryCollection: collection } });
  }

  function openSavedQueryTab(connName, q) {
    const id = `db-query:${connName}:${q.file_name}`;
    workspace.openTab({
      id,
      type: 'db-query',
      title: q.name,
      data: { conn: connName, folderPath, tabId: id, queryName: q.name, queryFileName: q.file_name, queryDescription: q.description, initialSql: q.sql, queryCollection: q.collection ?? null },
    });
  }

  function openDiagramTab(conn, db, schema) {
    workspace.openTab({
      id: `db-diagram:${conn}/${db}/${schema}`,
      type: 'db-diagram',
      title: `${schema} diagram`,
      data: { conn, db, schema, folderPath },
    });
  }

  // ── Connection form ───────────────────────────────────────────────────────

  function openNewForm() {
    editingConn = null;
    formData = defaultForm();
    formError = '';
    formTestResult = null;
    showConnForm = true;
  }

  function openEditForm(conn) {
    editingConn = conn.name;
    formData = {
      name: conn.name,
      conn_type: conn.conn_type,
      host: conn.host ?? '',
      port: '',
      database: conn.database ?? '',
      username: '',
      password: '',
      ssl: false,
      path: '',
      connection_string: '',
      notes: '',
    };
    formError = '';
    formTestResult = null;
    showConnForm = true;
  }

  function onTypeChange(type) {
    formData.conn_type = type;
    const defs = CONN_TYPE_DEFAULTS[type] ?? {};
    if (defs.port) formData.port = defs.port;
    if (defs.host !== undefined && !formData.host) formData.host = defs.host;
  }

  async function testForm() {
    formTesting = true; formTestResult = null; formError = '';
    try {
      const vars = await buildEnvVars();
      await dbSaveConnection(folderPath, sanitizeForm());
      const result = await dbTestConnection(folderPath, formData.name, { vars });
      formTestResult = result;
    } catch (e) {
      formError = e?.message ?? String(e);
    } finally {
      formTesting = false;
    }
  }

  function sanitizeForm() {
    return {
      name: formData.name.trim(),
      type: formData.conn_type,          // matches #[serde(rename = "type")] in Rust
      host: formData.host || null,
      port: formData.port || null,
      database: formData.database || null,
      username: formData.username || null,
      password: formData.password || null,
      ssl: formData.ssl,
      path: formData.path || null,
      connection_string: formData.connection_string || null,
      notes: formData.notes,
    };
  }

  async function saveForm() {
    if (!formData.name.trim()) { formError = 'Name is required'; return; }
    formError = '';
    try {
      await dbSaveConnection(folderPath, sanitizeForm());
      showConnForm = false;
      await loadConnections();
    } catch (e) {
      formError = e?.message ?? String(e);
    }
  }

  async function confirmDelete() {
    if (!deleteTarget) return;
    const name = deleteTarget;
    deleteTarget = null;
    try {
      await dbDeleteConnection(folderPath, name);
      if (activeConn === name) activeConn = null;
      cache = {};
      expanded = new Set();
      await loadConnections();
    } catch (e) { console.error(e); }
  }

  function ensureLoaded(type, ...parts) {
    const key = cacheKey(type, ...parts);
    if (cache[key] !== undefined || loadingNodes.has(key)) return;
    Promise.resolve().then(() => loadChildren(type, ...parts));
  }

  function openMenu(e, conn) {
    e.preventDefault(); e.stopPropagation();
    menuConn = conn;
    menuPos = { x: e.clientX, y: e.clientY };
  }
  function closeMenu() { menuConn = null; qMenu = null; }

  function invalidateQueriesCache(connName) {
    const key = cacheKey('queries', connName);
    const groupKey = cacheKey('g-queries', connName);
    const next = { ...cache };
    delete next[key];
    cache = next;
    if (expanded.has(groupKey)) loadChildren('queries', connName);
  }

  function openQMenu(e, type, connName, item = null) {
    e.preventDefault(); e.stopPropagation();
    menuConn = null;
    qMenu = { type, connName, item: item ? $state.snapshot(item) : null, x: e.clientX, y: e.clientY };
  }

  async function qMenuDo(action) {
    if (!qMenu) return;
    const { type, connName, item } = qMenu;
    qMenu = null;
    try {
      if (action === 'new-query') {
        const collection = type === 'collection' ? item.path : null;
        openQueryTab(connName, collection);
      } else if (action === 'add-collection') {
        const parentPath = type === 'collection' ? item.path : null;
        addCollectionDialog = { open: true, connName, value: '', parentPath };
      } else if (action === 'dup-query') {
        await dbDuplicateQuery(folderPath, connName, item.file_name, item.collection ?? null);
        invalidateQueriesCache(connName);
      } else if (action === 'rename-query') {
        renamingQuery = { connName, file_name: item.file_name, collection: item.collection ?? null, value: item.name };
      } else if (action === 'del-query') {
        await dbDeleteQuery(folderPath, connName, item.file_name, item.collection ?? null);
        invalidateQueriesCache(connName);
      } else if (action === 'dup-collection') {
        await dbDuplicateQueryCollection(folderPath, connName, item.path);
        invalidateQueriesCache(connName);
      } else if (action === 'rename-collection') {
        renamingCollection = { connName, path: item.path, value: item.name };
      } else if (action === 'del-collection') {
        deleteCollectionConfirm = { open: true, connName, item };
      }
    } catch (e) { console.error(e); }
  }

  async function commitRenameQuery() {
    if (!renamingQuery) return;
    const { connName, file_name, collection, value } = renamingQuery;
    renamingQuery = null;
    if (!value?.trim()) return;
    try {
      await dbRenameQuery(folderPath, connName, file_name, collection, value.trim());
      invalidateQueriesCache(connName);
    } catch (e) { console.error(e); }
  }

  async function commitRenameCollection() {
    if (!renamingCollection) return;
    const { connName, path, value } = renamingCollection;
    renamingCollection = null;
    if (!value?.trim()) return;
    try {
      await dbRenameQueryCollection(folderPath, connName, path, value.trim());
      invalidateQueriesCache(connName);
    } catch (e) { console.error(e); }
  }

  async function submitAddCollection() {
    if (!addCollectionDialog.value.trim()) return;
    const { connName, value, parentPath } = addCollectionDialog;
    addCollectionDialog = { open: false, connName: null, value: '', parentPath: null };
    try {
      await dbCreateQueryCollection(folderPath, connName, value.trim(), parentPath);
      invalidateQueriesCache(connName);
    } catch (e) { console.error(e); }
  }

  async function confirmDeleteCollection() {
    const { connName, item } = deleteCollectionConfirm;
    deleteCollectionConfirm = { open: false, connName: null, item: null };
    if (!item) return;
    try {
      await dbDeleteQueryCollection(folderPath, connName, item.dir_name);
      invalidateQueriesCache(connName);
    } catch (e) { console.error(e); }
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
<div class="h-full flex flex-col overflow-hidden" onclick={closeMenu}>

  <!-- Toolbar -->
  <div class="flex items-center gap-0.5 px-2 py-1.5 border-b shrink-0">
    <button type="button" onclick={openNewForm} title="New Connection"
      class="p-1 rounded hover:bg-muted transition-colors text-muted-foreground hover:text-foreground">
      <Plus size={14} />
    </button>
    <div class="flex-1"></div>
    <button type="button" onclick={loadConnections} title="Refresh"
      class="p-1 rounded hover:bg-muted transition-colors text-muted-foreground hover:text-foreground">
      <RefreshCw size={14} />
    </button>
  </div>

  <!-- Connection list + tree -->
  <div class="flex-1 overflow-y-auto py-1 text-xs">
    {#if loadingList && connections.length === 0}
      <div class="flex items-center justify-center py-8 gap-2 text-muted-foreground">
        <Loader2 size={14} class="animate-spin" /><span>Loading…</span>
      </div>
    {:else if connections.length === 0}
      <div class="flex flex-col items-center justify-center py-8 gap-2 text-muted-foreground px-4">
        <Database size={24} class="opacity-20" />
        <p class="text-center opacity-60 text-xs">No connections.<br />Click + to add one.</p>
      </div>
    {:else}
      {#each connections as conn (conn.name)}
        {@const isActive = activeConn === conn.name}
        {@const isExpanded = expanded.has(conn.name)}
        {@const isConnected = conn.connected}
        {@const isLoading = loadingConn === conn.name}

        <div
          role="none"
          class="group flex items-center gap-1.5 px-2 py-1.5 cursor-pointer select-none hover:bg-muted/40 transition-colors {isActive ? 'bg-muted/60' : ''}"
          onclick={() => { toggle(conn.name); selectConnection(conn.name); }}
          oncontextmenu={(e) => openMenu(e, conn)}
        >
          <span class="shrink-0 text-muted-foreground">
            {#if isLoading}
              <Loader2 size={10} class="animate-spin" />
            {:else if isExpanded}
              <ChevronDown size={10} />
            {:else}
              <ChevronRight size={10} />
            {/if}
          </span>
          <Database size={12} class="shrink-0 {isConnected ? 'text-green-500' : 'text-muted-foreground'}" />
          <span class="truncate flex-1 font-medium">{conn.name}</span>
          <span class="text-[10px] text-muted-foreground opacity-0 group-hover:opacity-100">{conn.conn_type}</span>
          <span class="shrink-0 w-1.5 h-1.5 rounded-full {isConnected ? 'bg-green-500' : 'bg-muted-foreground/30'}"></span>
          <button type="button"
            onclick={(e) => { e.stopPropagation(); openMenu(e, conn); }}
            class="opacity-0 group-hover:opacity-100 p-0.5 rounded hover:bg-muted transition-all shrink-0">
            <MoreHorizontal size={11} />
          </button>
        </div>

        {#if isActive && connError}
          <div class="px-4 py-2 text-xs text-destructive bg-destructive/10 mx-2 mb-1 rounded">{connError}</div>
        {/if}

        {#if isExpanded && isConnected}
          {@render dbTree(conn.name, conn.conn_type)}
        {/if}
      {/each}
    {/if}
  </div>
</div>

{#snippet queriesGroup(connName)}
  {@const key = cacheKey('queries', connName)}
  {@const groupKey = cacheKey('g-queries', connName)}
  {@const isExpanded = expanded.has(groupKey)}
  {@const tree = cache[key]}
  {@const totalCount = tree ? tree.root.length + tree.collections.reduce((s, c) => s + c.queries.length, 0) : null}
  <div role="none"
    class="flex items-center gap-1.5 py-1 cursor-pointer hover:bg-muted/40 transition-colors select-none"
    style:padding-left="12px"
    onclick={(e) => { e.stopPropagation(); toggle(groupKey); ensureLoaded('queries', connName); }}
    oncontextmenu={(e) => openQMenu(e, 'group', connName)}
  >
    <span class="text-muted-foreground shrink-0 text-[10px]">{isExpanded ? '▾' : '▸'}</span>
    <FileText size={11} class="text-orange-400 shrink-0" />
    <span class="text-xs font-medium">Queries</span>
    {#if totalCount !== null}<span class="text-[11px] text-muted-foreground/60 ml-1">({totalCount})</span>{/if}
  </div>
  {#if isExpanded}
    {#if !tree || loadingNodes.has(key)}
      <div class="flex items-center gap-1 py-0.5 text-muted-foreground" style:padding-left="24px">
        <Loader2 size={9} class="animate-spin" />
      </div>
    {:else if tree.root.length === 0 && tree.collections.length === 0}
      <div class="text-[11px] text-muted-foreground/50 py-1" style:padding-left="24px">No saved queries</div>
    {:else}
      {#each tree.root as q (q.file_name)}
        {@render queryItem(connName, q, 24)}
      {/each}
      {#each tree.collections as col (col.dir_name)}
        {@render collectionNode(connName, col, 24)}
      {/each}
    {/if}
  {/if}
{/snippet}

{#snippet queryItem(connName, q, indent)}
  {@const isRenaming = renamingQuery?.connName === connName && renamingQuery?.file_name === q.file_name}
  <div role="none"
    class="group flex items-center gap-1.5 py-1 cursor-pointer hover:bg-muted/40 transition-colors select-none"
    style:padding-left="{indent}px"
    onclick={() => !isRenaming && openSavedQueryTab(connName, q)}
    ondblclick={(e) => { e.stopPropagation(); renamingQuery = { connName, file_name: q.file_name, collection: q.collection ?? null, value: q.name }; }}
    oncontextmenu={(e) => openQMenu(e, 'query', connName, q)}
  >
    <span class="w-2 shrink-0"></span>
    <FileText size={11} class="text-orange-400 shrink-0" />
    {#if isRenaming}
      <!-- svelte-ignore a11y_autofocus -->
      <Textarea
        autofocus
        rows={1}
        class="flex-1 text-xs bg-background border border-primary rounded px-1 py-0 outline-none min-w-0 resize-none leading-tight min-h-0 h-6"
        bind:value={renamingQuery.value}
        onclick={(e) => e.stopPropagation()}
        onkeydown={(e) => { e.stopPropagation(); if (e.key === 'Enter') { e.preventDefault(); commitRenameQuery(); } if (e.key === 'Escape') renamingQuery = null; }}
        onblur={commitRenameQuery}
      />
    {:else}
      <span class="truncate text-xs flex-1">{q.name}</span>
    {/if}
  </div>
{/snippet}

{#snippet collectionNode(connName, col, indent)}
  {@const colKey = cacheKey('g-col', connName, col.path)}
  {@const colExpanded = expanded.has(colKey)}
  {@const isRenaming = renamingCollection?.connName === connName && renamingCollection?.path === col.path}
  {@const totalCount = col.queries.length + col.collections.reduce((s, c) => s + c.queries.length, 0)}
  <div role="none"
    class="group flex items-center gap-1.5 py-1 cursor-pointer hover:bg-muted/40 transition-colors select-none"
    style:padding-left="{indent}px"
    onclick={(e) => { e.stopPropagation(); if (!isRenaming) toggle(colKey); }}
    ondblclick={(e) => { e.stopPropagation(); renamingCollection = { connName, path: col.path, value: col.name }; }}
    oncontextmenu={(e) => openQMenu(e, 'collection', connName, col)}
  >
    <span class="text-muted-foreground shrink-0 text-[11px]">{colExpanded ? '▾' : '▸'}</span>
    <Folder size={11} class="text-yellow-400 shrink-0" />
    {#if isRenaming}
      <!-- svelte-ignore a11y_autofocus -->
      <Textarea
        autofocus
        rows={1}
        class="flex-1 text-xs bg-background border border-primary rounded px-1 py-0 outline-none min-w-0 resize-none leading-tight min-h-0 h-6"
        bind:value={renamingCollection.value}
        onclick={(e) => e.stopPropagation()}
        onkeydown={(e) => { e.stopPropagation(); if (e.key === 'Enter') { e.preventDefault(); commitRenameCollection(); } if (e.key === 'Escape') renamingCollection = null; }}
        onblur={commitRenameCollection}
      />
    {:else}
      <span class="truncate text-xs flex-1">{col.name}</span>
      <span class="text-[11px] text-muted-foreground/40 shrink-0 mr-1">({totalCount})</span>
    {/if}
  </div>
  {#if colExpanded && !isRenaming}
    {#each col.queries as q (q.file_name)}
      {@render queryItem(connName, q, indent + 12)}
    {/each}
    {#each col.collections as subCol (subCol.path)}
      {@render collectionNode(connName, subCol, indent + 12)}
    {/each}
  {/if}
{/snippet}

{#snippet dbTree(connName, connType)}
  {@render queriesGroup(connName)}
  {#if connType === 'sqlite'}
    {@render groupsTree(connName, 'main', 'main', 12, connType)}
  {:else}
    {@const dbKey = cacheKey('databases', connName)}
    {#if !cache[dbKey]}
      {ensureLoaded('databases', connName)}
      <div class="flex items-center gap-1.5 py-1 text-muted-foreground" style:padding-left="12px">
        <Loader2 size={10} class="animate-spin" /><span class="text-xs">Loading…</span>
      </div>
    {:else if cache[dbKey]?.error}
      <p class="text-xs text-destructive py-1" style:padding-left="12px">{cache[dbKey].error}</p>
    {:else}
      {#each cache[dbKey] as db (db)}
        {@const dbNodeKey = cacheKey('db', connName, db)}
        {@const dbExpanded = expanded.has(dbNodeKey)}
        <div role="none"
          class="flex items-center gap-1.5 py-1 cursor-pointer hover:bg-muted/40 transition-colors select-none"
          style:padding-left="12px"
          onclick={(e) => { e.stopPropagation(); toggle(dbNodeKey); }}
        >
          <span class="text-muted-foreground shrink-0 text-[10px]">{dbExpanded ? '▾' : '▸'}</span>
          <Database size={11} class="text-blue-400 shrink-0" />
          <span class="truncate text-[11px]">{db}</span>
        </div>
        {#if dbExpanded}
          {@render schemaTree(connName, db, connType, 24)}
        {/if}
      {/each}
    {/if}
  {/if}
{/snippet}

{#snippet schemaTree(connName, db, connType, indent)}
  {#if connType === 'mysql'}
    {@render groupsTree(connName, db, db, indent, connType)}
  {:else}
    {@const key = cacheKey('schemas', connName, db)}
    {#if !cache[key]}
      {ensureLoaded('schemas', connName, db)}
      <div class="flex items-center gap-1.5 py-1 text-muted-foreground" style:padding-left="{indent}px">
        <Loader2 size={10} class="animate-spin" /><span class="text-xs">Loading…</span>
      </div>
    {:else if cache[key]?.error}
      <p class="text-xs text-destructive py-1" style:padding-left="{indent}px">{cache[key].error}</p>
    {:else}
      {#each cache[key] as schema (schema)}
        {@const schKey = cacheKey('sch', connName, db, schema)}
        {@const schExpanded = expanded.has(schKey)}
        <div role="none"
          class="flex items-center gap-1.5 py-1 cursor-pointer hover:bg-muted/40 transition-colors select-none"
          style:padding-left="{indent}px"
          onclick={(e) => { e.stopPropagation(); toggle(schKey); }}
        >
          <span class="text-muted-foreground shrink-0 text-[10px]">{schExpanded ? '▾' : '▸'}</span>
          <Layers size={11} class="text-purple-400 shrink-0" />
          <span class="truncate text-[11px]">{schema}</span>
        </div>
        {#if schExpanded}
          {@render groupsTree(connName, db, schema, indent + 12, connType)}
        {/if}
      {/each}
    {/if}
  {/if}
{/snippet}

{#snippet groupsTree(connName, db, schema, indent, connType)}
  {@const tKey = cacheKey('tables', connName, db, schema)}
  {#if !cache[tKey]}
    {ensureLoaded('tables', connName, db, schema)}
    <div class="flex items-center gap-1.5 py-1 text-muted-foreground" style:padding-left="{indent}px">
      <Loader2 size={10} class="animate-spin" /><span class="text-xs">Loading…</span>
    </div>
  {:else if cache[tKey]?.error}
    <p class="text-xs text-destructive py-1" style:padding-left="{indent}px">{cache[tKey].error}</p>
  {:else}
    {@const allItems = cache[tKey]}
    {@const tables = allItems.filter(t => !t.table_type?.includes('VIEW'))}
    {@const views = allItems.filter(t => t.table_type?.includes('VIEW'))}

    <!-- Tables group -->
    {@const tablesGKey = cacheKey('g-tables', connName, db, schema)}
    {@const tablesGExp = expanded.has(tablesGKey)}
    <div role="none"
      class="flex items-center gap-1.5 py-1 cursor-pointer hover:bg-muted/40 transition-colors select-none"
      style:padding-left="{indent}px"
      onclick={(e) => { e.stopPropagation(); toggle(tablesGKey); }}
    >
      <span class="text-muted-foreground shrink-0 text-[10px]">{tablesGExp ? '▾' : '▸'}</span>
      <Table size={11} class="text-blue-400 shrink-0" />
      <span class="text-[11px] font-medium">Tables</span>
      <span class="text-[10px] text-muted-foreground/60 ml-1">({tables.length})</span>
    </div>
    {#if tablesGExp}
      {#each tables as t (t.name)}
        {@render tableNode(connName, db, schema, t, indent + 12)}
      {/each}
    {/if}

    <!-- Views group -->
    {#if views.length > 0}
      {@const viewsGKey = cacheKey('g-views', connName, db, schema)}
      {@const viewsGExp = expanded.has(viewsGKey)}
      <div role="none"
        class="flex items-center gap-1.5 py-1 cursor-pointer hover:bg-muted/40 transition-colors select-none"
        style:padding-left="{indent}px"
        onclick={(e) => { e.stopPropagation(); toggle(viewsGKey); }}
      >
        <span class="text-muted-foreground shrink-0 text-[10px]">{viewsGExp ? '▾' : '▸'}</span>
        <Eye size={11} class="text-cyan-400 shrink-0" />
        <span class="text-[11px] font-medium">Views</span>
        <span class="text-[10px] text-muted-foreground/60 ml-1">({views.length})</span>
      </div>
      {#if viewsGExp}
        {#each views as v (v.name)}
          <div role="none"
            class="flex items-center gap-1.5 py-0.5 cursor-pointer hover:bg-muted/40 transition-colors select-none"
            style:padding-left="{indent + 12}px"
            onclick={() => openDataTab(connName, db, schema, v.name)}
          >
            <span class="w-2 shrink-0"></span>
            <Eye size={10} class="text-cyan-400 shrink-0" />
            <span class="truncate text-[11px]">{v.name}</span>
          </div>
        {/each}
      {/if}
    {/if}

    <!-- Functions group (PostgreSQL only) -->
    {#if connType === 'postgresql'}
      {@render functionsGroup(connName, db, schema, indent)}
    {/if}
  {/if}
{/snippet}

{#snippet tableNode(connName, db, schema, t, indent)}
  {@const tNodeKey = cacheKey('tn', connName, db, schema, t.name)}
  {@const tExpanded = expanded.has(tNodeKey)}
  <div role="none"
    class="group flex items-center gap-1.5 py-0.5 cursor-pointer hover:bg-muted/40 transition-colors select-none"
    style:padding-left="{indent}px"
    onclick={(e) => { e.stopPropagation(); toggle(tNodeKey); }}
    ondblclick={() => openDataTab(connName, db, schema, t.name)}
  >
    <span class="text-muted-foreground shrink-0 text-[10px]">{tExpanded ? '▾' : '▸'}</span>
    <Table size={10} class="text-green-400 shrink-0" />
    <span class="truncate flex-1 text-[11px]">{t.name}</span>
    <button type="button"
      onclick={(e) => { e.stopPropagation(); openDataTab(connName, db, schema, t.name); }}
      class="opacity-0 group-hover:opacity-100 text-[9px] px-1 py-0.5 rounded bg-muted hover:bg-muted/80 shrink-0 mr-1">
      Open
    </button>
  </div>
  {#if tExpanded}
    <!-- Columns sub-group -->
    {@const colKey = cacheKey('columns', connName, db, schema, t.name)}
    {@const colGKey = cacheKey('g-cols', connName, db, schema, t.name)}
    {@const colGExp = expanded.has(colGKey)}
    <div role="none"
      class="flex items-center gap-1.5 py-0.5 cursor-pointer hover:bg-muted/30 transition-colors select-none text-muted-foreground"
      style:padding-left="{indent + 12}px"
      onclick={(e) => { e.stopPropagation(); toggle(colGKey); ensureLoaded('columns', connName, db, schema, t.name); }}
    >
      <span class="text-[10px] shrink-0">{colGExp ? '▾' : '▸'}</span>
      <span class="text-[11px]">Columns</span>
      {#if cache[colKey] && !cache[colKey]?.error}
        <span class="text-[10px] text-muted-foreground/60 ml-1">({cache[colKey].length})</span>
      {/if}
    </div>
    {#if colGExp}
      {#if !cache[colKey] || loadingNodes.has(colKey)}
        <div class="flex items-center gap-1 py-0.5 text-muted-foreground" style:padding-left="{indent + 24}px">
          <Loader2 size={9} class="animate-spin" />
        </div>
      {:else}
        {#each cache[colKey] as col (col.name)}
          <div class="flex items-center gap-1 py-0.5" style:padding-left="{indent + 24}px">
            <span class="text-muted-foreground/40 text-[10px] w-2 shrink-0">
              {#if col.is_primary}🔑{:else if col.is_unique}✦{:else}·{/if}
            </span>
            <span class="font-mono text-[10px] truncate {col.is_primary ? 'text-yellow-500' : col.nullable ? 'text-muted-foreground' : 'text-foreground'}">{col.name}</span>
            <span class="text-[9px] text-muted-foreground/60 truncate ml-auto pr-2">{col.col_type}</span>
          </div>
        {/each}
      {/if}
    {/if}

    <!-- Indexes sub-group -->
    {@const idxKey = cacheKey('indexes', connName, db, schema, t.name)}
    {@const idxGKey = cacheKey('g-idx', connName, db, schema, t.name)}
    {@const idxGExp = expanded.has(idxGKey)}
    <div role="none"
      class="flex items-center gap-1.5 py-0.5 cursor-pointer hover:bg-muted/30 transition-colors select-none text-muted-foreground"
      style:padding-left="{indent + 12}px"
      onclick={(e) => { e.stopPropagation(); toggle(idxGKey); ensureLoaded('indexes', connName, db, schema, t.name); }}
    >
      <span class="text-[10px] shrink-0">{idxGExp ? '▾' : '▸'}</span>
      <span class="text-[11px]">Indexes</span>
      {#if cache[idxKey] && !cache[idxKey]?.error}
        <span class="text-[10px] text-muted-foreground/60 ml-1">({cache[idxKey].length})</span>
      {/if}
    </div>
    {#if idxGExp}
      {#if !cache[idxKey] || loadingNodes.has(idxKey)}
        <div class="flex items-center gap-1 py-0.5 text-muted-foreground" style:padding-left="{indent + 24}px">
          <Loader2 size={9} class="animate-spin" />
        </div>
      {:else if !cache[idxKey]?.length}
        <div class="text-[10px] text-muted-foreground/50 py-0.5" style:padding-left="{indent + 24}px">None</div>
      {:else}
        {#each cache[idxKey] as idx (idx.name)}
          <div class="flex items-center gap-1.5 py-0.5" style:padding-left="{indent + 24}px">
            <span class="text-[10px] text-muted-foreground/40 shrink-0">#</span>
            <span class="font-mono text-[10px] truncate">{idx.name}</span>
            {#if idx.is_unique}<span class="text-[9px] text-purple-400 ml-auto pr-2">UNIQUE</span>{/if}
          </div>
        {/each}
      {/if}
    {/if}
  {/if}
{/snippet}

{#snippet functionsGroup(connName, db, schema, indent)}
  {@const key = cacheKey('functions', connName, db, schema)}
  {@const groupKey = cacheKey('fgroup', connName, db, schema)}
  {@const isExpanded = expanded.has(groupKey)}
  <div role="none"
    class="flex items-center gap-1.5 py-1 cursor-pointer hover:bg-muted/40 transition-colors select-none"
    style:padding-left="{indent}px"
    onclick={(e) => { e.stopPropagation(); toggle(groupKey); ensureLoaded('functions', connName, db, schema); }}
  >
    <span class="text-muted-foreground shrink-0 text-[10px]">{isExpanded ? '▾' : '▸'}</span>
    <Zap size={11} class="text-yellow-400 shrink-0" />
    <span class="text-[11px] font-medium">Functions</span>
    {#if cache[key]}<span class="text-[10px] text-muted-foreground/60 ml-1">({cache[key].length})</span>{/if}
  </div>
  {#if isExpanded && cache[key]}
    {#each cache[key] as fn_ (fn_.name)}
      <div class="flex items-center gap-1.5 py-0.5" style:padding-left="{indent + 12}px">
        <Zap size={10} class="text-yellow-400 shrink-0" />
        <span class="truncate font-mono text-[10px]">{fn_.name}</span>
        <span class="text-[9px] text-muted-foreground/60 ml-auto pr-2">{fn_.return_type}</span>
      </div>
    {/each}
  {/if}
{/snippet}

<!-- Context menu -->
{#if menuConn}
  <div
    class="fixed z-50 min-w-36 bg-popover border border-border rounded-md shadow-lg py-1 text-xs"
    style:left="{menuPos.x}px"
    style:top="{menuPos.y}px"
    role="menu"
  >
    <button type="button"
      onclick={() => { openQueryTab(menuConn.name); closeMenu(); }}
      class="w-full flex items-center gap-2 px-3 py-1.5 hover:bg-muted transition-colors text-left">
      <Zap size={12} />New Query
    </button>
    <button type="button"
      onclick={() => { openEditForm(menuConn); closeMenu(); }}
      class="w-full flex items-center gap-2 px-3 py-1.5 hover:bg-muted transition-colors text-left">
      <Settings size={12} />Edit
    </button>
    {#if menuConn.connected}
      <button type="button"
        onclick={() => { disconnect(menuConn.name); closeMenu(); }}
        class="w-full flex items-center gap-2 px-3 py-1.5 hover:bg-muted transition-colors text-left">
        <Circle size={12} />Disconnect
      </button>
    {/if}
    <div class="border-t border-border my-1"></div>
    <button type="button"
      onclick={() => { deleteTarget = menuConn.name; closeMenu(); }}
      class="w-full flex items-center gap-2 px-3 py-1.5 hover:bg-muted transition-colors text-left text-destructive">
      <Trash2 size={12} />Delete
    </button>
  </div>
{/if}

<!-- Query tree context menu -->
{#if qMenu}
  <div
    class="fixed z-50 min-w-36 bg-popover border border-border rounded-md shadow-lg py-1 text-xs"
    style:left="{qMenu.x}px"
    style:top="{qMenu.y}px"
    role="menu"
  >
    {#if qMenu.type === 'group'}
      <button type="button" onclick={() => qMenuDo('new-query')}
        class="w-full flex items-center gap-2 px-3 py-1.5 hover:bg-muted transition-colors text-left">
        <FileText size={12} />New Query
      </button>
      <button type="button" onclick={() => qMenuDo('add-collection')}
        class="w-full flex items-center gap-2 px-3 py-1.5 hover:bg-muted transition-colors text-left">
        <Folder size={12} />Add Collection
      </button>
    {:else if qMenu.type === 'collection'}
      <button type="button" onclick={() => qMenuDo('new-query')}
        class="w-full flex items-center gap-2 px-3 py-1.5 hover:bg-muted transition-colors text-left">
        <FileText size={12} />New Query
      </button>
      <button type="button" onclick={() => qMenuDo('add-collection')}
        class="w-full flex items-center gap-2 px-3 py-1.5 hover:bg-muted transition-colors text-left">
        <Folder size={12} />Add Collection
      </button>
      <button type="button" onclick={() => qMenuDo('dup-collection')}
        class="w-full flex items-center gap-2 px-3 py-1.5 hover:bg-muted transition-colors text-left">
        <Folder size={12} />Duplicate
      </button>
      <button type="button" onclick={() => qMenuDo('rename-collection')}
        class="w-full flex items-center gap-2 px-3 py-1.5 hover:bg-muted transition-colors text-left">
        <Settings size={12} />Rename
      </button>
      <div class="border-t border-border my-1"></div>
      <button type="button" onclick={() => qMenuDo('del-collection')}
        class="w-full flex items-center gap-2 px-3 py-1.5 hover:bg-muted transition-colors text-left text-destructive">
        <Trash2 size={12} />Delete
      </button>
    {:else if qMenu.type === 'query'}
      <button type="button" onclick={() => qMenuDo('dup-query')}
        class="w-full flex items-center gap-2 px-3 py-1.5 hover:bg-muted transition-colors text-left">
        <FileText size={12} />Duplicate
      </button>
      <button type="button" onclick={() => qMenuDo('rename-query')}
        class="w-full flex items-center gap-2 px-3 py-1.5 hover:bg-muted transition-colors text-left">
        <Settings size={12} />Rename
      </button>
      <div class="border-t border-border my-1"></div>
      <button type="button" onclick={() => qMenuDo('del-query')}
        class="w-full flex items-center gap-2 px-3 py-1.5 hover:bg-muted transition-colors text-left text-destructive">
        <Trash2 size={12} />Delete
      </button>
    {/if}
  </div>
{/if}

<!-- Add Collection dialog -->
<Dialog.Root bind:open={addCollectionDialog.open}>
  <Dialog.Content class="max-w-sm">
    <Dialog.Header>
      <Dialog.Title>New Collection</Dialog.Title>
    </Dialog.Header>
    <div class="py-2">
      <Input bind:value={addCollectionDialog.value} placeholder="Collection name" class="h-8 text-xs"
        onkeydown={(e) => { if (e.key === 'Enter') submitAddCollection(); }} />
    </div>
    <Dialog.Footer>
      <button type="button" onclick={() => (addCollectionDialog = { open: false, connName: null, value: '', parentPath: null })}
        class="px-3 py-1.5 text-xs rounded border border-border hover:bg-muted transition-colors">Cancel</button>
      <button type="button" onclick={submitAddCollection} disabled={!addCollectionDialog.value}
        class="px-3 py-1.5 text-xs rounded bg-primary text-primary-foreground hover:bg-primary/90 disabled:opacity-50 transition-colors">Create</button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>

<!-- Delete collection confirm -->
<AlertDialog.Root open={!!deleteCollectionConfirm.item} onOpenChange={(o) => { if (!o) deleteCollectionConfirm = { open: false, connName: null, item: null }; }}>
  <AlertDialog.Content>
    <AlertDialog.Header>
      <AlertDialog.Title>Delete collection?</AlertDialog.Title>
      <AlertDialog.Description>
        "{deleteCollectionConfirm.item?.name}" and all its queries will be permanently removed. This cannot be undone.
      </AlertDialog.Description>
    </AlertDialog.Header>
    <AlertDialog.Footer>
      <AlertDialog.Cancel>Cancel</AlertDialog.Cancel>
      <AlertDialog.Action onclick={confirmDeleteCollection} class="bg-destructive text-destructive-foreground hover:bg-destructive/90">
        Delete
      </AlertDialog.Action>
    </AlertDialog.Footer>
  </AlertDialog.Content>
</AlertDialog.Root>

<!-- Delete confirm -->
<AlertDialog.Root open={!!deleteTarget} onOpenChange={(o) => { if (!o) deleteTarget = null; }}>
  <AlertDialog.Content>
    <AlertDialog.Header>
      <AlertDialog.Title>Delete connection?</AlertDialog.Title>
      <AlertDialog.Description>
        "{deleteTarget}" will be permanently removed. This cannot be undone.
      </AlertDialog.Description>
    </AlertDialog.Header>
    <AlertDialog.Footer>
      <AlertDialog.Cancel>Cancel</AlertDialog.Cancel>
      <AlertDialog.Action onclick={confirmDelete} class="bg-destructive text-destructive-foreground hover:bg-destructive/90">
        Delete
      </AlertDialog.Action>
    </AlertDialog.Footer>
  </AlertDialog.Content>
</AlertDialog.Root>

<!-- Connection form dialog -->
<Dialog.Root bind:open={showConnForm}>
  <Dialog.Content class="max-w-md max-h-[90vh] overflow-y-auto">
    <Dialog.Header>
      <Dialog.Title>{editingConn ? 'Edit Connection' : 'New Connection'}</Dialog.Title>
    </Dialog.Header>

    <div class="flex flex-col gap-3 py-2">
      {#if formError}
        <div class="text-xs text-destructive bg-destructive/10 px-3 py-2 rounded">{formError}</div>
      {/if}
      {#if formTestResult}
        <div class="text-xs text-green-600 bg-green-500/10 px-3 py-2 rounded">
          Connected! {formTestResult.server_version} — {formTestResult.latency_ms}ms
        </div>
      {/if}

      {@render formField('Name', formData.name, (v) => (formData.name = v))}

      <!-- Type dropdown -->
      <div class="flex flex-col gap-1.5">
        <label class="text-xs text-muted-foreground">Type</label>
        <DropdownMenu.Root>
          <DropdownMenu.Trigger
            class="flex items-center justify-between gap-1.5 px-3 py-2 rounded border border-input bg-muted/40 hover:bg-muted text-xs transition-colors w-full">
            {CONN_TYPES.find(t => t.value === formData.conn_type)?.label ?? formData.conn_type}
            <ChevronDown size={12} class="text-muted-foreground" />
          </DropdownMenu.Trigger>
          <DropdownMenu.Content class="w-48">
            {#each CONN_TYPES as t}
              <DropdownMenu.Item
                class="text-xs {formData.conn_type === t.value ? 'bg-muted/60' : ''}"
                onclick={() => onTypeChange(t.value)}>
                {t.label}
              </DropdownMenu.Item>
            {/each}
          </DropdownMenu.Content>
        </DropdownMenu.Root>
      </div>

      {#if formData.conn_type === 'sqlite'}
        {@render formField('File path', formData.path, (v) => (formData.path = v), '{{env.SQLITE_PATH}} or /path/to/db.sqlite')}
      {:else if formData.conn_type === 'mongodb'}
        {@render formField('Connection string', formData.connection_string, (v) => (formData.connection_string = v), 'mongodb+srv://user:pass@host/db')}
      {:else}
        <div class="grid grid-cols-3 gap-2">
          <div class="col-span-2">{@render formField('Host', formData.host, (v) => (formData.host = v))}</div>
          <div>{@render formField('Port', formData.port, (v) => (formData.port = v))}</div>
        </div>
        {@render formField('Database', formData.database, (v) => (formData.database = v))}
        {@render formField('Username', formData.username, (v) => (formData.username = v))}
        {@render formField('Password', formData.password, (v) => (formData.password = v), '', true)}
        <label class="flex items-center gap-2 text-xs cursor-pointer">
          <Checkbox checked={formData.ssl} onCheckedChange={(v) => (formData.ssl = v)} />
          <span>SSL / TLS</span>
        </label>
      {/if}

      <p class="text-[11px] text-muted-foreground">
        Use <code class="bg-muted px-1 rounded">{'{{env.VAR_NAME}}'}</code> to reference environment variables.
      </p>
    </div>

    <Dialog.Footer class="flex items-center justify-between">
      <button type="button" onclick={testForm} disabled={formTesting || !formData.name}
        class="flex items-center gap-1.5 px-3 py-1.5 text-xs rounded border border-border hover:bg-muted transition-colors disabled:opacity-50">
        <TestTube size={12} />{formTesting ? 'Testing…' : 'Test'}
      </button>
      <div class="flex gap-2">
        <button type="button" onclick={() => (showConnForm = false)}
          class="px-3 py-1.5 text-xs rounded border border-border hover:bg-muted transition-colors">Cancel</button>
        <button type="button" onclick={saveForm} disabled={!formData.name}
          class="px-3 py-1.5 text-xs rounded bg-primary text-primary-foreground hover:bg-primary/90 transition-colors disabled:opacity-50">Save</button>
      </div>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>

{#snippet formField(label, value, set, ph = '', secret = false)}
  <div class="flex flex-col gap-1.5">
    <label class="text-xs text-muted-foreground">{label}</label>
    <Input
      type={secret ? 'password' : 'text'}
      placeholder={ph}
      {value}
      oninput={(e) => set(e.currentTarget.value)}
      class="h-8 text-xs"
    />
  </div>
{/snippet}
