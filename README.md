# rada-client

Portable desktop client for browsing and working with the Ukrainian
legislation database **«Законодавство України»** (`zakon.rada.gov.ua`).

A single static Windows `.exe` that keeps a **local, full-text-searchable
SQLite index** of ~295 000 document cards, fetches document bodies on
demand from the official open-data API, and lets you search, read,
bookmark, annotate and export legislation.

> Status: **core + web UI are built, tested and verified against the live
> API.** Snapshot/release automation and packaging are the remaining
> phase — see [PLAN.md](PLAN.md).

## Features (implemented)

- **Full local index** of all 295 340 document cards (metadata + titles),
  built from the official `doc.txt` dump, with an **FTS5** full-text index.
- **Search by requisites**: title (FTS / substring), publisher, document
  type, status, adoption-date range, document number. Sub-millisecond to
  tens-of-ms on a laptop.
- **Live API client** for the official `data.rada.gov.ua` open-data
  endpoint (correct `User-Agent`, gzip, 5–7 s pacing, rate-limit aware).
  - Document JSON card (all editions)
  - Clean plain-text body, current or a specific edition
  - Recently-updated feed (`/laws/main/r.tsv`)
  - Reference dictionaries (status / types / publishers)
- **Incremental sync** (`sync`) to keep the index current without a
  full rebuild.
- **Gzip snapshots** (`snapshot create/restore`) for offline
  distribution / monthly publishing, plus **`snapshot fetch`** to pull
  the latest published monthly snapshot straight from the GitHub release
  (no rebuild required).
- A **local web UI** (`web`): one embedded HTML page (vanilla JS, zero
  external assets) with a search form (title + all filters + paging), a
  document reader with an **edition switcher**, the live
  recently-updated feed, **bookmarks / highlights / comments** (stored
  inside the index itself), and **TXT / Markdown / HTML export** —
  single document or the whole search page at once.
- A **modular Rust workspace**: the database is an independent crate
  (`rada-db`) that can be reused or swapped without touching the client.

## Build

Requires a Rust toolchain for `x86_64-pc-windows-gnu` with a working
64-bit `dlltool`. On this machine that is `C:\rada\toolchain\w64devkit`
(prepend its `bin` to `PATH`), then:

```powershell
cargo build --release
# → target\release\rada-client.exe  (static, ~a few MB)
```

Run the test suite (offline, no network):

```powershell
cargo test
```

## Use

The data directory holds the SQLite index (default `data/zakon-index.sqlite`).

```powershell
# 1) Build the full index from the official dump (~51 MB, auto-downloaded).
.\rada-client.exe init

#    …or from an already-downloaded dump file:
.\rada-client.exe init --dump C:\path\to\doc.txt

# 2) Search the local index (title + any filters).
.\rada-client.exe search "податковий кодекс"
.\rada-client.exe search "" --status 5 --org 1 --date_from 20240101 --limit 20 --json

# 3) Live lookups (respect the 5–7 s API pacing).
.\rada-client.exe card 2755-17          # JSON card with all editions
.\rada-client.exe text 2755-17          # clean current text
.\rada-client.exe text 2755-17 --edition 20101202

# 4) Keep the index current.
.\rada-client.exe sync                   # dictionaries + updated-documents feed

# 5) Index statistics.
.\rada-client.exe status

# 6) Snapshots for distribution.
.\rada-client.exe snapshot create data\zakon-index.sqlite.gz
.\rada-client.exe snapshot restore data\zakon-index.sqlite.gz out.sqlite

#    …or fast offline start: pull the published monthly snapshot
#    (~70 MB, no rebuild) from the GitHub release:
.\rada-client.exe snapshot fetch

# 7) Local web UI (search + reader + annotations + export).
.\rada-client.exe web --port 8742
.\rada-client.exe web --open        # also open the browser
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
crates/rada-core   domain types + data.rada.gov.ua API client
crates/rada-db     SQLite index module (schema, import, search, dict,
                   incremental sync, gzip snapshots, annotations)
crates/rada-web    local web UI (embedded page + JSON API, tiny_http)
crates/rada-cli    `rada-client` binary (init/sync/search/card/text/
                   resolve/snapshot/status/web)
```

## Attribution & terms

Legislation data © Cabinet / Verkhovna Rada of Ukraine, distributed
publicly at `data.rada.gov.ua` (open data). This client performs only
targeted, rate-limited requests to the documented open-data API.

Reference material used during design is listed in
[PLAN.md](PLAN.md#references).
