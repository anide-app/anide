<script>
  // @ts-nocheck
  import { onMount, onDestroy, tick } from 'svelte';
  import { Terminal } from '@xterm/xterm';
  import { WebglAddon } from '@xterm/addon-webgl';
  import { FitAddon } from '@xterm/addon-fit';
  import { WebLinksAddon } from '@xterm/addon-web-links';
  import '@xterm/xterm/css/xterm.css';
  import { listen } from '@tauri-apps/api/event';
  import { openUrl } from '@tauri-apps/plugin-opener';
  import { terminalCreate, terminalWrite, terminalResize, terminalClose } from '$lib/commands/terminal.js';
  import { workspace } from '$lib/stores/workspace.svelte.js';

  let { data, tabId } = $props();

  let containerEl = $state(null);

  let _mounted = true;
  let _term = null;
  let _fit = null;
  let _webgl = null;
  let _unlistenData = null;
  let _unlistenExit = null;
  let _ro = null;
  let _themeObserver = null;

  // Read --background and --foreground from the live CSS variables so the
  // terminal always matches the app's current light/dark theme exactly.
  function getCssVar(name) {
    return getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  }

  function getTheme() {
    return {
      background: getCssVar('--background'),
      foreground: getCssVar('--foreground'),
      cursor: getCssVar('--foreground'),
      cursorAccent: getCssVar('--background'),
      // Let xterm own all ANSI colours — no overrides here
    };
  }

  onMount(async () => {
    _term = new Terminal({
      allowProposedApi: true,
      fontFamily: '"Geist Mono", "Cascadia Code", Consolas, "Courier New", monospace',
      fontSize: 13,
      lineHeight: 1.0,
      letterSpacing: 0,
      cursorBlink: true,
      cursorStyle: 'block',
      scrollback: 10000,
      fastScrollModifier: 'alt',
      theme: getTheme(),
    });

    _fit = new FitAddon();
    _term.loadAddon(_fit);

    // Open links in the OS default browser via Tauri's opener
    _term.loadAddon(new WebLinksAddon((_event, uri) => {
      openUrl(uri).catch(() => {});
    }));

    _term.open(containerEl);

    // WebGL renderer: GPU-accelerated, flicker-free, highest FPS.
    // Must be loaded AFTER term.open() so the canvas exists in the DOM.
    try {
      _webgl = new WebglAddon();
      _webgl.onContextLoss(() => {
        _webgl?.dispose();
        _webgl = null;
        // xterm falls back to its built-in canvas renderer automatically
      });
      _term.loadAddon(_webgl);
    } catch {
      // WebGL unavailable — xterm uses canvas renderer
    }

    _fit.fit();

    // Keep terminal colours in sync when the user toggles dark/light mode.
    // mode-watcher toggles the 'dark' class on <html>.
    _themeObserver = new MutationObserver(() => {
      if (_term) _term.options.theme = getTheme();
    });
    _themeObserver.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ['class'],
    });

    // Register listeners BEFORE creating the PTY so we never miss early output
    _unlistenData = await listen(`terminal:data:${data.sessionId}`, (event) => {
      const bytes = Uint8Array.from(atob(event.payload), c => c.charCodeAt(0));
      _term.write(bytes);
    });
    if (!_mounted) { _unlistenData(); return; }

    _unlistenExit = await listen(`terminal:exit:${data.sessionId}`, () => {
      _term.write('\r\n\x1b[90m[process exited]\x1b[0m\r\n');
    });
    if (!_mounted) { _unlistenData(); _unlistenExit(); return; }

    try {
      await terminalCreate(data.sessionId, data.cwd || null, _term.cols, _term.rows, data.shell ?? null, data.shellArgs ?? null);
    } catch (err) {
      if (_mounted) _term.write(`\r\n\x1b[31mFailed to start shell: ${err}\x1b[0m\r\n`);
    }
    // onDestroy may have fired during the awaits above; if so, the PTY session
    // was created but never cleaned up (close() ran before the session existed).
    if (!_mounted) { terminalClose(data.sessionId).catch(() => {}); return; }

    // Fire-and-forget writes for zero input lag — no await, no queuing
    _term.onData(input => {
      if (!_mounted) return;
      terminalWrite(data.sessionId, input).catch(() => {});
    });

    window.addEventListener('focus', _onWindowFocus);

    // Refit + notify backend whenever the container is resized.
    // Guard against hidden tabs: display:none collapses dimensions to 0 and
    // a fit() at 0 cols would corrupt the PTY's line-wrap state.
    _ro = new ResizeObserver(() => {
      requestAnimationFrame(() => {
        if (!_mounted || !_fit || !_term) return;
        if (!containerEl.offsetWidth || !containerEl.offsetHeight) return;
        _fit.fit();
        terminalResize(data.sessionId, _term.cols, _term.rows).catch(() => {});
      });
    });
    _ro.observe(containerEl);
  });

  onDestroy(() => {
    _mounted = false;
    _themeObserver?.disconnect();
    _ro?.disconnect();
    _unlistenData?.();
    _unlistenExit?.();
    window.removeEventListener('focus', _onWindowFocus);
    terminalClose(data.sessionId).catch(() => {});
    _webgl?.dispose();
    _term?.dispose();
  });

  function _onWindowFocus() {
    if (_mounted && _term && workspace.activeTabId === tabId) _term.focus();
  }

  // Refit + focus when this tab becomes active.
  // tick() waits for Svelte to flush the display:none→block class change, then
  // double-rAF gives the browser time to compute layout before fit() reads dimensions.
  $effect(() => {
    if (workspace.activeTabId === tabId && _fit && _term) {
      tick().then(() => {
        requestAnimationFrame(() => {
          requestAnimationFrame(() => {
            if (!_mounted || !_fit || !_term) return;
            if (workspace.activeTabId !== tabId) return;
            _fit.fit();
            if (_term.cols > 0 && _term.rows > 0) {
              terminalResize(data.sessionId, _term.cols, _term.rows).catch(() => {});
              _term.focus();
            }
          });
        });
      });
    }
  });
</script>

<div bind:this={containerEl} class="w-full h-full bg-background"></div>
