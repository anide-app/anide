<script>
  // @ts-nocheck
  let { data, tabId } = $props();

  import { onMount, untrack } from 'svelte';
  import { faker } from '@faker-js/faker';
  import { workspace } from '$lib/stores/workspace.svelte.js';
  import {
    readRequest, updateRequest, sendRequest as sendRequestCmd,
    createEmptyRequest,
  } from '$lib/commands/api.js';
  import { listEnvFiles, readEnvFile } from '$lib/commands/env.js';
  import { open as openFilePicker } from '@tauri-apps/plugin-dialog';
  import {
    Send, Save, Plus, Trash2, Loader2, ChevronDown, Globe, AlertTriangle,
    File as FileIcon, Download, Columns2, Rows2,
  } from '@lucide/svelte';
  import { toast } from 'svelte-sonner';
  import { Input } from '$lib/components/ui/input/index.js';
  import { Textarea } from '$lib/components/ui/textarea/index.js';
  import { PaneGroup, Pane, Handle as PaneHandle } from '$lib/components/ui/resizable/index.js';
  import * as DropdownMenu from '$lib/components/ui/dropdown-menu/index.js';
  import CodeMirrorEditor from '$lib/components/CodeMirrorEditor.svelte';

  const METHODS = ['GET', 'POST', 'PUT', 'PATCH', 'DELETE', 'HEAD', 'OPTIONS'];
  const METHOD_COLOR = {
    GET: 'text-green-500', POST: 'text-blue-400', PUT: 'text-yellow-500',
    PATCH: 'text-orange-400', DELETE: 'text-red-500', HEAD: 'text-purple-400',
    OPTIONS: 'text-gray-400',
  };
  const BODY_TYPES = ['none', 'json', 'form', 'raw', 'graphql'];
  const AUTH_TYPES = [
    { value: 'none', label: 'None' },
    { value: 'bearer', label: 'Bearer Token' },
    { value: 'basic', label: 'Basic Auth' },
    { value: 'apikey', label: 'API Key' },
    { value: 'oauth2', label: 'OAuth 2.0' },
  ];
  const ADDTO_TYPES = [
    { value: 'header', label: 'Header' },
    { value: 'query', label: 'Query' },
  ];

  // ── State ──────────────────────────────────────────────────────────────────
  let request = $state(createEmptyRequest('GET'));
  let loading = $state(true);
  let loadError = $state('');
  let sending = $state(false);
  let response = $state(null);
  let responseError = $state('');
  let activeSection = $state('params');
  let paramsSubTab = $state('query');
  let paneDirection = $state(localStorage.getItem('api-pane-direction') === 'horizontal' ? 'horizontal' : 'vertical');

  $effect(() => {
    localStorage.setItem('api-pane-direction', paneDirection);
  });
  let expandedResponses = $state(new Set());
  let cachedEnvVars = $state({});

  // Filename without extension — used as the # heading in the md file
  let requestName = $derived(data.relPath.split('/').pop().replace(/\.md$/i, ''));

  // ── Load ───────────────────────────────────────────────────────────────────
  onMount(async () => {
    try {
      const r = await readRequest(data.folderPath, data.relPath);
      if (!r.path_params) r.path_params = [];
      if (!r.form_params) r.form_params = [];
      request = r;
    } catch (e) {
      loadError = e?.message ?? String(e);
    } finally {
      loading = false;
    }
    cachedEnvVars = await buildEnvVars();
  });

  // ── Dirty tracking ─────────────────────────────────────────────────────────
  let _initialJson = $state('');
  $effect(() => {
    if (!loading && !loadError) {
      const json = untrack(() => JSON.stringify(request));
      _initialJson = json;
    }
  });

  let dirty = $derived(!loading && JSON.stringify(request) !== _initialJson);

  $effect(() => {
    const d = dirty;
    untrack(() => workspace.setTabDirty(tabId, d));
  });

  // ── Effective URL ──────────────────────────────────────────────────────────
  let effectiveUrl = $derived.by(() => {
    let url = applyEnvTokens(request.url || '', cachedEnvVars);
    for (const p of (request.path_params || [])) {
      if (p.enabled && p.key) url = url.replace(':' + p.key, encodeURIComponent(p.value || (':' + p.key)));
    }
    const qps = (request.params || []).filter(p => p.enabled && p.key);
    if (!qps.length) return url;
    try {
      const u = new URL(url);
      for (const p of qps) u.searchParams.append(p.key, p.value);
      return u.toString();
    } catch {
      const qs = qps.map(p => `${encodeURIComponent(p.key)}=${encodeURIComponent(p.value)}`).join('&');
      return url + (url.includes('?') ? '&' : '?') + qs;
    }
  });

  // ── Path params ────────────────────────────────────────────────────────────
  function extractPathParamNames(url) {
    if (!url) return [];
    return [...url.split('?')[0].matchAll(/:([a-zA-Z_][a-zA-Z0-9_]*)/g)].map(m => m[1]);
  }

  function syncPathParams(url, current) {
    const names = extractPathParamNames(url);
    const existing = new Map((current || []).map(p => [p.key, p]));
    return names.map(name => existing.get(name) ?? { key: name, value: '', enabled: true });
  }

  function onUrlInput(e) {
    const url = e.currentTarget.value;
    request = { ...request, url, path_params: syncPathParams(url, request.path_params) };
  }

  // ── Save ───────────────────────────────────────────────────────────────────
  async function save() {
    try {
      await updateRequest(data.folderPath, data.relPath, request);
      _initialJson = JSON.stringify(request);
      workspace.setTabDirty(tabId, false);
      toast.success('Saved');
    } catch (e) {
      toast.error(e?.message ?? 'Save failed');
    }
  }

  function onKeyDown(e) {
    if ((e.ctrlKey || e.metaKey) && e.key === 's') { e.preventDefault(); save(); }
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') { e.preventDefault(); send(); }
  }

  // ── Env vars ───────────────────────────────────────────────────────────────
  // Builds a namespaced map: ".env" → "env.KEY", ".env.local" → "env.local.KEY"
  async function buildEnvVars() {
    const envVars = {};
    try {
      const files = await listEnvFiles(data.folderPath);
      for (const f of files) {
        // ".env" → suffix="" → ns="env"
        // ".env.local" → suffix=".local" → ns="env.local"
        const suffix = f.name.startsWith('.env') ? f.name.slice(4) : null;
        if (suffix === null) continue;
        const ns = suffix ? `env${suffix}` : 'env';
        try {
          const content = await readEnvFile(data.folderPath, f.relPath);
          for (const line of content.content.split('\n')) {
            const stripped = line.replace(/^export\s+/, '').trim();
            if (!stripped || stripped.startsWith('#')) continue;
            const eq = stripped.indexOf('=');
            if (eq === -1) continue;
            const key = stripped.slice(0, eq).trim();
            let val = stripped.slice(eq + 1).trim();
            if ((val.startsWith('"') && val.endsWith('"')) || (val.startsWith("'") && val.endsWith("'")))
              val = val.slice(1, -1);
            envVars[`${ns}.${key}`] = val;
          }
        } catch { }
      }
    } catch { }
    return envVars;
  }

  // Replace {{env.*}} tokens in a string using the namespaced envVars map
  function applyEnvTokens(template, envVars) {
    return template.replace(/\{\{([^}]+)\}\}/g, (match, key) => {
      const k = key.trim();
      return (k.startsWith('env.') && envVars[k] !== undefined) ? envVars[k] : match;
    });
  }

  // Pre-resolve all {{env.*}} tokens in every string field of a request
  function preResolveEnvTokens(req, envVars) {
    const e = (s) => applyEnvTokens(s ?? '', envVars);
    return {
      ...req,
      url: e(req.url),
      headers: req.headers.map(h => ({ ...h, key: e(h.key), value: e(h.value) })),
      params: req.params.map(p => ({ ...p, key: e(p.key), value: e(p.value) })),
      path_params: (req.path_params || []).map(p => ({ ...p, value: e(p.value) })),
      form_params: (req.form_params || []).map(p => ({ ...p, value: e(p.value) })),
      request_body: e(req.request_body),
    };
  }

  // ── Faker ──────────────────────────────────────────────────────────────────
  function resolveFakerTokens(template) {
    return template.replace(/\{\{Faker\.([^}]+)\}\}/g, (_, path) => {
      try {
        const parts = path.split('.');
        let obj = faker;
        for (const part of parts.slice(0, -1)) { obj = obj[part]; if (!obj) return `{{Faker.${path}}}`; }
        const lastPart = parts.at(-1);
        const methodName = lastPart.replace(/\(.*\)$/, '');
        const argsMatch = lastPart.match(/\((.+)\)$/);
        const args = argsMatch ? [JSON.parse(argsMatch[1])] : [];
        const method = obj[methodName];
        if (typeof method !== 'function') return `{{Faker.${path}}}`;
        return String(method.call(obj, ...args));
      } catch { return `{{Faker.${path}}}`; }
    });
  }

  function preResolveFaker(req) {
    return {
      ...req,
      url: resolveFakerTokens(req.url),
      headers: req.headers.map(h => ({ ...h, value: resolveFakerTokens(h.value) })),
      params: req.params.map(p => ({ ...p, value: resolveFakerTokens(p.value) })),
      path_params: (req.path_params || []).map(p => ({ ...p, value: resolveFakerTokens(p.value) })),
      form_params: (req.form_params || []).map(p => ({ ...p, value: resolveFakerTokens(p.value) })),
      request_body: resolveFakerTokens(req.request_body ?? ''),
    };
  }

  // ── Send ───────────────────────────────────────────────────────────────────
  async function send() {
    sending = true; response = null; responseError = '';
    try {
      const envVars = await buildEnvVars();
      cachedEnvVars = envVars;
      const envResolved = preResolveEnvTokens(request, envVars);
      const fakerResolved = preResolveFaker(envResolved);
      const res = await sendRequestCmd({
        projectPath: data.folderPath,
        request: fakerResolved,
        envVars: {}, // env.* already resolved above; Rust handles plain {{KEY}} only
        followRedirects: true,
        timeoutMs: 30000,
      });
      response = res;
    } catch (e) {
      responseError = e?.message ?? String(e);
    } finally {
      sending = false;
    }
  }

  // ── KV helpers ─────────────────────────────────────────────────────────────
  function addRow(arr, extra = {}) { return [...arr, { key: '', value: '', enabled: true, ...extra }]; }
  function removeRow(arr, i) { return arr.filter((_, idx) => idx !== i); }
  function updateRow(arr, i, field, val) { return arr.map((r, idx) => idx === i ? { ...r, [field]: val } : r); }

  async function pickFormFile(i) {
    try {
      const selected = await openFilePicker({ multiple: false });
      if (selected) {
        request = { ...request, form_params: updateRow(request.form_params, i, 'value', selected) };
      }
    } catch { }
  }

  // ── Response helpers ───────────────────────────────────────────────────────
  let responseTab = $state('body');
  let responseView = $state('source'); // 'source' | 'preview' (html only)

  function statusClass(s) {
    if (!s) return 'text-muted-foreground';
    if (s < 300) return 'text-green-500';
    if (s < 400) return 'text-yellow-500';
    return 'text-red-500';
  }

  function formatSize(bytes) {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
  }

  function detectBodyLang(body, headers = []) {
    const ct = (headers || []).find(h => h.key?.toLowerCase() === 'content-type')?.value ?? '';
    if (ct.includes('json')) return 'json';
    if (ct.includes('/html')) return 'html';
    if (ct.includes('/xml') || ct.includes('+xml')) return 'xml';
    if (ct.startsWith('image/') || ct.startsWith('audio/') || ct.startsWith('video/')
        || ct === 'application/pdf' || ct === 'application/octet-stream') return 'binary';
    // sniff body
    const trimmed = (body || '').trimStart();
    if (trimmed[0] === '{' || trimmed[0] === '[') {
      try { JSON.parse(body); return 'json'; } catch { }
    }
    const lower = trimmed.toLowerCase();
    if (lower.startsWith('<!doctype html') || lower.startsWith('<html')) return 'html';
    if (trimmed.startsWith('<?xml') || (trimmed.startsWith('<') && trimmed.includes('</'))) return 'xml';
    return 'text';
  }

  function prettifyBody(body, lang) {
    if (lang === 'json') {
      try { return JSON.stringify(JSON.parse(body), null, 2); } catch { }
    }
    return body ?? '';
  }

  // Reset to source view whenever a new response arrives
  $effect(() => {
    if (response) untrack(() => { responseView = 'source'; });
  });

  let bodyEditorLang = $derived(request.body_type === 'json' ? 'json' : 'text');
  let responseLang = $derived(response ? detectBodyLang(response.body, response.headers) : 'text');
  let prettyResponseBody = $derived(prettifyBody(response?.body ?? '', responseLang));

  // ── Notes & saved responses ────────────────────────────────────────────────
  function parseSavedResponses(body) {
    const results = [];
    const regex = /<!-- saved-response:([^\s>]+) -->([\s\S]*?)<!-- \/saved-response -->/g;
    let match;
    while ((match = regex.exec(body ?? '')) !== null) {
      const [, ts, content] = match;
      const headingMatch = content.match(/###\s+(.+)/);
      const metaMatch = content.match(/\*([^·*]+)·([^*]+)\*/);
      const langMatch = content.match(/```(\w+)\n/);
      const bodyMatch = content.match(/```(?:\w*)\n([\s\S]*?)```/);
      results.push({
        timestamp: ts,
        statusText: headingMatch?.[1]?.trim() ?? '',
        time: metaMatch?.[1]?.trim() ?? '',
        size: metaMatch?.[2]?.trim() ?? '',
        lang: langMatch?.[1] ?? 'text',
        body: bodyMatch?.[1]?.trimEnd() ?? '',
      });
    }
    return results;
  }

  let savedResponses = $derived(parseSavedResponses(request.body));

  // Extract just the description text from the body (strips # heading and "description - " prefix)
  let descriptionPart = $derived.by(() => {
    const body = request.body ?? '';
    const respIdx = body.indexOf('<!-- saved-response:');
    const textPart = (respIdx === -1 ? body : body.slice(0, respIdx)).trimEnd();
    const lines = textPart.split('\n');
    let i = 0;
    if (lines[0]?.startsWith('# ')) i = 1;
    while (i < lines.length && lines[i].trim() === '') i++;
    const rest = lines.slice(i).join('\n').trimEnd();
    if (rest.startsWith('description - ')) return rest.slice('description - '.length).trimStart();
    return rest;
  });

  // Rebuild body as: "# name\n\ndescription - {text}\n\n{saved responses}"
  function setDescription(newDesc) {
    const body = request.body ?? '';
    const respIdx = body.indexOf('<!-- saved-response:');
    const tail = respIdx === -1 ? '' : '\n\n' + body.slice(respIdx);
    const descLine = newDesc.trim() ? `\n\ndescription - ${newDesc}` : '';
    request = { ...request, body: `# ${requestName}${descLine}${tail}` };
  }

  function deleteSavedResponse(ts) {
    const escaped = ts.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
    // Remove the block including any preceding blank lines
    const re = new RegExp(`\\n*<!-- saved-response:${escaped} -->[\\s\\S]*?<!-- \\/saved-response -->`, 'g');
    const newBody = (request.body ?? '').replace(re, '').trimEnd();
    request = { ...request, body: newBody };
  }

  async function saveResponse() {
    if (!response) return;
    const ts = new Date().toISOString();
    const lang = responseLang === 'binary' ? 'text' : responseLang;
    const pretty = prettifyBody(response.body, lang);

    const entry = [
      ``, ``,
      `<!-- saved-response:${ts} -->`,
      `### ${response.status} ${response.statusText} Response`,
      ``,
      `\`\`\`${lang}`,
      pretty,
      `\`\`\``,
      `<!-- /saved-response -->`,
    ].join('\n');

    // Always rebuild body with proper heading so docs viewer shows name + description
    const body = request.body ?? '';
    const respIdx = body.indexOf('<!-- saved-response:');
    const existingResponses = respIdx !== -1 ? '\n\n' + body.slice(respIdx).trimEnd() : '';
    const desc = descriptionPart.trim();
    const descLine = desc ? `\n\ndescription - ${desc}` : '';
    const newBody = `# ${requestName}${descLine}${existingResponses}${entry}`;

    request = { ...request, body: newBody };
    await save();
    activeSection = 'notes';
  }

  function toggleResponseExpand(ts) {
    const next = new Set(expandedResponses);
    if (next.has(ts)) next.delete(ts); else next.add(ts);
    expandedResponses = next;
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<!-- svelte-ignore a11y_click_events_have_key_events -->
<div
  class="h-full flex flex-col overflow-hidden"
  role="none"
  onkeydown={onKeyDown}
  tabindex="-1"
>
  {#if loading}
    <div class="flex flex-1 items-center justify-center gap-2 text-muted-foreground">
      <Loader2 size={16} class="animate-spin" /><span class="text-sm">Loading…</span>
    </div>
  {:else if loadError}
    <div class="flex flex-1 items-center justify-center gap-2 text-destructive">
      <AlertTriangle size={16} /><span class="text-sm">{loadError}</span>
    </div>
  {:else}

    <!-- ── URL bar ── -->
    <div class="flex flex-col border-b shrink-0 bg-background">
      <div class="flex items-center gap-2 px-3 py-2">
        <!-- Method dropdown -->
        <DropdownMenu.Root>
          <DropdownMenu.Trigger
            class="flex items-center gap-1 px-2 py-1.5 rounded border bg-muted/40 hover:bg-muted text-xs font-mono font-semibold transition-colors shrink-0 {METHOD_COLOR[request.method] ?? 'text-foreground'}"
          >
            {request.method}
            <ChevronDown size={10} />
          </DropdownMenu.Trigger>
          <DropdownMenu.Content class="min-w-28">
            {#each METHODS as m}
              <DropdownMenu.Item
                class="font-mono text-xs {METHOD_COLOR[m] ?? ''}"
                onclick={() => { request = { ...request, method: m }; }}
              >{m}</DropdownMenu.Item>
            {/each}
          </DropdownMenu.Content>
        </DropdownMenu.Root>

        <!-- URL input -->
        <Input
          type="text"
          placeholder="https://api.example.com/:id/endpoint"
          value={request.url}
          oninput={onUrlInput}
          class="flex-1 text-sm font-mono h-8"
        />

        <!-- Send -->
        <button
          type="button"
          onclick={send}
          disabled={sending || !request.url.trim()}
          class="flex items-center gap-1.5 px-3 py-1.5 rounded bg-primary text-primary-foreground text-xs font-medium hover:bg-primary/90 transition-colors disabled:opacity-50 disabled:cursor-not-allowed shrink-0"
        >
          {#if sending}<Loader2 size={13} class="animate-spin" />{:else}<Send size={13} />{/if}
          Send
        </button>

        <!-- Save -->
        <button
          type="button"
          onclick={save}
          title="Save (Ctrl+S)"
          class="p-1.5 rounded hover:bg-muted transition-colors {dirty ? 'text-primary' : 'text-muted-foreground'}"
        >
          <Save size={14} />
        </button>
      </div>

      <!-- Effective URL preview -->
      {#if effectiveUrl && effectiveUrl !== request.url}
        <div class="px-3 pb-1.5 text-[10px] text-muted-foreground font-mono truncate">
          <span class="opacity-50">→</span> {effectiveUrl}
        </div>
      {/if}
    </div>

    <!-- ── Split pane ── -->
    <PaneGroup direction={paneDirection} class="flex-1 min-h-0">

      <!-- Request pane -->
      <Pane defaultSize={45} minSize={15} class="flex flex-col overflow-hidden min-h-0">

        <!-- Section tabs -->
        <div class="flex items-center gap-0 border-b shrink-0 bg-muted/20 px-3">
          {#each ['params', 'headers', 'auth', 'body', 'notes'] as section}
            {@const count =
              section === 'params'
                ? (request.params.filter(p => p.enabled && p.key).length + (request.path_params || []).filter(p => p.enabled && p.key).length)
                : section === 'headers' ? request.headers.filter(h => h.enabled && h.key).length
                : section === 'notes' ? savedResponses.length
                : 0}
            <button
              type="button"
              onclick={() => (activeSection = section)}
              class="px-3 py-2 text-xs capitalize border-b-2 transition-colors
                {activeSection === section
                  ? 'border-primary text-foreground'
                  : 'border-transparent text-muted-foreground hover:text-foreground'}"
            >
              {section}
              {#if count > 0}
                <span class="ml-1 text-[10px] bg-primary/20 text-primary rounded px-1">{count}</span>
              {/if}
            </button>
          {/each}
          <div class="flex-1"></div>
          <button
            type="button"
            title={paneDirection === 'vertical' ? 'Switch to side-by-side' : 'Switch to stacked'}
            onclick={() => (paneDirection = paneDirection === 'vertical' ? 'horizontal' : 'vertical')}
            class="p-1 mb-0.5 rounded hover:bg-muted text-muted-foreground hover:text-foreground transition-colors shrink-0"
          >
            {#if paneDirection === 'vertical'}
              <Columns2 size={13} />
            {:else}
              <Rows2 size={13} />
            {/if}
          </button>
        </div>

        <!-- Section content: absolute children give definite height to body/editor -->
        <div class="flex-1 min-h-0 overflow-hidden relative">

          {#if activeSection === 'body'}
            <!-- Body needs absolute positioning so CodeMirror gets a fixed height -->
            <div class="absolute inset-0 flex flex-col">
              <!-- Body type picker -->
              <div class="flex items-center gap-1 px-3 py-1.5 border-b shrink-0">
                {#each BODY_TYPES as bt}
                  <button
                    type="button"
                    onclick={() => (request = { ...request, body_type: bt })}
                    class="px-2 py-0.5 rounded text-[11px] transition-colors capitalize
                      {request.body_type === bt
                        ? 'bg-primary/20 text-primary font-medium'
                        : 'text-muted-foreground hover:text-foreground hover:bg-muted'}"
                  >{bt}</button>
                {/each}
              </div>

              {#if request.body_type === 'none'}
                <div class="flex-1 flex items-center justify-center text-xs text-muted-foreground">No body</div>
              {:else if request.body_type === 'form'}
                {@render formParamsTable()}
              {:else}
                <CodeMirrorEditor
                  bind:value={request.request_body}
                  language={bodyEditorLang}
                  placeholder={
                    request.body_type === 'json' ? '{\n  "key": "value"\n}' :
                    request.body_type === 'graphql' ? '{ users { id email } }' :
                    'Request body…'
                  }
                  class="flex-1 min-h-0"
                />
              {/if}
            </div>

          {:else}
            <!-- Non-body sections scroll normally -->
            <div class="absolute inset-0 overflow-y-auto">

              {#if activeSection === 'params'}
                <!-- Sub-tabs -->
                <div class="flex items-center gap-2 px-3 py-1.5 border-b bg-muted/10 text-xs sticky top-0 bg-background/95 backdrop-blur-sm z-10">
                  {#each [['query', 'Query'], ['path', 'Path Params']] as [v, label]}
                    <button
                      type="button"
                      onclick={() => (paramsSubTab = v)}
                      class="px-2 py-0.5 rounded transition-colors
                        {paramsSubTab === v
                          ? 'bg-primary/20 text-primary font-medium'
                          : 'text-muted-foreground hover:text-foreground hover:bg-muted'}"
                    >{label}</button>
                  {/each}
                </div>

                {#if paramsSubTab === 'query'}
                  {@render kvTable(
                    request.params,
                    (rows) => (request = { ...request, params: rows }),
                    'Param key…', 'Value',
                  )}
                {:else}
                  {#if (request.path_params || []).length === 0}
                    <div class="flex items-center justify-center py-8 text-xs text-muted-foreground">
                      Add <code class="mx-1 px-1 bg-muted rounded">:paramName</code> to the URL to see path params here
                    </div>
                  {:else}
                    <table class="w-full text-xs table-fixed">
                      <colgroup>
                        <col style="width:28px" />
                        <col style="width:40%" />
                        <col />
                      </colgroup>
                      <thead>
                        <tr class="border-b text-muted-foreground">
                          <th class="px-2 py-1.5"></th>
                          <th class="text-left px-2 py-1.5 font-medium">Param</th>
                          <th class="text-left px-2 py-1.5 font-medium">Value</th>
                        </tr>
                      </thead>
                      <tbody>
                        {#each (request.path_params || []) as row, i}
                          <tr class="border-b border-border/30 hover:bg-muted/20">
                            <td class="px-2 py-1">
                              <input
                                type="checkbox"
                                checked={row.enabled}
                                onchange={(e) => (request = { ...request, path_params: updateRow(request.path_params, i, 'enabled', e.currentTarget.checked) })}
                                class="rounded border-border"
                              />
                            </td>
                            <td class="px-2 py-1">
                              <span class="font-mono text-muted-foreground">:{row.key}</span>
                            </td>
                            <td class="px-1 py-0.5">
                              <Input
                                type="text"
                                value={row.value}
                                placeholder="value"
                                oninput={(e) => (request = { ...request, path_params: updateRow(request.path_params, i, 'value', e.currentTarget.value) })}
                                class="w-full bg-transparent border-0 shadow-none h-7 px-1 py-0 text-xs font-mono focus-visible:ring-0 focus-visible:bg-muted/40"
                              />
                            </td>
                          </tr>
                        {/each}
                      </tbody>
                    </table>
                  {/if}
                {/if}

              {:else if activeSection === 'headers'}
                {@render kvTable(request.headers, (rows) => (request = { ...request, headers: rows }), 'Header name', 'Value')}

              {:else if activeSection === 'auth'}
                <div class="p-3 flex flex-col gap-3">
                  <div class="flex items-center gap-2">
                    <span class="text-xs text-muted-foreground w-16 shrink-0">Type</span>
                    <DropdownMenu.Root>
                      <DropdownMenu.Trigger class="flex items-center gap-1.5 px-2 py-1 rounded border bg-muted/40 hover:bg-muted text-xs transition-colors min-w-32 justify-between">
                        {AUTH_TYPES.find(t => t.value === request.auth.type)?.label ?? 'None'}
                        <ChevronDown size={10} />
                      </DropdownMenu.Trigger>
                      <DropdownMenu.Content class="min-w-36">
                        {#each AUTH_TYPES as t}
                          <DropdownMenu.Item
                            class="text-xs {request.auth.type === t.value ? 'bg-muted/60' : ''}"
                            onclick={() => {
                              const v = t.value;
                              if (v === 'none') request = { ...request, auth: { type: 'none' } };
                              else if (v === 'bearer') request = { ...request, auth: { type: 'bearer', token: '' } };
                              else if (v === 'basic') request = { ...request, auth: { type: 'basic', username: '', password: '' } };
                              else if (v === 'apikey') request = { ...request, auth: { type: 'apikey', key: '', value: '', addTo: 'header' } };
                              else if (v === 'oauth2') request = { ...request, auth: { type: 'oauth2', grant_type: 'client_credentials', token_url: '', client_id: '', client_secret: '', scope: '' } };
                            }}
                          >{t.label}</DropdownMenu.Item>
                        {/each}
                      </DropdownMenu.Content>
                    </DropdownMenu.Root>
                  </div>

                  {#if request.auth.type === 'bearer'}
                    {@render authField('Token', 'auth.token', request.auth.token, (v) => (request = { ...request, auth: { ...request.auth, token: v } }))}
                  {:else if request.auth.type === 'basic'}
                    {@render authField('Username', 'auth.username', request.auth.username, (v) => (request = { ...request, auth: { ...request.auth, username: v } }))}
                    {@render authField('Password', 'auth.password', request.auth.password, (v) => (request = { ...request, auth: { ...request.auth, password: v } }), true)}
                  {:else if request.auth.type === 'apikey'}
                    {@render authField('Key', 'auth.key', request.auth.key, (v) => (request = { ...request, auth: { ...request.auth, key: v } }))}
                    {@render authField('Value', 'auth.value', request.auth.value, (v) => (request = { ...request, auth: { ...request.auth, value: v } }))}
                    <div class="flex items-center gap-2">
                      <span class="text-xs text-muted-foreground w-24 shrink-0">Add to</span>
                      <DropdownMenu.Root>
                        <DropdownMenu.Trigger class="flex items-center gap-1.5 px-2 py-1 rounded border bg-muted/40 hover:bg-muted text-xs transition-colors min-w-24 justify-between">
                          {ADDTO_TYPES.find(t => t.value === request.auth.addTo)?.label ?? 'Header'}
                          <ChevronDown size={10} />
                        </DropdownMenu.Trigger>
                        <DropdownMenu.Content class="min-w-24">
                          {#each ADDTO_TYPES as t}
                            <DropdownMenu.Item
                              class="text-xs {request.auth.addTo === t.value ? 'bg-muted/60' : ''}"
                              onclick={() => (request = { ...request, auth: { ...request.auth, addTo: t.value } })}
                            >{t.label}</DropdownMenu.Item>
                          {/each}
                        </DropdownMenu.Content>
                      </DropdownMenu.Root>
                    </div>
                  {:else if request.auth.type === 'oauth2'}
                    {@render authField('Token URL', 'auth.token_url', request.auth.token_url, (v) => (request = { ...request, auth: { ...request.auth, token_url: v } }))}
                    {@render authField('Client ID', 'auth.client_id', request.auth.client_id, (v) => (request = { ...request, auth: { ...request.auth, client_id: v } }))}
                    {@render authField('Client Secret', 'auth.client_secret', request.auth.client_secret, (v) => (request = { ...request, auth: { ...request.auth, client_secret: v } }), true)}
                    {@render authField('Scope', 'auth.scope', request.auth.scope, (v) => (request = { ...request, auth: { ...request.auth, scope: v } }))}
                  {/if}
                </div>

              {:else if activeSection === 'notes'}
                <div class="p-3 flex flex-col gap-4">
                  <!-- Request name as heading -->
                  <h2 class="text-base font-semibold tracking-tight">{requestName}</h2>

                  <!-- Description -->
                  <div class="flex flex-col gap-1.5">
                    <span class="text-xs font-medium text-muted-foreground">description</span>
                    <Textarea
                      placeholder="Add a description for this request…"
                      value={descriptionPart}
                      oninput={(e) => setDescription(e.currentTarget.value)}
                      class="text-xs font-mono resize-none min-h-[80px]"
                    />
                  </div>

                  <!-- Saved responses -->
                  {#if savedResponses.length > 0}
                    <div class="flex flex-col gap-1.5">
                      <span class="text-xs font-medium text-muted-foreground">
                        Saved Responses ({savedResponses.length})
                      </span>
                      {#each savedResponses as resp}
                        <div class="border border-border/50 rounded overflow-hidden">
                          <div
                            role="none"
                            class="flex items-center gap-2 px-2 py-1.5 bg-muted/30 cursor-pointer hover:bg-muted/50 transition-colors select-none"
                            onclick={() => toggleResponseExpand(resp.timestamp)}
                          >
                            <span class="text-[10px] font-mono text-muted-foreground flex-1 truncate">
                              {new Date(resp.timestamp).toLocaleString()}
                            </span>
                            <span class="text-[10px] font-semibold {statusClass(parseInt(resp.statusText))}">
                              {resp.statusText}
                            </span>
                            <span class="text-[10px] text-muted-foreground shrink-0">{resp.time}</span>
                            <button
                              type="button"
                              onclick={(e) => { e.stopPropagation(); deleteSavedResponse(resp.timestamp); }}
                              class="p-0.5 rounded hover:bg-muted text-muted-foreground hover:text-destructive transition-colors shrink-0"
                            ><Trash2 size={10} /></button>
                            <ChevronDown
                              size={10}
                              class="text-muted-foreground shrink-0 transition-transform {expandedResponses.has(resp.timestamp) ? 'rotate-180' : ''}"
                            />
                          </div>
                          {#if expandedResponses.has(resp.timestamp)}
                            <div class="h-48 border-t border-border/30">
                              <CodeMirrorEditor
                                value={resp.body}
                                language={resp.lang}
                                readonly
                                class="h-full"
                              />
                            </div>
                          {/if}
                        </div>
                      {/each}
                    </div>
                  {:else}
                    <p class="text-xs text-muted-foreground text-center py-2">
                      No saved responses yet — use the ↓ icon in the response pane.
                    </p>
                  {/if}
                </div>
              {/if}

            </div>
          {/if}

        </div>
      </Pane>

      <!-- Resizable handle -->
      <PaneHandle withHandle />

      <!-- Response pane -->
      <Pane defaultSize={55} minSize={15} class="flex flex-col overflow-hidden min-h-0">

        {#if sending}
          <div class="flex-1 flex items-center justify-center gap-2 text-muted-foreground">
            <Loader2 size={16} class="animate-spin" /><span class="text-sm">Sending…</span>
          </div>

        {:else if responseError}
          <div class="p-4 flex items-start gap-2 text-destructive">
            <AlertTriangle size={14} class="shrink-0 mt-0.5" />
            <span class="text-xs break-all">{responseError}</span>
          </div>

        {:else if response}
          <!-- Status bar: tabs LEFT, stats + save RIGHT -->
          <div class="flex items-center gap-1 px-3 py-2 border-b shrink-0 bg-muted/20">
            <!-- Body / Headers tabs on the left -->
            {#each ['body', 'headers'] as rt}
              <button
                type="button"
                onclick={() => (responseTab = rt)}
                class="text-xs capitalize px-2 py-0.5 rounded transition-colors
                  {responseTab === rt ? 'bg-muted text-foreground' : 'text-muted-foreground hover:text-foreground'}"
              >{rt}</button>
            {/each}

            <!-- HTML source / preview toggle -->
            {#if responseLang === 'html' && responseTab === 'body'}
              <div class="flex border border-border/50 rounded overflow-hidden ml-1">
                {#each [['source', 'Source'], ['preview', 'Preview']] as [v, label]}
                  <button
                    type="button"
                    onclick={() => (responseView = v)}
                    class="text-[10px] px-2 py-0.5 transition-colors
                      {responseView === v ? 'bg-muted text-foreground' : 'text-muted-foreground hover:text-foreground'}"
                  >{label}</button>
                {/each}
              </div>
            {/if}

            <div class="flex-1"></div>

            <!-- Stats on the right -->
            <div class="flex items-center gap-3">
              <span class="text-xs font-semibold {statusClass(response.status)}">
                {response.status} {response.statusText}
              </span>
              <span class="text-xs text-muted-foreground">{response.durationMs}ms</span>
              <span class="text-xs text-muted-foreground">{formatSize(response.sizeBytes)}</span>
            </div>

            <!-- Save response to notes -->
            <button
              type="button"
              onclick={saveResponse}
              title="Save response to notes"
              class="p-1 rounded hover:bg-muted text-muted-foreground hover:text-foreground transition-colors"
            >
              <Download size={12} />
            </button>
          </div>

          {#if responseTab === 'body'}
            {#if responseLang === 'binary'}
              <!-- Binary / file response -->
              <div class="flex-1 flex flex-col items-center justify-center gap-2 text-muted-foreground">
                <FileIcon size={32} class="opacity-20" />
                <p class="text-xs font-medium">Binary response</p>
                <p class="text-[10px] opacity-60">
                  {response.headers.find(h => h.key?.toLowerCase() === 'content-type')?.value ?? 'Unknown content type'}
                </p>
              </div>
            {:else if responseLang === 'html' && responseView === 'preview'}
              <!-- HTML preview in sandboxed iframe -->
              <iframe
                srcdoc={response.body}
                sandbox="allow-scripts allow-same-origin"
                class="flex-1 w-full border-0 bg-white"
                title="Response preview"
              ></iframe>
            {:else}
              <CodeMirrorEditor
                value={prettyResponseBody}
                language={responseLang}
                readonly
                class="flex-1 min-h-0"
              />
            {/if}
          {:else}
            <div class="flex-1 overflow-auto">
              <table class="w-full text-xs">
                <thead>
                  <tr class="border-b text-muted-foreground">
                    <th class="text-left px-3 py-1.5 font-medium">Header</th>
                    <th class="text-left px-3 py-1.5 font-medium">Value</th>
                  </tr>
                </thead>
                <tbody>
                  {#each response.headers as h}
                    <tr class="border-b border-border/40 hover:bg-muted/30">
                      <td class="px-3 py-1.5 font-mono text-muted-foreground">{h.key}</td>
                      <td class="px-3 py-1.5 font-mono break-all">{h.value}</td>
                    </tr>
                  {/each}
                </tbody>
              </table>
            </div>
          {/if}

        {:else}
          <div class="flex-1 flex items-center justify-center text-xs text-muted-foreground gap-2">
            <Globe size={16} class="opacity-30" />
            Hit Send to see the response
          </div>
        {/if}

      </Pane>
    </PaneGroup>

  {/if}
</div>

<!-- ── Form params table ── -->
{#snippet formParamsTable()}
  <div class="flex-1 overflow-y-auto">
    <table class="w-full text-xs table-fixed">
      <colgroup>
        <col style="width:28px" />
        <col style="width:32%" />
        <col style="width:68px" />
        <col />
        <col style="width:28px" />
      </colgroup>
      <thead>
        <tr class="border-b text-muted-foreground">
          <th class="px-2 py-1.5"></th>
          <th class="text-left px-2 py-1.5 font-medium">Key</th>
          <th class="text-left px-2 py-1.5 font-medium">Type</th>
          <th class="text-left px-2 py-1.5 font-medium">Value / File</th>
          <th></th>
        </tr>
      </thead>
      <tbody>
        {#each (request.form_params || []) as row, i}
          <tr class="group border-b border-border/30 hover:bg-muted/20">
            <td class="px-2 py-1">
              <input
                type="checkbox"
                checked={row.enabled}
                onchange={(e) => (request = { ...request, form_params: updateRow(request.form_params, i, 'enabled', e.currentTarget.checked) })}
                class="rounded border-border"
              />
            </td>
            <td class="px-1 py-0.5">
              <Input
                type="text"
                value={row.key}
                placeholder="key"
                oninput={(e) => (request = { ...request, form_params: updateRow(request.form_params, i, 'key', e.currentTarget.value) })}
                class="w-full bg-transparent border-0 shadow-none h-7 px-1 py-0 text-xs font-mono focus-visible:ring-0 focus-visible:bg-muted/40"
              />
            </td>
            <td class="px-1 py-0.5">
              <DropdownMenu.Root>
                <DropdownMenu.Trigger class="flex items-center gap-0.5 px-1.5 py-0.5 rounded border bg-muted/30 hover:bg-muted text-[10px] transition-colors w-full justify-between">
                  {row.param_type === 'file' ? 'file' : 'text'}
                  <ChevronDown size={8} />
                </DropdownMenu.Trigger>
                <DropdownMenu.Content class="min-w-16">
                  <DropdownMenu.Item class="text-xs" onclick={() => (request = { ...request, form_params: updateRow(request.form_params, i, 'param_type', 'text') })}>text</DropdownMenu.Item>
                  <DropdownMenu.Item class="text-xs" onclick={() => (request = { ...request, form_params: updateRow(request.form_params, i, 'param_type', 'file') })}>file</DropdownMenu.Item>
                </DropdownMenu.Content>
              </DropdownMenu.Root>
            </td>
            <td class="px-1 py-0.5">
              {#if row.param_type === 'file'}
                <div class="flex items-center gap-1 min-w-0">
                  <button
                    type="button"
                    onclick={() => pickFormFile(i)}
                    class="flex items-center gap-1 px-2 py-0.5 rounded border bg-muted/40 hover:bg-muted text-[10px] shrink-0 transition-colors"
                  >
                    <FileIcon size={10} />Browse
                  </button>
                  <span class="text-[10px] text-muted-foreground truncate" title={row.value}>
                    {row.value ? row.value.split(/[\\/]/).pop() : 'No file chosen'}
                  </span>
                </div>
              {:else}
                <Input
                  type="text"
                  value={row.value}
                  placeholder="value"
                  oninput={(e) => (request = { ...request, form_params: updateRow(request.form_params, i, 'value', e.currentTarget.value) })}
                  class="w-full bg-transparent border-0 shadow-none h-7 px-1 py-0 text-xs font-mono focus-visible:ring-0 focus-visible:bg-muted/40"
                />
              {/if}
            </td>
            <td class="px-1">
              <button
                type="button"
                onclick={() => (request = { ...request, form_params: removeRow(request.form_params, i) })}
                class="opacity-0 group-hover:opacity-100 p-0.5 rounded hover:bg-muted text-muted-foreground hover:text-destructive transition-all"
              ><Trash2 size={11} /></button>
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
    <button
      type="button"
      onclick={() => (request = { ...request, form_params: addRow(request.form_params || [], { param_type: 'text' }) })}
      class="flex items-center gap-1.5 px-3 py-2 text-xs text-muted-foreground hover:text-foreground transition-colors"
    >
      <Plus size={12} />Add field
    </button>
  </div>
{/snippet}

<!-- ── Generic KV table ── -->
{#snippet kvTable(rows, setRows, keyPlaceholder, valPlaceholder)}
  <table class="w-full text-xs table-fixed">
    <colgroup>
      <col style="width:28px" />
      <col style="width:40%" />
      <col />
      <col style="width:28px" />
    </colgroup>
    <thead>
      <tr class="border-b text-muted-foreground">
        <th class="px-2 py-1.5"></th>
        <th class="text-left px-2 py-1.5 font-medium">{keyPlaceholder}</th>
        <th class="text-left px-2 py-1.5 font-medium">{valPlaceholder}</th>
        <th></th>
      </tr>
    </thead>
    <tbody>
      {#each rows as row, i}
        <tr class="group border-b border-border/30 hover:bg-muted/20">
          <td class="px-2 py-1">
            <input
              type="checkbox"
              checked={row.enabled}
              onchange={(e) => setRows(updateRow(rows, i, 'enabled', e.currentTarget.checked))}
              class="rounded border-border"
            />
          </td>
          <td class="px-1 py-0.5">
            <Input
              type="text"
              value={row.key}
              placeholder="Key"
              oninput={(e) => setRows(updateRow(rows, i, 'key', e.currentTarget.value))}
              class="w-full bg-transparent border-0 shadow-none h-7 px-1 py-0 text-xs font-mono focus-visible:ring-0 focus-visible:bg-muted/40"
            />
          </td>
          <td class="px-1 py-0.5">
            <Input
              type="text"
              value={row.value}
              placeholder="Value"
              oninput={(e) => setRows(updateRow(rows, i, 'value', e.currentTarget.value))}
              class="w-full bg-transparent border-0 shadow-none h-7 px-1 py-0 text-xs font-mono focus-visible:ring-0 focus-visible:bg-muted/40"
            />
          </td>
          <td class="px-1">
            <button
              type="button"
              onclick={() => setRows(removeRow(rows, i))}
              class="opacity-0 group-hover:opacity-100 p-0.5 rounded hover:bg-muted text-muted-foreground hover:text-destructive transition-all"
            ><Trash2 size={11} /></button>
          </td>
        </tr>
      {/each}
    </tbody>
  </table>
  <button
    type="button"
    onclick={() => setRows(addRow(rows))}
    class="flex items-center gap-1.5 px-3 py-2 text-xs text-muted-foreground hover:text-foreground transition-colors"
  >
    <Plus size={12} />Add row
  </button>
{/snippet}

{#snippet authField(label, id, value, set, secret = false)}
  <div class="flex items-center gap-2">
    <span class="text-xs text-muted-foreground w-24 shrink-0">{label}</span>
    <Input
      {id}
      type={secret ? 'password' : 'text'}
      {value}
      oninput={(e) => set(e.currentTarget.value)}
      class="flex-1 text-xs font-mono h-7"
    />
  </div>
{/snippet}
