# rada-client

Portable desktop client for browsing and working with the Ukrainian
legislation database **«Законодавство України»** (`zakon.rada.gov.ua`).

Two static Windows `.exe` files, zero end-user dependencies (no .NET, no
Python). The client keeps a **local, full-text-searchable SQLite index** of
~295 000 document cards and fetches document bodies on demand from the
official open-data API:

| Executable | Role |
|---|---|
| `rada-client-gui.exe` | **The app.** One egui window: full-text search, document reader with structured act rendering (live card/text/editions), annotations & favorites, live arrivals feed, 17 color themes, one-click snapshot download, live sync and local backup/restore. |
| `rada-client.exe` | CLI: build/sync the index, search, live lookups, snapshots (create/restore/fetch), annotations, the local web UI. |

> Status: **v0.9.0** — GUI, CLI, web UI, snapshot release automation all
> built, tested and verified against the live API. Releases:
> [latest](https://github.com/shevchukdp/rada_client/releases)
> + monthly index snapshots. Roadmap: [PLAN.md](PLAN.md).

## Features (implemented)

- **Native GUI (egui 0.29)** — 17 color themes (classical/white/black/invert/auto/
  matrix/opencode/nord/dracula/solarized/gruvbox/monokai/one-dark/tokyo-night/
  catppuccin/github-dark, header picker, persisted), Ukrainian labels:
  - **Пошук** — local FTS5 search with filters (status/type/date/number/publisher),
    AND-narrowing with last-word prefix matching (automatic OR fallback),
    a **date-range calendar** (Ukrainian months, dates beyond the freshest data
    fetched from rada are disabled), **clickable column sorting**
    (Назва ▲▼ / Дата ▲▼ / Редакція ▼ / Релевантність),
    paging, live name resolver; a **«Розширений пошук на порталі»** button that
    opens the zakon.rada.gov.ua search with the current query in the browser;
    **export of the results list** (.md/.txt);
    **checkbox-select results → individual numbered files** in a chosen folder;
    one-click **copy nreg** from any result row.
  - **Offline number search** — typing an nreg (`1556-18`, `1178-2022-п`,
    `n0423500-26`) in the title box opens the document directly, no network
    (GUI and web).
  - **Arrivals offline** — the last feed snapshot is stored in the index and
    replayed with a «збережене» chip when the network is down; substring
    filter by title/number/publication.
  - **Continue / recents** — the app persists the last opened documents and
    query (`%LOCALAPPDATA%\rada-client\settings.ini`); Home shows clickable
    chips of the **last opened documents**, and the search field pre-loads the
    last query.
  - **Offline text cache** — every live document read is stored per edition
    (`docs_text`, schema v3); when the network fails the reader serves the
    cached copy with a «кешовано» chip. CLI: `cache` / `cache --purge [nreg]` /
    `cache --search TERM` (offline full-text scan, Unicode case-insensitive,
    ranked by occurrences with context snippets).
  - **Related documents («Пов'язані документи»)** — the reader parses the live
    portal page for links to other acts referenced by the open one (one paced
    request), shows their offline titles, and opens any of them on click; the
    web UI has the same list behind a «Пов'язані документи» button
    (`GET /api/related/{nreg}`). CLI: `related <nreg> [--json]`.
  - **Mouse-selection highlights** — the «Виділення мишею» chip in the reader
    makes the text selectable by mouse; «виділити» saves the selection as a
    gold highlight annotation (double-click selects a word/paragraph).
  - **Query history** — «недавні запити» chips under the search filters repeat
    a past search with one click (persisted, clearable); «копіювати текст» in
    the reader copies the whole act to the clipboard.
  - **Cache full-text search («Знайти в кеші»)** — offline scan of every cached
    document body (Unicode case-insensitive, ranked, context snippets); rows
    open in the reader. Also `--cache-search TERM` on launch, CLI
    `cache --search TERM`, and web «Пошук у тексті».
  - **Plain-text mode** — «Форматований/Простий» toggle (GUI) and «Простий»
    (web) switch off structural styling of titles/sections/articles.
  - **Printing** — «Друк» in the reader (and «🖨 Друк» in the web UI) opens a
    print-ready HTML view in the browser (web: auto-opens the print dialog).
  - **About dialog («Про програму»)** — version, index path/size/docs, freshest
    fetched data, dictionaries, live-request counter, API limits and links;
    the web footer shows the same version (`version` in `/api/status`).
  - **Keyboard shortcuts** — **Ctrl+F** focuses the find-in-text field (reader)
    or the query field (search); **Esc** clears the find query; **Alt+←/→**
    pages through the results list from inside the reader; **Shift+Enter** steps
    to the previous find match.
  - **Favorites (Обране)** — every saved annotation across all documents on one
    page: kind filters, one-click open, delete; bare bookmarks show an italic
    context snippet from the offline text cache (GUI and web).
  - **Bandwidth-friendly** — repeated dictionary / arrivals-feed fetches use
    conditional HTTP (`ETag`/`If-None-Match`, `Last-Modified`/`If-Modified-Since`);
    the server's 304 means the body is never re-downloaded (≈200 MB/day limit).
  - **Live pacing control** — «−»/«+» buttons in the top bar adjust the
    request pause 1–60 s at runtime; the choice persists (CLI `--pace` wins).
    Save/open dialogs remember the last used folder.
  - **Читалка** — live JSON card + clean text with an **edition switcher** and
    **structured rendering**: the act header/name line, publication & amendments
    blocks («(Відомості …)», `{Із змінами …}`), «РОЗДІЛ/ГЛАВА» and «Стаття N»
    headings and numbered items are all styled (same classification in the web
    reader); `.md`/`.txt`/`.html` export (annotations appended; HTML keeps the
    structure as styled `<p>` blocks and inlines saved highlights as gold
    marks), **"find in text"**
    (case-insensitive, match counter, » / « jump-to-match, matches highlighted),
    "open on the site" link, and **results paging** (‹ / › — move through the
    search results list you opened a document from).
  - **Анотації** — bookmarks / comments / highlights, stored inside the
    index itself (survive snapshot restores — auto-backed-up), with **inline
    text editing** («змінити»), **portable JSON export/import** (Favorites
    page or CLI `annot export/import`), saved "highlight" annotations
    **painted gold in the document text**, and a "» текст" button that
    scrolls to the passage.
  - **Оновлення** — the live recently-updated feed (`r.tsv`), rows open the
    reader.
  - **Головна** — index status (documents, size, dictionaries),
    one-click **live sync**, **snapshot download** with progress, **full
    index build** from the official `doc.txt` dump (or a local dump file),
    and local **backup / restore** of the index to a `.sqlite.gz` file
    (annotations preserved both ways).
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
  external assets): search (with publisher filter + live name resolver),
  reader, find-in-text, annotations with **inline editing**
  (`PUT /api/annotations`), export (txt/md/html), keyboard shortcuts,
  recent documents, auto-loaded «Нові документи» (offline-capable), **number
  search** (an nreg in the query opens the document), a **Favorites (Обране) tab**
  (all annotations, filters, delete/edit, JSON export/import), a **«Дані» panel**
  to run live sync, snapshot fetch and **index backup/restore as background jobs**
  (`POST /api/jobs/sync|snapshot|backup|restore|init`, `GET /api/jobs`, `GET /api/backups`),
  including a **full index rebuild from the official dump** as a background job,
  **offline full-text search over cached bodies** («Пошук у тексті»,
  `GET /api/cache/search?q=`), **date-range filters** (`date_from`/`date_to`,
  validated 8-digit YYYYMMDD, never blocking),
  and the same 17 themes (CSS variables, localStorage).
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
.\rada-client-gui.exe --start favorites                   # start on the saved annotations page
.\rada-client-gui.exe --cache-search "освіту"             # open the search page with a cache full-text scan
```

### CLI (automation / advanced)

```powershell
# 1) Fast offline start: pull the published monthly snapshot (~70 MB, no rebuild).
.\rada-client.exe snapshot fetch

#    …or build the full index from the official dump (~51 MB, auto-downloaded).
.\rada-client.exe init
.\rada-client.exe init --dump C:\path\to\doc.txt

# 2) Search the local index (title + any filters + --sort).
.\rada-client.exe search "податковий кодекс"
.\rada-client.exe search "" --status 5 --org 1 --date_from 20240101 --limit 20 --json
.\rada-client.exe search "кодекс" --sort nazva_asc   # date_desc|date_asc|nazva_asc|nazva_desc|podd_desc|relevance

# 3) Live lookups (respect the 5–7 s API pacing).
.\rada-client.exe card 2755-17          # JSON card with all editions
.\rada-client.exe text 2755-17          # clean current text (offline cache fallback)
.\rada-client.exe text 2755-17 --edition 20101202
.\rada-client.exe cache                 # offline text-cache stats
.\rada-client.exe cache --purge 2755-17 # drop one document's cached text
.\rada-client.exe cache --search "освіту" --limit 10   # offline full-text over cached bodies
.\rada-client.exe resolve "податковий кодекс"   # name → nreg
.\rada-client.exe export 2755-17 --fmt md --out zakon.md   # cache-first export
.\rada-client.exe dict                   # load dictionaries (stan/typs/organs)
.\rada-client.exe dict --kind typs

# 4) Keep the index current.
.\rada-client.exe sync                   # dictionaries + updated-documents feed

#    Annotations live in the index (same store the GUI/web use).
.\rada-client.exe annot list                         # every saved annotation
.\rada-client.exe annot list 2041-15 --json
.\rada-client.exe annot add 2041-15 highlight "текст виділення"
.\rada-client.exe annot add 2341-14 comment "увага на розділ 3"
.\rada-client.exe annot del 4

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
