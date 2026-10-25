# rada-client — Work Plan & Design Record

This document records the **verified findings**, the **architecture
decisions**, the **data-distribution model**, and the **phase roadmap**
for the Ukrainian-legislation desktop client. It is the single source of
truth for "what we know for sure" vs. "what we still need to build."

Everything marked **verified** was confirmed by a live probe or a local
build on 2026-10-26.

---

## 1. Goal & constraints

Build a **portable Windows desktop client** for browsing and working with
the Ukrainian legislation database (`zakon.rada.gov.ua` /
`data.rada.gov.ua`):

- **Rust**, single portable `.exe`, **minimal runtime dependencies**
  (no .NET SDK, no Python required on the end-user laptop).
- **Full local index** of ~295 000 document cards with FTS5 so title
  search is instant and works offline.
- **Document body text fetched on demand** (not stored locally).
- Search by requisites, reader with editions, live arrivals, bookmarks,
  highlights, comments, and export (Markdown / HTML / TXT / bulk).

**Chosen distribution of data:** the `.exe` auto-downloads the official
`doc.txt` dump on first run (and/or a gzip snapshot from a GitHub Release /
LFS), then does cheap incremental syncs. Data is a **separate, swappable
module**, never baked into the binary.

---

## 2. Verified environment

| Item | Value | Verified |
|---|---|---|
| OS | Windows (win32) | — |
| Rust | 1.99, target `x86_64-pc-windows-gnu` | — |
| C toolchain | **w64devkit 2.10.1 (GCC 16.2.0)** at `C:\rada\toolchain\w64devkit` | build |
| `dlltool` fix | 64-bit `dlltool` from w64devkit on `PATH` (the stock 32-bit one can't emit 64-bit import libs) | build |
| `libgcc` fix | `libgcc.a` → `lib\libgcc_eh.a` (rustc passes `-lgcc_eh`) | build |
| `rusqlite` (bundled) | compiles SQLite C + FTS5 | build |
| `reqwest` + schannel TLS | compiles & runs | build + live |
| `git` | **not on `PATH`**; at `C:\Program Files\Git\cmd\git.exe` | — |

> Any `cargo` invocation **must** have
> `C:\rada\toolchain\w64devkit\w64devkit\bin` first on `PATH`
> (see `C:\rada\build.ps1`).

---

## 3. Verified API facts (source of truth: `data.rada.gov.ua`)

- **HTTPS-only** host (port 80 → 301 to https).
- **Anonymous `User-Agent` must be exactly `OpenData`** (case-sensitive)
  for the JSON/TXT endpoints. Browser UA, `OpenData/1.0`, lowercase → `403`.
- The server sends **gzip even without** `Accept-Encoding`.
- **Limits:** ≤ 60 req/min (5–7 s pause recommended), ≤ 100 000 req/day,
  ≤ 200 MB/day. **Never** poll `/api/token` or `/api/limits` per request
  (IP gets blocked).
- `/api/token` returns a daily UUID, but *using* it requires pre-registering
  the IP → **do not use it**; stay on `OpenData`.

### Endpoints (all live-verified)

| Endpoint | Purpose | Encoding |
|---|---|---|
| `GET /laws/card/{nreg}.json` | JSON card incl. edition list (`eds`) | UTF-8 |
| `GET /laws/show/{nreg}` | Full HTML of current edition | UTF-8 |
| `GET /laws/show/{nreg}.txt` | **Clean plain text** of current edition | UTF-8 |
| `GET /laws/show/{nreg}/ed{YYYYMMDD}` | HTML of a specific edition | UTF-8 |
| `GET /laws/show/{nreg}/ed{YYYYMMDD}.txt` | Clean text of a specific edition | UTF-8 |
| `GET /laws/main/r.tsv` | Recently-updated feed: `num card nazva status publics link size file` | UTF-8 |
| `GET /ogd/zak/laws/data/csv/doc.txt` | **Full dump**, 9-col TSV | **Windows-1251** |
| `GET /ogd/zak/laws/data/csv/{stan,typs,organs}.txt` | Dictionaries `id<TAB>label` | **Windows-1251** |
| `GET /laws/find/a?text=...` | name → nreg resolver (302 or HTML list) | — |

### `nreg` URL encoding

Percent-encode **each `/`-segment separately, keep the slashes**:
`254к/96-вр` → `254%D0%BA/96-%D0%B2%D1%80`. Encoding the slash as `%2F`
yields 404. (Implemented + unit-tested in `rada-core::api`.)

### `doc.txt` row shape (9 columns)

`dokid  nreg  nazva  status  types  organs  minjust  npix  poddat`
- `organs` = `orgid:orgdat:orgnum` (multiple via `|`).
- `nazva` is the only free-text column; the trailing 6 fixed columns are
  parsed from the right (robust to a stray tab in the title).

### Status dictionary (`stan.txt`)

| id | label |
|---|---|
| 0 | Не визначено |
| 1 | Втратив чинність |
| 2 | Набирає чинності |
| 3 | Дію зупинено |
| 4 | Дію відновлено |
| 5 | Чинний |
| 6 | Не набрав чинності |
| 7 | Не застосовується |
| 8 | Частково набрав чинності |
| 9 | Втратив чинність крім окремих |

Distribution in the 2026-10 dump: status 5 ≈ 122 522, status 0 ≈ 126 007,
status 1 ≈ 45 875, rest negligible.

### Known pitfalls (from reference repos)

- **"Archive twin"** codes have status «Чинний» **plus** a title suffix
  `(редакції до ...)` and can contain different figures → filter them out.
- A code's `Редакція від` date may be in the future (not yet in force).
- No in-document full-text-search endpoint exists; search titles via the
  local index, drill in by article.

---

## 4. Measured numbers

| Metric | Value |
|---|---|
| `doc.txt` size | 51.7 MB, 295 340 rows, parse ~1 s |
| Full index + FTS5 on disk | 187–248 MB raw |
| Full index **gzip** | **~43 MB** (fits GitHub's 100 MB/file limit) |
| `rusqlite` + `reqwest` + friends | all compile on the patched toolchain |
| FTS5 title search | ~2–35 ms |
| `LIKE` substring search | ~250–350 ms |
| Full import (295 340 rows) | ~106 s (release) |

**GitHub limits that shape distribution:** 100 MB per-file hard limit;
Git LFS free quota ≈ 1 GB; Releases can hold multi-GB assets.

---

## 5. Data distribution model (decided)

The **index is a generated artifact** of the canonical raw source
(`doc.txt` + `doc-update.txt` + `r.tsv`). We distribute it as data, not as
code:

1. **Auto-download (default).** The exe pulls the official `doc.txt`
   (51 MB) from `data.rada.gov.ua` on first run and builds the index
   locally. The exe stays small (~a few MB); the data is always current.
2. **Gzip snapshot in a GitHub Release (monthly).** A prebuilt
   `zakon-index-YYYY-MM.sqlite.gz` (~43 MB) attached to a monthly release.
   The app can download the latest snapshot for an instant, versioned,
   offline-capable first run.
3. **Git LFS in the repo (monthly commits).** The same `.sqlite.gz`
   committed via LFS to a `data/` path, tagged monthly — for users who
   clone the repo and want the data without a network fetch.
4. **Modular DB.** `rada-db` is a standalone crate, so the index can be
   built, swapped, or distributed independently of the client.

**Incremental sync** thereafter uses `/laws/main/r.tsv` (a small list of
changed docs) + per-document card refresh, respecting the 60 req/min limit.

> The raw 187–248 MB index **cannot** go in plain git (>100 MB). The
> ~43 MB **gzip** is the distributable unit for both Release and LFS.

---

## 6. Architecture

```
crates/rada-core   domain types + data.rada.gov.ua API client (no DB)
crates/rada-db     SQLite index module (schema, import, FTS search,
                   dictionaries, incremental sync, gzip snapshots)
crates/rada-cli    `rada-client` binary (init/sync/search/card/text/
                   snapshot/status)
```

`reqwest` is the **only** network stack (blocking, gzip, schannel TLS).
`rusqlite` is **bundled** (static SQLite, no external `.dll`).

### `rada-db` schema

```
docs(dokid PK, nreg UNIQUE, nazva, status, types, org_id, org_date,
     org_num, organs, minjust, npix, poddat, fetched_at)
  + indexes on nazva, status, org_id, org_date, poddat
docs_fts  FTS5(nazva, content=docs, content_rowid=rowid)  + sync triggers
dict(kind, id, label)          -- stan / typs / organs
meta(key, value)               -- schema_version, last_import
```

Search strategy: **multi-word → FTS5 MATCH (ranked); single word → LIKE
substring**; plus optional filters (org / type / status / date range /
number). `LIMIT/OFFSET` pagination. (A derived-`Default` `limit: 0` bug was
caught by tests and fixed to a manual `Default` with `limit: 50`.)

---

## 7. Status (as of this commit)

- **Done & verified:** modular workspace builds **green**; **15/15 tests
  pass**; full index built from the real dump; FTS search works; **live
  `card` and `text` calls succeed** against `data.rada.gov.ua`; gzip
  snapshot module round-trips.
- **Next:** the **local web UI** (Rust HTTP server + embedded HTML/JS) —
  search form, document reader with edition switcher, arrivals, bookmarks,
  highlights/comments, Markdown/HTML/TXT export, bulk export. Then
  packaging (icon, `README`, release/LFS snapshot automation) and the
  first push to `github.com/shevchukdp/rada_client`.

---

## 8. References (design sources)

Saved locally under `C:\rada\references\`:

- `PavloGor/makedown-get-htm` — full `api.md` documentation of the
  `/laws/*` endpoints.
- `LEXERY-AI/lexery-ukrainian-law` — `nreg` URL grammar, validity/editions
  semantics, "archive twin" pitfalls.
- `vladzsh/rada-client` — Python reference client: token, pacing, status
  mapping.
- `Dogon86/Zakon.rada` — data model (Law / Bookmark / Note entities).
- `skivol/ukrainian-laws-in-time-generator` — edition-sync approach.

Official open-data portal & API: `https://data.rada.gov.ua/open/main/api`.
