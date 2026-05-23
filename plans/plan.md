# Anide Feature Roadmap

Anide consolidates the tools every developer keeps open alongside their editor into one lightweight
native app. This folder contains design specs for the four major features planned after the core
Git, Docker, Terminal, and Env tools shipped.

## Features

| Feature | Spec | Replaces | Storage |
|---|---|---|---|
| REST Client | [api.md](api.md) | Postman, Insomnia, Bruno | `.anide/requests/` |
| Database Browser | [db.md](db.md) | DBeaver, TablePlus | `.anide/database/` |
| Cache / KV | [kv.md](kv.md) | Redis Insight, Another Redis | `.anide/kv/` |
| Object Storage | [s3.md](s3.md) | S3 Console, Cyberduck | `.anide/s3/` |

All connection definitions and request collections live inside `.anide/` so they are committed
to the repo and shared with the team. Secrets stay in `.env` files (gitignored); only the
template references live in version control.

---

## Template Variable System

The same `{{...}}` syntax works everywhere — URL bars, connection strings, headers, body fields,
query parameters. Templates are resolved at runtime just before a request is sent or a connection
is opened.

### Syntax reference

| Pattern | Resolves to |
|---|---|
| `{{VAR_NAME}}` | Value of `VAR_NAME` from the active env file set |
| `{{env.VAR_NAME}}` | Same — explicit env namespace |
| `{{env.production.API_KEY}}` | `API_KEY` from `.env.production` specifically |
| `{{env..env.local.SECRET}}` | `SECRET` from `.env.local` specifically |
| `{{Faker.internet.email}}` | Random email address (new value each run) |
| `{{Faker.datatype.uuid}}` | Random UUID v4 |
| `{{Faker.name.fullName}}` | Random full name |
| `{{Faker.number.int({"min":1,"max":100})}}` | Random integer with bounds |
| `{{Faker.date.recent}}` | Recent ISO 8601 date string |
| `{{Faker.lorem.sentence}}` | Random sentence |

### Resolution order for bare `{{VAR}}`

1. `.env.local`
2. `.env`
3. Other `.env.*` files sorted alphabetically

The first file that defines the key wins. If no file defines it, the template is left as-is
and a warning is shown in the UI.

### Faker categories

All resolution happens in the frontend (no Rust needed). Uses the `@faker-js/faker` package.

| Namespace | Methods |
|---|---|
| `internet` | `email`, `url`, `ip`, `userAgent`, `password`, `domainName` |
| `person` / `name` | `fullName`, `firstName`, `lastName` |
| `datatype` / `string` | `uuid`, `alphaNumeric` |
| `number` | `int`, `float` |
| `date` | `past`, `future`, `recent`, `birthdate` |
| `lorem` | `word`, `words`, `sentence`, `paragraph` |
| `phone` | `number` |
| `color` | `rgb`, `hsl`, `human` |

### UI behaviour

- Inputs with template strings show a preview chip on focus — the resolved value rendered inline.
- Unresolved variables are highlighted red.
- A "Variable Inspector" panel lists every `{{...}}` token in the current request or connection,
  its source, and its resolved value.
- Faker values regenerate on each send/connect (not on each keystroke).

---

## Shared storage conventions

Every tool stores its config files as Markdown with YAML frontmatter. This keeps them
human-readable, diffable, and editable in any editor.

```
.anide/
  requests/                    # REST client collections
    users/
      get-users.md
      create-user.md
    auth/
      login.md
    .history/                  # response history (gitignored)

  database/                    # DB connections — one folder per connection
    local-postgres/
      config.md                # connection details (host, port, creds via {{env.*}})
      queries/
        get-active-users.md    # saved SQL queries (name + description + SQL body)
        monthly-revenue.md
    prod-mysql/
      config.md
      queries/
        find-orders.md

  kv/                          # Redis/Valkey connections — one folder per connection
    local-redis/
      config.md
      queries/
        flush-sessions.md      # saved: scan patterns, Lua scripts, command sequences
        atomic-counter.md
    staging-valkey/
      config.md

  s3/                          # Object storage connections — one folder per connection
    aws-production/
      config.md                # no queries folder (S3 has no query concept)
    minio-dev/
      config.md
```

---

## Implementation order

1. **REST Client** — backend skeleton (`api.rs`) already exists; needs HTTP execution +
   template resolution + frontend tab.
2. **Database** — new Rust module, `sqlx` dep, new frontend tab.
3. **Cache/KV** — new Rust module, `redis` dep, new frontend tab.
4. **Object Storage** — new Rust module, `aws-sdk-s3` dep, new frontend tab.
