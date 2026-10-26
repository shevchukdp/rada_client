# rada-client

Portable desktop client for browsing and working with the Ukrainian
legislation database **«Законодавство України»** (`zakon.rada.gov.ua`).

Two static Windows `.exe` files, zero end-user dependencies (no .NET, no
Python). The client keeps a **local, full-text-searchable SQLite index** of
~295 000 document cards and fetches document bodies on demand from the
official open-data API:

| Executable | Role |
|---|---|
| `rada-client-gui.exe` | **The app.** One egui window: search, document reader (live card/text/editions), annotations, live arrivals feed, index status, one-click snapshot download. |
| `rada-client.exe` | CLI: build/sync the index, search, live lookups, snapshots, the local web UI. |

> Status: **v0.2.0** — GUI, CLI, web UI, snapshot release automation all
> built, tested and verified against the live API. Releases:
> [v0.2.0 (GUI)](https://github.com/shevchukdp/rada_client/releases/tag/v0.2.0)
> + monthly index snapshots. Roadmap: [PLAN.md](PLAN.md).

## Features (implemented)

- **Native GUI (egui 0.29)** — dark theme, Ukrainian labels:
  - **Пошук** — local FTS5 search with filters (status/type/date/number),
    paging, live name resolver; **export of the results list** (.md/.txt).
  - **Читалка** — live JSON card + clean text with an **edition switcher**,
    `.md`/`.txt` export, "open on the site" link.
  - **Анотації** — bookmarks / comments / highlights, stored inside the
    index itself (survive snapshot restores — auto-backed-up).
  - **Оновлення** — the live recently-updated feed (`r.tsv`), rows open the
    reader.
  - **Головна** — index status (documents, size, dictionaries),
    one-click **live sync** and **snapshot download** with progress.
  - Dictionaries are **auto-loaded on startup** when empty (3 paced
    requests), so filters work out of the box.
- **Full local index** of all 295 340 document cards (metadata + titles),
  built from the official `doc.txt` dump, with an **FTS5** full-text index.
- **Live API client** for `data.rada.gov.ua` (correct `User-Agent: OpenData`,
  gzip, paced 5 s per request, rate-limit aware): document JSON card with
  editions, clean text (current or specific edition), updated feed,
  reference dictionaries, name→nreg resolver.
- **Incremental sync** (`sync`) + **gzip snapshots** (`snapshot
  create/restore`) and **`snapshot fetch`** (pull the published monthly
  snapshot straight from the GitHub release — no rebuild).
- **Local web UI** (`web`) — one embedded HTML page (vanilla JS, zero
  external assets) with the same search/reader/annotations/export.
- **Modular Rust workspace** — the database is an independent crate
  (`rada-db`) that can be reused or swapped without touching the clients.

## Build

Requires a Rust toolchain for `x86_64-pc-windows-gnu` with a working
64-bit `dlltool`. On this machine that is `C:\rada\toolchain\w64devkit`
(prepend its `bin` to `PATH`; `windres` is needed for the exe icon), then:

```powershell
cargo build --release
# → target\release\rada-client-gui.exe (~12 MB) and rada-client.exe (~8.6 MB)
```

Run the test suite (offline, no network):

```powershell
cargo test
```

## Use

### GUI (default experience)

Double-click `rada-client-gui.exe`. The index defaults to
`.\data\zakon-index.sqlite` next to the exe; if it is missing, the home
page offers **“Отримати снапшот”** (the published monthly snapshot, ~70 MB,
downloaded + restored with progress).

Launch arguments:

```powershell
.\rada-client-gui.exe --db C:\path\to\index.sqlite --pace 5
.\rada-client-gui.exe --query "закон про вищу освіту"   # start on Search, pre-filled
.\rada-client-gui.exe --nreg 2041-15                     # open a document in the reader
.\rada-client-gui.exe --start arrivals                    # start on the feed page
```

### CLI (automation / advanced)

```powershell
# 1) Fast offline start: pull the published monthly snapshot (~70 MB, no rebuild).
.\rada-client.exe snapshot fetch

#    …or build the full index from the official dump (~51 MB, auto-downloaded).
.\rada-client.exe init
.\rada-client.exe init --dump C:\path\to\doc.txt

# 2) Search the local index (title + any filters).
.\rada-client.exe search "податковий кодекс"
.\rada-client.exe search "" --status 5 --org 1 --date_from 20240101 --limit 20 --json

# 3) Live lookups (respect the 5–7 s API pacing).
.\rada-client.exe card 2755-17          # JSON card with all editions
.\rada-client.exe text 2755-17          # clean current text
.\rada-client.exe text 2755-17 --edition 20101202
.\rada-client.exe resolve "податковий кодекс"   # name → nreg
.\rada-client.exe dict                   # load dictionaries (stan/typs/organs)
.\rada-client.exe dict --kind typs

# 4) Keep the index current.
.\rada-client.exe sync                   # dictionaries + updated-documents feed

# 5) Index statistics.
.\rada-client.exe status

# 6) Snapshots for distribution.
.\rada-client.exe snapshot create data\zakon-index.sqlite.gz
.\rada-client.exe snapshot restore data\zakon-index.sqlite.gz out.sqlite

# 7) Local web UI (search + reader + annotations + export).
.\rada-client.exe web --port 8742
.\rada-client.exe web --open            # also open the browser
# → http://127.0.0.1:8742
```

## Data & API notes (verified 2026-10-26)

- **Source of truth:** `https://data.rada.gov.ua` (HTTPS only).
- **Auth:** anonymous, but `User-Agent` must be **exactly `OpenData`**
  (case-sensitive) for the JSON/TXT endpoints; anything else → `403`.
- **Limits:** ≤ 60 req/min (5–7 s pause recommended), ≤ 100 000 req/day,
  ≤ 200 MB/day. Never poll `/api/token` or `/api/limits` per request.
- **Encodings:** `doc.txt` and dictionary files are **Windows-1251**;
  all `/laws/*` HTML/JSON responses are UTF-8.
- **Dictionary files:** the types dictionary is published as `typ.txt`
  (not `typs.txt`); the client maps the local kind `typs` → `typ.txt`.
  A missing dictionary is skipped, never fatal to a sync.
- **`nreg` URL encoding:** percent-encode each `/`-segment separately,
  keep the slashes (`254к/96-вр` → `254%D0%BA/96-%D0%B2%D1%80`).
- **Document bodies are fetched on demand** (not stored in the index).

### Measured performance

| Artifact | Size | Note |
|---|---|---|
| `doc.txt` full dump | 51.7 MB | 295 340 cards, parse ~1 s |
| SQLite index + FTS5 | ~187–248 MB raw | ~43 MB gzip |
| GitHub per-file hard limit | 100 MB | raw index needs LFS/release; gzip fits |
| FTS5 title search | ~2–35 ms | |
| `LIKE` substring search | ~250–350 ms | |

Full details, data-distribution model and roadmap: [PLAN.md](PLAN.md).

## Repository layout

```
crates/rada-core   domain types + data.rada.gov.ua API client (+ GitHub REST)
crates/rada-db     SQLite index module (schema, import, search, dict,
                   incremental sync, gzip snapshots, annotations)
crates/rada-web    local web UI (embedded page + JSON API, tiny_http)
crates/rada-cli    `rada-client` binary (init/sync/dict/search/card/text/
                   resolve/snapshot/status/web)
crates/rada-gui    `rada-client-gui` binary (egui app window)
assets/icon        exe icon source (embedded via windres build scripts)
```

## Attribution & terms

Legislation data © Cabinet / Verkhovna Rada of Ukraine, distributed
publicly at `data.rada.gov.ua` (open data). This client performs only
targeted, rate-limited requests to the documented open-data API.

Reference material used during design is listed in
[PLAN.md](PLAN.md#references).
