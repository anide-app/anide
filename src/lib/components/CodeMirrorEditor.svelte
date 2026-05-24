<script>
  // @ts-nocheck
  import { onMount, untrack } from 'svelte';
  import { EditorView, basicSetup } from 'codemirror';
  import { placeholder as cmPlaceholder } from '@codemirror/view';
  import { EditorState } from '@codemirror/state';
  import { json } from '@codemirror/lang-json';
  import { html } from '@codemirror/lang-html';
  import { xml } from '@codemirror/lang-xml';

  let {
    value = $bindable(''),
    language = 'text',
    readonly = false,
    class: className = '',
    placeholder = '',
  } = $props();

  // App-matched theme — uses the same CSS variables as the rest of the UI
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

  let container = $state(null);
  let view = null;
  let _syncing = false;

  function langExtension(lang) {
    if (lang === 'json') return json();
    if (lang === 'html') return html();
    if (lang === 'xml') return xml();
    return null;
  }

  function buildExtensions(lang, isReadonly, ph) {
    const exts = [basicSetup, appTheme];
    if (isReadonly) {
      exts.push(EditorState.readOnly.of(true));
      exts.push(EditorView.editable.of(false));
    } else {
      exts.push(
        EditorView.updateListener.of((update) => {
          if (update.docChanged && !_syncing) {
            value = update.state.doc.toString();
          }
        })
      );
    }
    const le = langExtension(lang);
    if (le) exts.push(le);
    if (ph) exts.push(cmPlaceholder(ph));
    return exts;
  }

  onMount(() => {
    view = new EditorView({
      state: EditorState.create({
        doc: value ?? '',
        extensions: buildExtensions(language, readonly, placeholder),
      }),
      parent: container,
    });
    return () => { view?.destroy(); view = null; };
  });

  // Sync external value → editor
  $effect(() => {
    const v = value ?? '';
    if (!view) return;
    const current = view.state.doc.toString();
    if (current !== v) {
      _syncing = true;
      view.dispatch({ changes: { from: 0, to: current.length, insert: v } });
      _syncing = false;
    }
  });

  // Swap language when prop changes
  $effect(() => {
    const lang = language;
    if (!view || readonly) return;
    untrack(() => {
      const doc = view.state.doc.toString();
      view.setState(EditorState.create({
        doc,
        extensions: buildExtensions(lang, false, placeholder),
      }));
    });
  });
</script>

<div bind:this={container} class={className}></div>
