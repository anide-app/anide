<script>
  // @ts-nocheck
  /**
   * Generic CodeMirror file editor with save / discard.
   *
   * Props:
   *   title  : string                        — shown in the header bar
   *   load   : () => Promise<string>         — called on mount to fetch content
   *   save   : (content: string) => Promise  — called on Save
   *   language?: 'yaml'|'markdown'|null      — auto-detected from title when omitted
   *   icon?  : Lucide component              — header icon (default: FileCode)
   */
  /**
   * onCancel — optional callback. When provided a "Cancel" button appears in the
   * header so the parent can switch away from edit mode without saving.
   */
  let { title, load, save: saveFn, language = undefined, icon: Icon = null, onCancel = null, onDirtyChange = null, externalChangeTick = 0 } = $props();

  import { EditorView, basicSetup } from 'codemirror';
  import { keymap } from '@codemirror/view';
  import { indentWithTab } from '@codemirror/commands';
  import { Compartment } from '@codemirror/state';
  import { LanguageDescription, StreamLanguage } from '@codemirror/language';

  const envLanguage = StreamLanguage.define({
    startState: () => ({ inValue: false }),
    token(stream, state) {
      if (stream.sol()) state.inValue = false;
      if (stream.eatSpace()) return null;
      if (!state.inValue) {
        if (stream.peek() === '#') { stream.skipToEnd(); return 'comment'; }
        if (stream.match(/^export(?=\s)/)) return 'keyword';
        if (stream.match(/^[A-Za-z_][A-Za-z0-9_.]*(?==)/)) return 'variableName';
        if (stream.eat('=')) { state.inValue = true; return 'operator'; }
      } else {
        const q = stream.peek();
        if (q === '"' || q === "'") {
          stream.next();
          while (!stream.eol()) {
            const c = stream.next();
            if (c === '\\') stream.next();
            else if (c === q) break;
          }
          return 'string';
        }
        if (!stream.eol()) { stream.skipToEnd(); return 'string'; }
      }
      stream.next();
      return null;
    },
  });
  import { yaml } from '@codemirror/lang-yaml';
  import { markdown, markdownLanguage } from '@codemirror/lang-markdown';
  import { languages } from '@codemirror/language-data';
  import { oneDark } from '@codemirror/theme-one-dark';
  import { mode } from 'mode-watcher';
  import { untrack } from 'svelte';
  import { Save, Undo2, X, Loader2, AlertTriangle, FileCode } from '@lucide/svelte';

  const themeCompartment = new Compartment();
  const langCompartment  = new Compartment();
  let   langLoadSeq      = 0; // per-instance, cancels stale async loads

  let resolvedLang = $derived(language !== undefined ? language : detectLang(title));

  // ── State ──────────────────────────────────────────────────────────────────
  let originalContent = $state('');
  let savedContent = '';   // plain let — mutable dirty baseline; updated on load and save
  let loadSeq  = 0;        // request ID to discard out-of-order load() completions
  let loading  = $state(true);
  let saving   = $state(false);
  let error    = $state('');
  let isDirty  = $state(false);

  let editorEl = $state(null);
  let view     = null; // managed by $effect, not reactive

  // ── Load ───────────────────────────────────────────────────────────────────
  $effect(() => {
    // Track only load identity and title — NOT the reactive state read inside
    // load() (e.g. projectInfo in ReadmeTab). untrack() prevents those reads
    // from becoming effect dependencies and wiping the editor on unrelated updates.
    load; title;

    loading = true;
    error   = '';
    isDirty = false;

    const req = ++loadSeq;
    untrack(() => load())
      .then(c  => { if (req === loadSeq) { originalContent = c; savedContent = c; } })
      .catch(e => { if (req === loadSeq) error = String(e); })
      .finally(() => { if (req === loadSeq) loading = false; });
  });

  // ── Create / recreate editor when content loads ────────────────────────────
  $effect(() => {
    if (!editorEl || loading || error) return;

    view?.destroy();

    view = new EditorView({
      doc: savedContent,
      extensions: [
        basicSetup,
        keymap.of([indentWithTab]),
        langCompartment.of(getSyncLangExt(resolvedLang)),
        EditorView.theme({
          '&':                   { height: '100%', backgroundColor: 'transparent', color: 'var(--foreground)', fontFamily: 'Geist Mono, ui-monospace, monospace', fontSize: '13px' },
          '.cm-content':         { padding: '1rem 0.5rem', lineHeight: '1.65' },
          '.cm-gutters':         { backgroundColor: 'color-mix(in oklch, var(--muted) 30%, transparent)', color: 'color-mix(in oklch, var(--muted-foreground) 60%, transparent)', border: 'none' },
          '.cm-activeLineGutter':{ backgroundColor: 'color-mix(in oklch, var(--muted) 50%, transparent)' },
          '.cm-activeLine':      { backgroundColor: 'color-mix(in oklch, var(--muted) 25%, transparent)' },
          '.cm-focused':         { outline: 'none' },
          '.cm-scroller':        { overflow: 'auto' },
        }),
        themeCompartment.of(untrack(() => mode.current) === 'dark' ? oneDark : []),
        EditorView.updateListener.of(upd => {
          if (upd.docChanged) isDirty = upd.state.doc.toString() !== savedContent;
        }),
      ],
      parent: editorEl,
    });

    // Async syntax highlighting for non-yaml/markdown files
    if (resolvedLang !== 'yaml' && resolvedLang !== 'markdown') {
      const seq = ++langLoadSeq;
      loadLangFor(title).then(ext => {
        if (ext && langLoadSeq === seq && view) {
          view.dispatch({ effects: langCompartment.reconfigure(ext) });
        }
      });
    }

    return () => { view?.destroy(); view = null; };
  });

  // Notify parent when dirty state changes; also clears on unmount.
  // untrack() prevents the callback reference from being a dependency —
  // otherwise a new function identity from the parent causes an infinite loop.
  $effect(() => {
    const d = isDirty;
    untrack(() => onDirtyChange?.(d));
    return () => untrack(() => onDirtyChange?.(false));
  });

  // Auto-reload when the file changes on disk and the tab has no unsaved edits.
  // Uses loadSeq so a stale response never overwrites a newer one.
  $effect(() => {
    const tick = externalChangeTick;
    if (!tick) return;
    if (untrack(() => loading || isDirty)) return;
    const req = ++loadSeq;
    untrack(() => load())
      .then(c => {
        if (req !== loadSeq) return;
        savedContent = c;
        originalContent = c;
        if (view) view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: c } });
      })
      .catch(() => {});
  });

  // Hot-swap dark/light theme
  $effect(() => {
    const isDark = mode.current === 'dark';
    if (!view) return;
    view.dispatch({ effects: themeCompartment.reconfigure(isDark ? oneDark : []) });
  });

  // ── Save ───────────────────────────────────────────────────────────────────
  async function handleSave() {
    if (!view || !isDirty || saving) return;
    saving = true;
    error  = '';
    try {
      const content = view.state.doc.toString();
      await saveFn(content);
      originalContent = content;
      savedContent = content;
      isDirty = false;
    } catch (e) {
      error = String(e);
    } finally {
      saving = false;
    }
  }

  // ── Discard ────────────────────────────────────────────────────────────────
  function discard() {
    if (!view) return;
    view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: savedContent } });
    isDirty = false;
  }

  // ── Ctrl/Cmd + S ──────────────────────────────────────────────────────────
  function onKeyDown(e) {
    if ((e.ctrlKey || e.metaKey) && e.key === 's') { e.preventDefault(); void handleSave(); }
  }

  // ── Helpers ────────────────────────────────────────────────────────────────
  function detectLang(t) {
    const ext = t?.split('.').pop()?.toLowerCase();
    if (ext === 'yml' || ext === 'yaml') return 'yaml';
    if (ext === 'md'  || ext === 'mdx')  return 'markdown';
    return null;
  }

  // Synchronous: only yaml/markdown have instant support; everything else waits for async load
  function getSyncLangExt(lang) {
    if (lang === 'yaml')     return yaml();
    if (lang === 'markdown') return markdown({ base: markdownLanguage, codeLanguages: languages });
    return [];
  }

  // Async: match filename against language-data registry (covers JS, TS, Rust, Go, Python, CSS, …)
  // .svelte is not in language-data; HTML mode handles <script>/<style>/template well.
  function loadLangFor(filename) {
    const ext = filename?.split('.').pop()?.toLowerCase();
    if (ext === 'svelte') {
      return import('codemirror-lang-svelte').then(m => m.svelte()).catch(() => null);
    }
    const base = filename?.split(/[\\/]/).pop() ?? '';
    if (base.toLowerCase().startsWith('.env')) {
      return Promise.resolve(envLanguage);
    }
    const desc = LanguageDescription.matchFilename(languages, filename);
    return desc ? desc.load().catch(() => null) : Promise.resolve(null);
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="h-full flex flex-col overflow-hidden" onkeydown={onKeyDown}>

  <!-- Header -->
  <div class="flex items-center gap-2 px-4 border-b shrink-0 h-10">
    {#if Icon}
      <Icon size={14} class="text-muted-foreground shrink-0" />
    {:else}
      <FileCode size={14} class="text-muted-foreground shrink-0" />
    {/if}
    <span class="text-sm font-medium flex-1 truncate font-mono">{title}</span>

    {#if isDirty}
      <span class="text-[10px] text-muted-foreground/60 shrink-0">unsaved changes</span>
      <button
        type="button"
        onclick={discard}
        disabled={saving}
        class="flex items-center gap-1.5 px-2.5 py-1 rounded text-xs text-muted-foreground
               hover:text-foreground hover:bg-muted transition-colors disabled:opacity-50"
      >
        <Undo2 size={12} />Discard
      </button>
    {/if}

    {#if onCancel}
      <button
        type="button"
        onclick={onCancel}
        disabled={saving}
        class="flex items-center gap-1.5 px-2.5 py-1 rounded text-xs text-muted-foreground
               hover:text-foreground hover:bg-muted transition-colors disabled:opacity-50"
      >
        <X size={12} />Cancel
      </button>
    {/if}

    {#if isDirty}
      <button
        type="button"
        onclick={handleSave}
        disabled={saving}
        class="flex items-center gap-1.5 px-2.5 py-1 rounded text-xs font-medium
               bg-primary text-primary-foreground hover:bg-primary/90 transition-colors disabled:opacity-50"
      >
        {#if saving}
          <Loader2 size={12} class="animate-spin" />Saving…
        {:else}
          <Save size={12} />Save
        {/if}
      </button>
    {/if}
  </div>

  <!-- Body -->
  {#if loading}
    <div class="flex flex-1 items-center justify-center gap-2 text-muted-foreground">
      <Loader2 size={16} class="animate-spin" />
      <span class="text-sm">Loading…</span>
    </div>
  {:else if error}
    <div class="flex flex-1 items-center justify-center gap-2 text-destructive px-8">
      <AlertTriangle size={16} class="shrink-0" />
      <span class="text-sm break-all">{error}</span>
    </div>
  {:else}
    <div bind:this={editorEl} class="flex-1 overflow-hidden [&_.cm-editor]:h-full [&_.cm-editor]:text-[13px]"></div>
  {/if}

</div>
