# Anide

The AI-native IDE. The AI writes. You ship.

---

You are the senior engineer. The AI is the contributor. Run Claude Code, Codex, Grok, or any AI CLI in Anide's terminal. Every file the agent touches surfaces as a Git diff. You review, decide, and commit what's right. Nothing auto-commits.

---

## How it works

1. **Plan &amp; describe** - open a terminal in Anide, tell the AI what to build
2. **AI executes** - the agent writes code, edits files, runs commands
3. **Diff appears** - every changed file surfaces in Git with a full diff
4. **Test &amp; review** - use Anide's tools to hit the endpoint, query the DB, check logs
5. **Commit &amp; ship** - approve what's right, discard what isn't

---

## Your AI sees the whole stack

Most AI coding tools only see the open file. Anide is designed to give your agent live context from every layer of your stack via MCP.

Git and Docker ship today. MCP shipping soon. The remaining providers are on the roadmap - built as Anide grows:


| Provider                   | What the agent can see                      |
| -------------------------- | ------------------------------------------- |
| Git                        | current diff, branch, commit history        |
| Docker                     | running containers, logs, port mappings     |
| Database *(planned)*       | live schema, table structure, query results |
| API client *(planned)*     | saved requests, response history            |
| Cache *(planned)*          | key/TTL state, pub/sub channels             |
| Object Storage *(planned)* | bucket contents, file metadata              |


The agent checks what's actually running before it writes a line. No guessing from stale docs.

---

## Review first

Every change the agent makes is a file on disk. Anide's Git tool shows you the full diff before anything is committed. You decide what ships. There is no auto-commit, no silent overwrite.

---

## Local first

- Runs entirely on your machine
- No telemetry, no analytics, no tracking
- No mandatory account or cloud sync
- Credentials never leave your disk and they are stored in `.env` at your repo .gitignore -d

```text
your-project/
├── .anide/
│   ├── README.md          ← project notes, editable in-app
│   └── ...
├── .env
└── ...
```

---

## Built on Tauri

- **&lt; 1s startup** - native OS WebView, no bundled Chromium
- **~8.3 MB binary** - dramatically smaller than Electron alternatives
- **&lt; 30MB RAM usage** - Uses less than 30 mb of RAM while running
- **Rust backend** - memory-safe, no GC pauses
- **Stack:** Svelte 5, SvelteKit 2, Tailwind CSS 4, Tauri 2, CodeMirror 6, Bun

---

## Also replaces

Anide's tools are primarily MCP context providers for your agent. But you can drive them yourself too — without switching apps.

What is there today:

- **Git** — stage, diff, commit, branch, push, pull, history
- **Docker** — start/stop containers, stream logs, exec in, compose up/down

What's coming:

- **Database** — query, browse schema, edit rows (replaces DBeaver, TablePlus)
- **REST client** — send requests, manage collections (replaces Postman, Insomnia)
- **Cache** — browse keys, debug pub/sub (replaces Redis Insight)
- **Object Storage** — browse buckets, upload, download (replaces MinIO Browser, Cyberduck)

---

## Roadmap

### REST Client

Replaces: Postman, Insomnia, Bruno

- [ ] REST, GraphQL, gRPC, WebSocket, SSE requests
- [ ] Auth: Bearer/JWT, API Key, Basic, OAuth 2.0, mTLS
- [x] Environment variables pulled from your `.env` files automatically
- [x] Collections committed to Git alongside your code
- [ ] Import from Postman JSON, OpenAPI, curl

### Database

Replaces: DBeaver, TablePlus, MongoDB Compass

- [ ] Connect to PostgreSQL, MySQL/MariaDB, SQLite, MongoDB
- [x] Write and run queries with results table
- [x] Explore schema and table structure
- [x] Edit rows directly
- [ ] Track and compare migrations across environments

### KV / Cache

Replaces: Redis Insight, Another Redis Desktop Manager

- [ ] Connect to Redis, Valkey, KeyDB, Dragonfly
- [ ] Browse and inspect keys with TTL visualization
- [ ] Debug pub/sub channels
- [ ] Run CLI commands inline
- [ ] Flush keys during dev without leaving the app

### Object Storage

Replaces: MinIO Browser, Cyberduck, AWS S3 Console

- [ ] Connect to AWS S3, Cloudflare R2, MinIO, Backblaze B2, DigitalOcean Spaces
- [ ] Browse, upload, and download files
- [ ] Preview images and files inline
- [ ] Generate presigned URLs
- [ ] Manage bucket policies

### Git

Replaces: GitHub Desktop, GitKraken, Fork

- [x] Stage files and commit
- [x] View diffs before committing
- [x] Create, switch, and manage branches
- [x] Push, pull, fetch
- [x] View commit history
- [x] Resolve merge conflicts

### Docker

Replaces: Docker Desktop, Lazydocker

- [x] Start, stop, restart containers
- [x] Stream container logs live
- [x] `docker-compose up/down`
- [ ] Browse images and volumes
- [ ] Check port mappings
- [x] Exec into a running container

---

## Dev

```bash
bun install
bun run tauri dev
```

Build:

```bash
bun run tauri build
```

---

## License

Apache 2.0 — see [./LICENSE](./LICENSE).

---

## Contributing

Issues are welcome. Pull requests are restricted to collaborators - this is a solo project with a focused vision.

---

> [anide.app](https://anide.app)