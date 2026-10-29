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
| `rada-client.exe` | CLI: build/sync the index, search, live lookups, snapshots (create/restore/fetch), annotations, an offline `stats` report, the local web UI. |

> Status: **v0.9.0** — GUI, CLI, web UI, snapshot release automation all
> built, tested and verified against the live API. Releases:
> [latest](https://github.com/shevchukdp/rada_client/releases)
> + monthly index snapshots. Roadmap: [PLAN.md](PLAN.md).

## Features (implemented)

- **Native GUI (egui 0.29)** — 17 color themes (classical/white/black/invert/auto/
  matrix/opencode/nord/dracula/solarized/gruvbox/monokai/one-dark/tokyo-night/
  catppuccin/github-dark, header picker, persisted) — the default «Класична» is a
  light classic Windows theme (gray face, white panels, Windows-blue accent),
  Ukrainian labels:
  - **Пошук** — local FTS5 search with filters (status/type/date/number/publisher;
    the chosen status/type/publisher + sort are remembered across launches),
    AND-narrowing with last-word prefix matching (automatic OR fallback),
    a **date-range calendar** (Ukrainian months, dates beyond the freshest data
    fetched from rada are disabled) with **quick presets** («7 днів / місяць /
    рік» set «Дата з» relative to the freshest fetched data),
    **clickable column sorting**
    (Назва ▲▼ / Дата ▲▼ / Редакція ▼ / Релевантність — the choice is
     remembered across launches),
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
    (`GET /api/related/{nreg}`) with a «повторити» retry on failure, an empty
    notice, a collapse-on-second-click and a reset on document change.
    CLI: `related <nreg> [--json] [--out FILE]`.
  - **Mouse-selection highlights** — the «Виділення мишею» chip in the reader
    makes the text selectable by mouse; «виділити» saves the selection as a
    gold highlight annotation (double-click selects a word/paragraph).
  - **Query history** — «недавні запити» chips under the search filters repeat
    a past search with one click (persisted, clearable); «копіювати текст» in
    the reader copies the whole act to the clipboard, and «цилата» copies the
    full reference — «Назва // Офіційний вебпортал парламенту України. Редакція
    від дд.мм.рррр, URL: …» (same format as the web button). Each search hit row has
    «копія» (copy nreg) and «сайт» (open the act on zakon.rada.gov.ua), and
    **right-click opens a context menu** (open reader / open site / copy
    nreg / copy title / mark for file export).
  - **New-arrival badges** — the arrivals feed is diffed against what you have
    seen: fresh docs are gold, with a «нових» counter and a «лише нові» filter
    (the seen list persists across sessions); the text filter is remembered
    too, and the shown list exports to **CSV** (UTF-8 BOM, Excel-friendly).
  - **Search facets** — after each search, chip rows «Статус/Тип/Видавник» show
    per-option result counts («Чинний (36)」); a click sets that filter and
    re-runs the search, a second click clears it.
  - **Offline-readiness badge** — search rows whose act text is already in the
    local text cache carry a «· кеш» mark on the meta line, so you know which
    hits open without the network.
  - **Reading time** — the reader toolbar reports words, characters and an
    estimated reading time (≈400 wpm), e.g. «43 733 слів · 340 212 симв. ·
    ~110 хв».
  - **Grouped favorites** — the Обране page groups annotations per document
    (gold «Title (count)» header with an «відкрити» button), with a text/title/nreg
    filter box, kind chips **with live counts** («усі (9) · коментарі (2) …»),
    a «групувати» toggle, sort chips
    («новіші / старіші / за назвою») and **collapsible groups** (click a
    header to hide its rows; «розгорнути всі» restores them).
  - **Keyboard shortcuts** — Ctrl+F focuses find/query; Alt+←/→ **or**
    Alt+PageUp/PageDown step through the results list inside the reader;
    **Ctrl+Plus / Ctrl+Minus / Ctrl+0** zoom the reader text (±10%, reset).
  - **Cache full-text search («Знайти в кеші»)** — offline scan of every cached
    document body (Unicode case-insensitive, ranked, context snippets); rows
    open in the reader. Also `--cache-search TERM` on launch, CLI
    `cache --search TERM`, and web «Пошук у тексті». The home page shows the
    cache size («кеш текстів») with an «очистити» button (confirm dialog).
  - **Reader outline («Зміст»)** — a collapsible panel listing every
    «Розділ/Глава/Стаття» heading; one click scrolls straight to it (shown for
    acts with at least two headings, e.g. 604 entries in the Criminal Code).
  - **Reader «Догори» button** — deep in a long act a floating button appears
    at the bottom-left with the scroll progress («Догори · 47%»); one click
    returns to the title (web «догори» parity).
  - **Plain-text mode** — «Форматований/Простий» toggle (GUI) and «Простий»
    (web) switch off structural styling of titles/sections/articles.
  - **Printing** — «Друк» in the reader (and «🖨 Друк» in the web UI) opens a
    print-ready HTML view in the browser (web: auto-opens the print dialog).
  - **About dialog («Про програму»)** — version, index path/size/docs, freshest
    fetched data, dictionaries, live-request counter, API limits and links;
    the web footer shows the same version (`version` in `/api/status`).
  - **Keyboard shortcuts** — **Ctrl+F** focuses the find-in-text field (reader),
    the query field (search) or the text filter (arrivals/favorites);
    **Ctrl+K** jumps to Пошук with the query
    focused; **Ctrl+B** bookmarks the open act; **Ctrl+M** toggles the reader's
    форматований/простий text mode; **Ctrl+Enter** opens the top search hit (or
    the top visible arrival) in the reader; **F5** re-fetches the reader text; **Esc** clears the find query;
    **Alt+←/→**
    pages through the results list from inside the reader; **Alt+↑/↓** cycles
    the reader's edition combo (поточна ↔ dated, wrap-around); **Shift+Enter** steps
    to the previous find match; **Ctrl+Plus / Ctrl+Minus / Ctrl+0** zoom the
    reader text (±10%, persisted); **Alt+1…Alt+5** switch pages
    (Головна / Пошук / Читалка / Оновлення / Обране); a topbar **«клавіші»
    button** opens the whole
    list (same source as the About dialog's shortcut table).
  - **Favorites (Обране)** — every saved annotation across all documents on one
    page: kind filters, one-click open, «цилата» (copy the full act reference),
    «копія» (copy nreg), «сайт» (open on zakon.rada.gov.ua), «експорт .txt»
    (exactly the filtered/sorted view, web parity), delete; the filter, kind
    chip, sort and grouping choices persist across restarts; bare bookmarks show
    an italic context snippet from the offline text cache (GUI and web).
  - **Bandwidth-friendly** — repeated dictionary / arrivals-feed fetches use
    conditional HTTP (`ETag`/`If-None-Match`, `Last-Modified`/`If-Modified-Since`);
    the server's 304 means the body is never re-downloaded (≈200 MB/day limit).
  - **Live pacing control** — «−»/«+» buttons in the top bar adjust the
    request pause 1–60 s at runtime; the choice persists (CLI `--pace` wins).
    Save/open dialogs remember the last used folder.
  - **Читалка** — live JSON card + clean text with an **edition switcher**, an
    **edition comparison** («Порівняти редакції» — word/char/reading-time per
    edition plus the signed «різниця: ±N слів», exportable to .txt via
    «експорт звіту») and
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
    page or CLI `annot export/import`, with an «Перевірка імпорту» preflight
    report before anything is written), saved "highlight" annotations
    **painted gold in the document text**, and a "» текст" button that
    scrolls to the passage.
  - **Оновлення** — the live recently-updated feed (`r.tsv`), rows open the
    reader, with a «оновлено: щойно / N хв тому / N год тому / N дн тому»
    freshness label beside the document counter.
  - **Головна** — index status (documents, size, dictionaries) with a **quick
    dictionary search** (substring over stan/typs/organs; clicking a match
    applies that status/type/publisher filter and searches) and the six **recently opened documents** (chips with a relative open time: «щойно /
    N хв тому / N год тому / N дн тому»),
    one-click **live sync**, **snapshot download** with progress, **full
    index build** from the official `doc.txt` dump (or a local dump file),
    and local **backup / restore** of the index to a `.sqlite.gz` file
    (annotations preserved both ways) — all traced in a timestamped
    «Журнал дій» (UTC `[HH:MM]` per line) with an «очистити» button.
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
  reader, find-in-text **with an «N з M» match counter** («нічого не знайдено»
  on zero hits), annotations with **inline editing**
  (`PUT /api/annotations`), export (txt/md/html), keyboard shortcuts,
  recent documents, auto-loaded «Нові документи» (offline-capable), **number
  search** (an nreg in the query opens the document), a **Favorites (Обране) tab**
  (all annotations, filters, delete/edit, per-row «цилата» copy-reference,
  JSON export/import as .json + filtered .txt export), a **«Дані» panel**
  to run live sync, snapshot fetch and **index backup/restore as background jobs**
  (`POST /api/jobs/sync|snapshot|backup|restore|init`, `GET /api/jobs`, `GET /api/backups`),
  each running job showing its **live progress line** on the 2 s poll,
  including a **full index rebuild from the official dump** as a background job,
  **offline full-text search over cached bodies** («Пошук у тексті»,
  `GET /api/cache/search?q=`, with a «× звичайний пошук» cache-mode banner and
  query-word highlighting in hit titles and snippets),
  **offline-resilient reader** (a failed live body fetch falls back to the
  cached copy with a «кешовано» hint, `GET /api/cached?nreg=`; successful
  reads refill the cache), **date-range filters** (`date_from`/`date_to`,
  validated 8-digit YYYYMMDD, never blocking),
  **dict facets** (the status/type/publisher dropdowns show per-option result
  counts after each search), **theme-colored printing** (the print view keeps
  the current theme's accent on white paper, under a formal masthead — title,
  «реєстр. № …», edition date, URL and print date),
  **deep links** (`?q=&sort=&status=…` reproduces a search on load,
  `?doc=<nreg>` opens a document directly; the URL also carries a
  copy-friendly `#doc=<nreg>` / `#q=<term>` fragment kept in sync without
  adding browser-history entries), a floating **«догори» button** in the reader
  (appears past 800 px of scroll, smooth-scrolls back to the top),
  **reader doc-stats** (a muted «N слів · N симв. · ~N хв» line under the
  title, the desktop app's own counting rules, hidden until the text loads),
  **web reader outline («Зміст»** —
  smooth-scroll jump list of «Розділ/Глава/Стаття» headings, collapsible with a
  persisted «— згорнуто» choice), each outline heading carrying its own
  **«копія» chip** (copies that «Стаття/Розділ/Глава» heading to the clipboard
  with the shared «скопійовано» flash; the chip label never enters the
  clipboard or the find highlights, and it's skipped in docs with over 400
  headings),
  **reader copy-text + reading-time stats** (clipboard button and a muted
  «N слів · M симв. · ~K хв» label),
  **reader export buttons** (`.md` / `.txt` / `.html` download the open act
  client-side in the CLI `export` formats, reusing the loaded body and
  sanitizing the filename),
  **favorites .txt export** (downloads the currently filtered/sorted
  annotation list — «what you see is what you get»),
  **a «клавіші» help popover** in the header (every real shortcut listed from
  one source of truth; closes on Esc or an outside click),
  **active-filter chips** («Активні фільтри»: one chip per set filter with a ×
  that clears it and re-searches, plus «скинути все»),
  **a helpful zero-results state** («нічого не знайдено» + «скинути фільтри» +
  «шукати в кеші» + per-filter clear chips),
  **status quick chips** (up to three frequent statuses from the dictionary +
  «Усі», built from the status select itself; clicking re-searches, and the
  active chip stays in sync with deep links and the dropdown),
  **arrivals feed with «НОВЕ» badges** (viewed nregs remembered in
  localStorage, «нових: N» + «лише нові» + text filter + an «автооновлення»
  chip that refreshes the feed once a minute while the tab is open + an
  «оновлено: щойно / N хв тому» freshness label) grouped under
  collapsible **date headers** («дд.мм.рррр», dateless docs last under «без дати»),
  with an **«експорт .csv»** button (the visible list only, UTF-8 with BOM,
  the desktop app's own CSV format),
  **edition comparison** («Порівняти редакції» — word/char/time stats per
  edition plus «різниця: ±N слів»),
  **edition quick-nav** («попередня»/«наступна» buttons cycle the editions
  list with wrap-around, hidden for a single edition and gated while fetching),
  **per-hit «копія»/«сайт» buttons** (nreg to clipboard with a «скопійовано»
  flash; the act opens on zakon.rada.gov.ua in a new tab) with **query-word
  highlighting** in titles (whole-word, Unicode-aware, find-in-text colors),
  **reader prev/next stepping** («попередній / наступний» through the last
  results list with a «3 / 20» counter, Alt+Left/Right),
  and a **«цилата» button** in the reader toolbar (copies «Title // Офіційний
  вебпортал парламенту України. Редакція від дд.мм.рррр, URL: …»),
  **recent query chips** («недавні запити:» from browser storage, click to
  re-run, «очистити»),
  and a **Favorites page with desktop parity** (sort chips «новіші / старіші /
  за назвою», a text/title/nreg filter with «показано X з Y», and collapsible
  per-document groups, choices in localStorage),
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
.\rada-client.exe search "кодекс" --out hits.txt     # table to file (BOM); --json gives BOM-less JSON; --facets stays on stdout
.\rada-client.exe search "освіта" --facets           # + per-option counts: Статус/Тип/Видавник (top-6, dict labels)

# 2b) Offline health report: index file/size/sync, documents, annotations, dictionaries, cache.
.\rada-client.exe stats                    # one aligned line per source
.\rada-client.exe stats --json --out s.json  # machine-readable, BOM-less
.\rada-client.exe verify                   # db health: SQLite integrity, FKs, FTS consistency, duplicates; exit 1 on problems
.\rada-client.exe verify --json --out v.json  # checks + verdict, BOM-less

# 3) Live lookups (respect the 5–7 s API pacing).
.\rada-client.exe hits "податковий кодекс"      # offline result LIST (numbered, with dates/status); --format md for markdown links, --limit ≤500, --out FILE (BOM)
.\rada-client.exe card 2755-17          # JSON card with all editions
.\rada-client.exe card 2755-17 --out card.txt   # summary table to file (BOM); --json --out → BOM-less JSON; failed fetch writes no file
.\rada-client.exe text 2755-17          # clean current text (offline cache fallback)
.\rada-client.exe text 2755-17 --out zakon.txt   # write the text to a file (UTF-8)
.\rada-client.exe text 2755-17 --edition 20101202
.\rada-client.exe text 2755-17 --grep "стаття 7" --context 2   # grep the body: «N: line», «--» between blocks
.\rada-client.exe cache                 # offline text-cache stats; --out FILE writes the line (BOM)
.\rada-client.exe stats                  # offline index report (docs, dicts, cache, snapshot)
.\rada-client.exe stats --json           # machine-readable
.\rada-client.exe cache --purge 2755-17 # drop one document's cached text
.\rada-client.exe backfill --query "податковий кодекс" --limit 5   # pre-cache top hits' texts for offline reading (--dry-run lists without network)
.\rada-client.exe backfill --query "податковий кодекс" --out bf.txt  # summary report to file (BOM; progress stays on stdout); --json → BOM-less
.\rada-client.exe cache --search "освіту" --limit 10   # offline full-text over cached bodies
.\rada-client.exe cache --search "освіту" --out hits.txt  # report to file (BOM); --json → BOM-less JSON
.\rada-client.exe cache --dump C:\archiv                   # offline: write EVERY cached body to DIR as {nreg}.txt (BOM), quiet summary
.\rada-client.exe arrivals --save            # live updated-docs feed, diffed vs the saved snapshot
.\rada-client.exe arrivals --since 7         # only docs dated in the last 7 days (undated ones kept)
.\rada-client.exe resolve "податковий кодекс"   # name → nreg (offline index first, live only as fallback)
.\rada-client.exe resolve "кодекс" --limit 5 --out var.txt   # candidate table to file (BOM); --json too
.\rada-client.exe export 2755-17 --out zakon.md  # offline export of the CACHED body: md|txt|html (--fmt alias), BOM file or stdout; never touches the network
.\rada-client.exe toc 2755-17                # offline outline of the CACHED body: Розділи/Глави + indented Статті; --json / --out too
.\rada-client.exe dict                   # load dictionaries (stan/typs/organs)
.\rada-client.exe dict --kind typs
.\rada-client.exe dict --search "чинн"   # offline label filter («локально» — no refetch)
.\rada-client.exe dict --fresh           # quick freshness report («N записів» / «порожньо», no refetch)
.\rada-client.exe dict --kind stan --out stan.txt  # dictionary table to file (BOM); --json → BOM-less JSON
.\rada-client.exe dict --from-file stan.txt --kind stan   # seed a dictionary from a local dump (id\tlabel, cp1251)
.\rada-client.exe dict-stats             # offline population report: стани / типи / органи + total; --json / --out too
.\rada-client.exe dates                  # offline year histogram of adoption dates («без дати» + «найновіший рік»); --json / --out too
.\rada-client.exe orgs                   # offline top publishers by document count + «всього документів»; --limit ≤200, --json / --out too
.\rada-client.exe statuses               # offline status distribution (dict labels) + «всього документів»; --json / --out too
.\rada-client.exe months                 # offline month histogram of one year (--year РРРР, always 12 rows + «разом»); --json / --out too

# 4) Keep the index current.
.\rada-client.exe sync                   # dictionaries + updated-documents feed

#    Annotations live in the index (same store the GUI/web use).
.\rada-client.exe annot list                         # every saved annotation
.\rada-client.exe annot export --out annots.json      # all annotations as JSON (BOM-less)
.\rada-client.exe annot list 2041-15 --json
.\rada-client.exe annot list --out annots.txt    # table to file (BOM); --json → BOM-less JSON array
.\rada-client.exe annot search "податок"      # grep ALL annotations: text, kind and nreg; --json / --out too
.\rada-client.exe recent                     # newest annotations across all docs (title/kind/time/text); --limit N, --json / --out too
.\rada-client.exe favorites                  # ALL annotations grouped by nreg (titles resolved); --kind comment|highlight|bookmark (+коментарі/виділення/закладки), --sort nreg|title|new, --json / --out too
.\rada-client.exe annot add 2041-15 highlight "текст виділення"
.\rada-client.exe annot add 2341-14 comment "увага на розділ 3"
.\rada-client.exe annot add 2341-14 comment --text-file note.txt   # long UTF-8 comment from a file
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
