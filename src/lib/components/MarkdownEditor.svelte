<script>
  // @ts-nocheck
  /**
   * Notion-style WYSIWYG markdown editor built on TipTap.
   *
   * Props:
   *   content        : string                         — initial markdown
   *   onSave?        : (md: string) => Promise|void
   *   onCancel?      : () => void
   *   onDirtyChange? : (dirty: boolean) => void
   *   title?         : string
   *   icon?          : Lucide component
   *   readonly?      : boolean
   *   headerExtra?   : Snippet — rendered in the header bar after the filename
   *   belowHeader?   : Snippet — rendered between the header bar and editor body
   */
  let {
    content: initialContent = '',
    onSave = null,
    onCancel = null,
    onDirtyChange = null,
    title = '',
    icon: Icon = null,
    readonly = false,
    extraDirty = false,
    headerExtra = null,
    belowHeader = null,
  } = $props();

  import { onMount, onDestroy } from 'svelte';
  import { untrack } from 'svelte';
  import { Editor } from '@tiptap/core';
  import StarterKit from '@tiptap/starter-kit';
  import { Markdown } from '@tiptap/markdown';
  import CodeBlockLowlight from '@tiptap/extension-code-block-lowlight';
  import { TaskItem, TaskList } from '@tiptap/extension-list';
  import { Table, TableRow, TableCell, TableHeader } from '@tiptap/extension-table';
  import { Placeholder } from '@tiptap/extensions';
  import { all, createLowlight } from 'lowlight';
  import { BubbleMenu, FloatingMenu } from 'svelte-tiptap';
  import {
    Bold, Italic, Strikethrough, Code, Code2, Heading1, Heading2, Heading3,
    List, ListOrdered, ListTodo, TextQuote, Link2, Table2, Trash2,
    Undo2, Save, X, Loader2, FileText, Check,
  } from '@lucide/svelte';

  const lowlight = createLowlight(all);

  let editorEl    = $state(null);
  let langInputEl = $state(null);
  let linkInputEl = $state(null);
  let editor      = $state(null);
  let isDirty     = $state(false);
  let saving      = $state(false);
  let showLink    = $state(false);
  let linkValue   = $state('');
  let originalMd  = '';

  // Derived from editor transaction updates (onTransaction reassigns `editor`)
  let inCodeBlock  = $derived(editor?.isActive('codeBlock') ?? false);
  let inTable      = $derived(editor?.isActive('table') ?? false);
  let codeBlockLang = $derived(inCodeBlock ? (editor?.getAttributes('codeBlock')?.language ?? '') : '');

  // Strip &nbsp; / U+00A0 that TipTap emits for empty paragraphs
  function cleanMd(raw) {
    if (!raw) return '';
    return raw
      .replace(/&nbsp;/g, '')
      .replace(/ /g, '')
      .replace(/[ \t]+$/gm, '')
      .trimEnd();
  }

  onMount(() => {
    originalMd = cleanMd(initialContent);

    const inst = new Editor({
      element: editorEl,
      extensions: [
        StarterKit.configure({
          codeBlock: false,
          link: {
            openOnClick: false,
            autolink: true,
            linkOnPaste: true,
            HTMLAttributes: { target: '_blank', rel: 'noopener noreferrer' },
          },
        }),
        Markdown.configure({
          html: false,
          tightLists: true,
          transformPastedText: true,
          transformCopiedText: true,
        }),
        CodeBlockLowlight.configure({ lowlight }),
        TaskList,
        TaskItem.configure({
          nested: true,
          HTMLAttributes: {
            style: 'display:flex;align-items:flex-start;gap:0.5rem;list-style:none;margin:0.2rem 0;',
          },
        }),
        Table.configure({ resizable: false }),
        TableRow,
        TableHeader,
        TableCell,
        Placeholder.configure({ placeholder: "Write something…" }),
      ],
      content: initialContent,
      contentType: 'markdown',
      editable: !readonly,
      onCreate() {},
      onUpdate({ editor: e }) {
        try {
          const md = cleanMd(e.getMarkdown?.());
          isDirty = md !== undefined ? md !== originalMd : true;
        } catch {
          isDirty = true;
        }
      },
      onTransaction({ editor: e }) {
        editor = undefined;
        editor = e;
      },
    });
    editor = inst;
  });

  onDestroy(() => { editor?.destroy(); editor = null; });

  export function focus() { editor?.commands.focus(); }

  // Resync when parent swaps the content prop while the editor is mounted (guard: clean only)
  $effect(() => {
    const incoming = cleanMd(initialContent);
    untrack(() => {
      if (!editor || isDirty || incoming === originalMd) return;
      originalMd = incoming;
      editor.commands.setContent(incoming, { contentType: 'markdown' });
      isDirty = false;
    });
  });

  $effect(() => { if (editor) editor.setEditable(!readonly, false); });

  $effect(() => {
    const d = isDirty;
    untrack(() => onDirtyChange?.(d));
    return () => untrack(() => onDirtyChange?.(false));
  });

  async function handleSave() {
    if (!editor || (!isDirty && !extraDirty) || saving) return;
    saving = true;
    try {
      const md = cleanMd(editor.getMarkdown?.() ?? editor.getHTML());
      await onSave?.(md);
      originalMd = md;
      isDirty = false;
    } catch (e) {
      console.error('MarkdownEditor save failed:', e);
    } finally {
      saving = false;
    }
  }

  function handleDiscard() {
    if (!editor) return;
    editor.commands.setContent(originalMd, { contentType: 'markdown' });
    isDirty = false;
  }

  function globalKeyDown(e) {
    if ((e.ctrlKey || e.metaKey) && e.key === 's') { e.preventDefault(); void handleSave(); }
  }

  // ── Link handling ──────────────────────────────────────────────────────────
  function openLink() {
    linkValue = editor?.getAttributes('link').href ?? '';
    showLink = true;
    setTimeout(() => linkInputEl?.focus(), 20);
  }

  function applyLink() {
    showLink = false;
    const url = linkValue.trim();
    if (!url) {
      editor?.chain().focus().unsetLink().run();
    } else {
      editor?.chain().focus().setLink({ href: url.startsWith('http') ? url : `https://${url}` }).run();
    }
    linkValue = '';
  }

  function cancelLink() {
    showLink = false;
    linkValue = '';
    editor?.commands.focus();
  }

  const isActive = (name, attrs) => editor?.isActive(name, attrs) ?? false;

  function bubbleMouseDown(e) {
    if (e.target !== linkInputEl) e.preventDefault();
  }

  const bubbleShouldShow = ({ editor: e, from, to }) =>
    !readonly && from !== to && !e.isActive('codeBlock');

  const tableShouldShow = ({ editor: e }) =>
    !readonly && e.isActive('table');

  const floatShouldShow = ({ editor: e, state }) => {
    if (!e.isEditable || e.isActive('codeBlock')) return false;
    const { selection } = state;
    const { empty } = selection;
    if (!empty) return false;
    const parent = selection.$anchor.parent;
    return parent.type.name === 'paragraph' && parent.textContent === '';
  };
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="h-full flex flex-col overflow-hidden" onkeydown={globalKeyDown}>

  <!-- Header -->
  <div class="flex items-center gap-2 px-4 border-b shrink-0 h-10">
    {#if Icon}<Icon size={14} class="text-muted-foreground shrink-0" />
    {:else}<FileText size={14} class="text-muted-foreground shrink-0" />{/if}
    <span class="text-sm font-medium truncate flex-1 min-w-0">{title}</span>

    <!-- Extra header content (e.g. frontmatter toggle) injected by parent -->
    {#if headerExtra}
      <span class="w-px h-4 bg-border shrink-0"></span>
      {@render headerExtra()}
    {/if}

    <!-- Language badge when cursor is inside a code block -->
    {#if inCodeBlock && !readonly}
      <span class="w-px h-4 bg-border shrink-0"></span>
      <input
        bind:this={langInputEl}
        value={codeBlockLang}
        onchange={(e) => editor?.commands.updateAttributes('codeBlock', { language: e.target.value })}
        onkeydown={(e) => { if (e.key === 'Enter' || e.key === 'Escape') { e.preventDefault(); editor?.commands.focus(); } }}
        onmousedown={(e) => e.stopPropagation()}
        placeholder="language"
        spellcheck="false"
        class="h-5 w-24 text-[11px] font-mono bg-muted/40 rounded px-2 text-muted-foreground border border-border/60 outline-none focus:border-primary/60 focus:text-foreground shrink-0"
      />
    {/if}

    {#if (isDirty || extraDirty) && !readonly}
      <span class="text-[10px] text-muted-foreground/60 shrink-0">unsaved changes</span>
    {/if}

    {#if isDirty && !readonly}
      <button type="button" onclick={handleDiscard} disabled={saving}
        class="flex items-center gap-1.5 px-2.5 py-1 rounded text-xs text-muted-foreground hover:text-foreground hover:bg-muted transition-colors disabled:opacity-50">
        <Undo2 size={12} />Discard
      </button>
    {/if}

    {#if onCancel}
      <button type="button" onclick={onCancel} disabled={saving}
        class="flex items-center gap-1.5 px-2.5 py-1 rounded text-xs text-muted-foreground hover:text-foreground hover:bg-muted transition-colors disabled:opacity-50">
        <X size={12} />Cancel
      </button>
    {/if}

    {#if (isDirty || extraDirty) && !readonly}
      <button type="button" onclick={handleSave} disabled={saving}
        class="flex items-center gap-1.5 px-2.5 py-1 rounded text-xs font-medium bg-primary text-primary-foreground hover:bg-primary/90 transition-colors disabled:opacity-50">
        {#if saving}<Loader2 size={12} class="animate-spin" />Saving…
        {:else}<Save size={12} />Save{/if}
      </button>
    {/if}
  </div>

  <!-- Below-header slot: e.g. frontmatter panel -->
  {@render belowHeader?.()}

  <!-- Editor body -->
  <div class="flex-1 overflow-auto relative">

    {#if editor}
      <!-- Floating menu on empty lines: block type shortcuts -->
      <FloatingMenu {editor} shouldShow={floatShouldShow} options={{ placement: 'left', offset: 20 }}>
        {#snippet children()}
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div
            onmousedown={(e) => e.preventDefault()}
            class="flex items-center gap-0.5 rounded-lg border border-border bg-popover shadow-md p-1"
          >
            <button onclick={() => editor?.chain().focus().toggleHeading({ level: 1 }).run()} class="bbl" title="Heading 1"><Heading1 size={13} /></button>
            <button onclick={() => editor?.chain().focus().toggleHeading({ level: 2 }).run()} class="bbl" title="Heading 2"><Heading2 size={13} /></button>
            <button onclick={() => editor?.chain().focus().toggleHeading({ level: 3 }).run()} class="bbl" title="Heading 3"><Heading3 size={13} /></button>
            <span class="w-px h-4 bg-border mx-0.5 shrink-0"></span>
            <button onclick={() => editor?.chain().focus().toggleBulletList().run()} class="bbl" title="Bullet list"><List size={13} /></button>
            <button onclick={() => editor?.chain().focus().toggleOrderedList().run()} class="bbl" title="Ordered list"><ListOrdered size={13} /></button>
            <button onclick={() => editor?.chain().focus().toggleTaskList().run()} class="bbl" title="Task list"><ListTodo size={13} /></button>
            <span class="w-px h-4 bg-border mx-0.5 shrink-0"></span>
            <button onclick={() => editor?.chain().focus().toggleBlockquote().run()} class="bbl" title="Blockquote"><TextQuote size={13} /></button>
            <button onclick={() => editor?.chain().focus().toggleCodeBlock().run()} class="bbl" title="Code block"><Code2 size={13} /></button>
            <button onclick={() => editor?.chain().focus().insertTable({ rows: 3, cols: 3, withHeaderRow: true }).run()} class="bbl" title="Table"><Table2 size={13} /></button>
          </div>
        {/snippet}
      </FloatingMenu>

      <!-- Text selection bubble menu -->
      <BubbleMenu {editor} shouldShow={bubbleShouldShow} options={{ placement: 'top-start', offset: 8 }}>
        {#snippet children()}
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div
            onmousedown={bubbleMouseDown}
            class="flex items-center rounded-lg border border-border bg-popover shadow-lg overflow-hidden z-50"
          >
            {#if showLink}
              <div class="flex items-center gap-1 px-2 py-1">
                <span class="text-[10px] text-muted-foreground font-medium shrink-0">URL</span>
                <input
                  bind:this={linkInputEl}
                  bind:value={linkValue}
                  onkeydown={(e) => {
                    if (e.key === 'Enter') { e.preventDefault(); applyLink(); }
                    if (e.key === 'Escape') { e.preventDefault(); cancelLink(); }
                  }}
                  type="url"
                  placeholder="https://"
                  class="h-6 w-52 bg-transparent text-xs outline-none text-foreground placeholder:text-muted-foreground/40 px-1"
                />
                <button onclick={applyLink} class="bbl text-primary" title="Apply"><Check size={12} /></button>
                <button onclick={cancelLink} class="bbl" title="Cancel"><X size={12} /></button>
              </div>
            {:else}
              <div class="flex items-center gap-0 p-1">
                <button onclick={() => editor?.chain().focus().toggleBold().run()} class="bbl {isActive('bold') ? 'on' : ''}" title="Bold"><Bold size={13} /></button>
                <button onclick={() => editor?.chain().focus().toggleItalic().run()} class="bbl {isActive('italic') ? 'on' : ''}" title="Italic"><Italic size={13} /></button>
                <button onclick={() => editor?.chain().focus().toggleStrike().run()} class="bbl {isActive('strike') ? 'on' : ''}" title="Strikethrough"><Strikethrough size={13} /></button>
                <button onclick={() => editor?.chain().focus().toggleCode().run()} class="bbl {isActive('code') ? 'on' : ''}" title="Inline code"><Code size={13} /></button>
                <span class="w-px h-4 bg-border mx-0.5 shrink-0"></span>
                <button onclick={() => editor?.chain().focus().toggleHeading({ level: 1 }).run()} class="bbl {isActive('heading', { level: 1 }) ? 'on' : ''}" title="H1"><Heading1 size={13} /></button>
                <button onclick={() => editor?.chain().focus().toggleHeading({ level: 2 }).run()} class="bbl {isActive('heading', { level: 2 }) ? 'on' : ''}" title="H2"><Heading2 size={13} /></button>
                <button onclick={() => editor?.chain().focus().toggleHeading({ level: 3 }).run()} class="bbl {isActive('heading', { level: 3 }) ? 'on' : ''}" title="H3"><Heading3 size={13} /></button>
                <span class="w-px h-4 bg-border mx-0.5 shrink-0"></span>
                <button onclick={() => editor?.chain().focus().toggleBulletList().run()} class="bbl {isActive('bulletList') ? 'on' : ''}" title="Bullet list"><List size={13} /></button>
                <button onclick={() => editor?.chain().focus().toggleOrderedList().run()} class="bbl {isActive('orderedList') ? 'on' : ''}" title="Ordered list"><ListOrdered size={13} /></button>
                <button onclick={() => editor?.chain().focus().toggleTaskList().run()} class="bbl {isActive('taskList') ? 'on' : ''}" title="Task list"><ListTodo size={13} /></button>
                <button onclick={() => editor?.chain().focus().toggleBlockquote().run()} class="bbl {isActive('blockquote') ? 'on' : ''}" title="Quote"><TextQuote size={13} /></button>
                <span class="w-px h-4 bg-border mx-0.5 shrink-0"></span>
                <button onclick={openLink} class="bbl {isActive('link') ? 'on' : ''}" title="Link"><Link2 size={13} /></button>
              </div>
            {/if}
          </div>
        {/snippet}
      </BubbleMenu>

      <!-- Table actions bubble — shows whenever cursor is inside a table -->
      <BubbleMenu {editor} shouldShow={tableShouldShow} options={{ placement: 'top', offset: 8 }}>
        {#snippet children()}
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div
            onmousedown={(e) => e.preventDefault()}
            class="flex items-center gap-0.5 rounded-lg border border-border bg-popover shadow-lg p-1 z-50"
          >
            <button
              onclick={() => editor?.chain().focus().addColumnBefore().run()}
              class="bbl" title="Add column before">
              <span class="text-[10px] font-mono leading-none">+◀</span>
            </button>
            <button
              onclick={() => editor?.chain().focus().addColumnAfter().run()}
              class="bbl" title="Add column after">
              <span class="text-[10px] font-mono leading-none">▶+</span>
            </button>
            <button
              onclick={() => editor?.chain().focus().deleteColumn().run()}
              class="bbl" title="Delete column">
              <span class="text-[10px] font-mono leading-none">−col</span>
            </button>
            <span class="w-px h-4 bg-border mx-0.5 shrink-0"></span>
            <button
              onclick={() => editor?.chain().focus().addRowBefore().run()}
              class="bbl" title="Add row above">
              <span class="text-[10px] font-mono leading-none">+▲</span>
            </button>
            <button
              onclick={() => editor?.chain().focus().addRowAfter().run()}
              class="bbl" title="Add row below">
              <span class="text-[10px] font-mono leading-none">▼+</span>
            </button>
            <button
              onclick={() => editor?.chain().focus().deleteRow().run()}
              class="bbl" title="Delete row">
              <span class="text-[10px] font-mono leading-none">−row</span>
            </button>
            <span class="w-px h-4 bg-border mx-0.5 shrink-0"></span>
            <button
              onclick={() => editor?.chain().focus().deleteTable().run()}
              class="bbl text-destructive hover:bg-destructive/10"
              title="Delete table">
              <Trash2 size={12} />
            </button>
          </div>
        {/snippet}
      </BubbleMenu>
    {/if}

    <!-- TipTap editor mount point -->
    <div bind:this={editorEl} class="md-editor min-h-full px-10 py-8"></div>

  </div>
</div>

<style>
  /* ── Bubble / floating menu button ──────────────────────────────────────── */
  :global(.bbl) {
    display: flex; align-items: center; justify-content: center;
    width: 1.625rem; height: 1.625rem;
    border-radius: 0.25rem;
    color: var(--muted-foreground);
    flex-shrink: 0;
    transition: color 0.1s, background-color 0.1s;
  }
  :global(.bbl:hover) { color: var(--foreground); background: var(--muted); }
  :global(.bbl.on)    { color: var(--primary); background: color-mix(in oklch, var(--primary) 15%, transparent); }

  /* ── Editor prose ────────────────────────────────────────────────────────── */
  :global(.md-editor .ProseMirror) {
    outline: none;
    color: var(--foreground);
    font-size: 1rem;
    line-height: 1.75;
    min-height: 100%;
    caret-color: var(--primary);
  }

  /* Placeholder */
  :global(.md-editor .ProseMirror .is-editor-empty:first-child::before) {
    content: attr(data-placeholder);
    float: left; height: 0; pointer-events: none;
    color: color-mix(in oklch, var(--muted-foreground) 50%, transparent);
  }

  /* Paragraphs */
  :global(.md-editor .ProseMirror p) { margin: 0.35rem 0; }

  /* Headings */
  :global(.md-editor .ProseMirror h1) {
    font-size: 1.875rem; font-weight: 800; line-height: 1.2;
    margin: 1.75rem 0 0.5rem; letter-spacing: -0.025em;
    border-bottom: 1px solid var(--border); padding-bottom: 0.5rem;
  }
  :global(.md-editor .ProseMirror h2) {
    font-size: 1.375rem; font-weight: 700; line-height: 1.3;
    margin: 1.5rem 0 0.4rem;
    border-bottom: 1px solid color-mix(in oklch, var(--border) 60%, transparent); padding-bottom: 0.3rem;
  }
  :global(.md-editor .ProseMirror h3) {
    font-size: 1.125rem; font-weight: 600; line-height: 1.4; margin: 1.25rem 0 0.35rem;
  }
  :global(.md-editor .ProseMirror h4) { font-size: 1rem; font-weight: 600; margin: 1rem 0 0.3rem; }

  :global(.md-editor .ProseMirror strong) { font-weight: 700; }
  :global(.md-editor .ProseMirror em)     { font-style: italic; }
  :global(.md-editor .ProseMirror s)      { text-decoration: line-through; color: var(--muted-foreground); }

  /* Inline code */
  :global(.md-editor .ProseMirror :not(pre) > code) {
    font-family: 'Geist Mono', ui-monospace, monospace;
    font-size: 0.82em;
    padding: 0.12em 0.38em;
    border-radius: 0.25rem;
    background: var(--muted);
    color: var(--foreground);
    border: 1px solid var(--border);
  }

  /* Code block */
  :global(.md-editor .ProseMirror pre) {
    background: var(--muted);
    border: 1px solid var(--border);
    border-radius: 0.625rem;
    padding: 1rem 1.25rem;
    margin: 1rem 0;
    overflow-x: auto;
    font-family: 'Geist Mono', ui-monospace, monospace;
    font-size: 0.82rem;
    line-height: 1.65;
  }
  :global(.md-editor .ProseMirror pre code) {
    background: transparent; border: none; padding: 0;
    font-size: inherit; border-radius: 0; color: var(--foreground);
  }

  /* Blockquote */
  :global(.md-editor .ProseMirror blockquote) {
    border-left: 3px solid color-mix(in oklch, var(--primary) 60%, transparent);
    padding-left: 1rem; margin: 0.75rem 0;
    color: var(--muted-foreground);
  }

  /* Lists */
  :global(.md-editor .ProseMirror ul)   { list-style: disc;    padding-left: 1.5rem; margin: 0.35rem 0; }
  :global(.md-editor .ProseMirror ol)   { list-style: decimal; padding-left: 1.5rem; margin: 0.35rem 0; }
  :global(.md-editor .ProseMirror li)   { margin: 0.15rem 0; }
  :global(.md-editor .ProseMirror li p) { margin: 0; }

  /* ── Task list (checkbox) ─────────────────────────────────────────────────── */
  :global(.md-editor .ProseMirror ul[data-type="taskList"]) {
    list-style: none; padding-left: 0.25rem;
  }
  :global(.md-editor .ProseMirror ul[data-type="taskList"] > li) {
    list-style: none !important;
    display: flex !important;
    align-items: flex-start !important;
    gap: 0.5rem !important;
  }
  :global(.md-editor .ProseMirror ul[data-type="taskList"] > li > label) {
    display: flex; align-items: center;
    flex-shrink: 0; cursor: pointer;
    margin-top: 0.3rem;
  }
  :global(.md-editor .ProseMirror ul[data-type="taskList"] > li > label input[type="checkbox"]) {
    position: static !important;
    opacity: 1 !important;
    width: 0.9rem !important;
    height: 0.9rem !important;
    margin: 0 !important;
    flex-shrink: 0 !important;
    cursor: pointer !important;
    border-radius: 0.2rem !important;
    color: var(--primary) !important;
    border-color: color-mix(in oklch, var(--primary) 55%, var(--border)) !important;
  }
  :global(.md-editor .ProseMirror ul[data-type="taskList"] > li > label span) {
    display: none !important;
  }
  :global(.md-editor .ProseMirror ul[data-type="taskList"] > li[data-checked="true"] > label input[type="checkbox"]) {
    background-color: var(--primary) !important;
    border-color: var(--primary) !important;
  }
  :global(.md-editor .ProseMirror ul[data-type="taskList"] > li > *:not(label)) {
    flex: 1; min-width: 0;
  }
  :global(.md-editor .ProseMirror ul[data-type="taskList"] > li[data-checked="true"] > *:not(label)) {
    color: var(--muted-foreground); text-decoration: line-through;
  }

  /* HR */
  :global(.md-editor .ProseMirror hr) {
    border: none; border-top: 1px solid var(--border); margin: 1.5rem 0;
  }

  /* Links */
  :global(.md-editor .ProseMirror a) {
    color: var(--primary); text-decoration: underline;
    text-underline-offset: 2px; cursor: pointer;
  }
  :global(.md-editor .ProseMirror a:hover) { opacity: 0.8; }

  /* Tables */
  :global(.md-editor .ProseMirror table) {
    width: 100%;
    border-collapse: separate;
    border-spacing: 0;
    margin: 1rem 0;
    font-size: 0.9em;
    border: 1px solid var(--border);
    border-radius: 0.5rem;
    overflow: hidden;
  }
  :global(.md-editor .ProseMirror th) {
    background: var(--muted);
    font-weight: 600; text-align: left;
    padding: 0.5rem 0.75rem;
    border-bottom: 1px solid var(--border);
    border-right: 1px solid var(--border);
    font-size: 0.85em;
  }
  :global(.md-editor .ProseMirror th:last-child) { border-right: none; }
  :global(.md-editor .ProseMirror td) {
    padding: 0.4rem 0.75rem;
    border-bottom: 1px solid var(--border);
    border-right: 1px solid var(--border);
    vertical-align: top;
  }
  :global(.md-editor .ProseMirror td:last-child) { border-right: none; }
  :global(.md-editor .ProseMirror tr:last-child td,
          .md-editor .ProseMirror tr:last-child th) { border-bottom: none; }
  :global(.md-editor .ProseMirror tr:nth-child(even) td) {
    background: color-mix(in oklch, var(--muted) 40%, transparent);
  }
  :global(.md-editor .ProseMirror .selectedCell) {
    background: color-mix(in oklch, var(--primary) 12%, transparent) !important;
  }

  /* ── Lowlight syntax tokens ──────────────────────────────────────────────── */
  :global(.md-editor .hljs-comment, .md-editor .hljs-quote)                 { color: var(--muted-foreground); font-style: italic; }
  :global(.md-editor .hljs-keyword, .md-editor .hljs-selector-tag)          { color: #c678dd; }
  :global(.md-editor .hljs-string, .md-editor .hljs-attr)                   { color: #98c379; }
  :global(.md-editor .hljs-number, .md-editor .hljs-literal)                { color: #d19a66; }
  :global(.md-editor .hljs-built_in, .md-editor .hljs-type)                 { color: #e5c07b; }
  :global(.md-editor .hljs-title, .md-editor .hljs-section)                 { color: #61afef; font-weight: 600; }
  :global(.md-editor .hljs-variable, .md-editor .hljs-template-variable)    { color: #e06c75; }
  :global(.md-editor .hljs-name, .md-editor .hljs-attribute)                { color: #e06c75; }
  :global(.md-editor .hljs-tag)                                              { color: #e06c75; }
  :global(.md-editor .hljs-punctuation, .md-editor .hljs-operator)          { color: color-mix(in oklch, var(--foreground) 70%, transparent); }
  :global(.md-editor .hljs-meta)                                             { color: #61afef; }
  :global(.md-editor .hljs-regexp)                                           { color: #56b6c2; }
  :global(.md-editor .hljs-symbol, .md-editor .hljs-bullet)                 { color: #61aeee; }
  :global(.md-editor .hljs-deletion)                                         { color: #e06c75; }
  :global(.md-editor .hljs-addition)                                         { color: #98c379; }
</style>
