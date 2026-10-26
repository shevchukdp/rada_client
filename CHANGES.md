# CHANGES / ЗМІНИ

Журнал змін проєкту, двоязиково: **UKR** та **ENG**.
Bilingual change log for this project: **Ukrainian** and **English**.

---

## 2026-10-26

### Модульна архітектура (3 крата) / Modular architecture (3 crates)

- **UKR:** Створено Rust-воркспейс з трьох модулів: `rada-core` (моделі даних + клієнт API `data.rada.gov.ua`), `rada-db` (окрема SQLite-модуль індексу — схема, імпорт, FTS5-пошук, словники, інкрементальний синк, gzip-скапшоти), `rada-cli` (байнар `rada-client`). Збірка зелена, 16/16 тестів проходять.
- **ENG:** Created a Rust workspace of three modules: `rada-core` (data models + `data.rada.gov.ua` API client), `rada-db` (standalone SQLite index module — schema, import, FTS5 search, dictionaries, incremental sync, gzip snapshots), `rada-cli` (`rada-client` binary). Green build, 16/16 tests passing.

### Побудовано повний індекс / Built the full index

- **UKR:** Імпортовано офіційний дамп `doc.txt` (51.7 MB): **295 340 карток** за ~106 с. SQLite-індекс із FTS5 ≈ 187–248 MB (gzip ≈ 43 MB). Пошук назв: FTS5 ~35 мс, LIKE ~300 мс.
- **ENG:** Imported the official `doc.txt` dump (51.7 MB): **295,340 cards** in ~106 s. SQLite index with FTS5 ≈ 187–248 MB (gzip ≈ 43 MB). Title search: FTS5 ~35 ms, LIKE ~300 ms.

### Живий API перевірено / Live API verified

- **UKR:** Працюють (перевірено наживо): JSON-картка документа зі списком редакцій, чистий текст поточної/конкретної редакції, фід оновлень `r.tsv`, словники (`stan`/`typs`/`organs`), резолвер «назва → nreg» через `find/a`. Збережено обмеження API: UA саме `OpenData`, пауза 5–7 с між запитами.
- **ENG:** Verified live: document JSON card with the edition list, clean text of the current/specific edition, the updated feed `r.tsv`, dictionaries (`stan`/`typs`/`organs`), and the name→nreg resolver via `find/a`. API rules preserved: UA exactly `OpenData`, 5–7 s pause between requests.

### Виправлено баги, знайдені тестами та живими запитами / Bugs fixed (found by tests and live calls)

- **UKR:**
  1. `SearchQuery` за замовчуванням мав `limit = 0` (пошук повертав порожній список при коректному `COUNT`) — тепер ручний `Default` з `limit = 50`.
  2. У `card.json` поле `types` буває і числом, і рядком `"21|1|124"` — десеріалізатор тепер приймає обидва варіанти.
  3. Резолвер `find/a` не бачив редирект 302, бо клієнт за замовчуванням його витікав — додано окремий клієнт без редиректів та парсер nreg із `Location` (`rsplit_once`).
- **ENG:**
  1. `SearchQuery` defaulted to `limit = 0` (search returned an empty list while `COUNT` was correct) — now a manual `Default` with `limit = 50`.
  2. In `card.json`, the `types` field can be a number or a `"21|1|124"` string — the deserializer now accepts both.
  3. The `find/a` resolver couldn't see the 302 because the client followed redirects by default — added a separate no-redirect client and an nreg parser for the `Location` header (`rsplit_once`).

### Нова команда CLI: `resolve` / New CLI command: `resolve`

- **UKR:** `rada-client resolve "<назва>"` — живий пошук nreg за назвою документа (зручний для UI-шару).
- **ENG:** `rada-client resolve "<name>"` — live nreg lookup by document name (UI-layer friendly).

### Документація / Documentation

- **UKR:** Додано `README.md` (збірка, використання, факти API), `PLAN.md` (перевірені факти, модель дистрибуції даних, roadmap, джерела), `.gitignore` (target/, data/, sqlite-файли, секрети).
- **ENG:** Added `README.md` (build, usage, API facts), `PLAN.md` (verified facts, data-distribution model, roadmap, sources), `.gitignore` (target/, data/, sqlite files, secrets).

### Реліз-білд / Release build

- **UKR:** Статичний портативний `rada-client.exe` (release) ≈ **8 MB** — без зовнішніх dll.
- **ENG:** Static portable `rada-client.exe` (release) ≈ **8 MB** — no external dlls.

### Перший коміт у репозиторій / First commit to the repository

- **UKR:** Ініційовано git, перший пуш у `shevchukdp/rada_client` (main). Токен GitHub НЕ потрапляв у repo та `.git/config` (окремий одноразовий push-URL, потім видалений).
- **ENG:** Initialized git, first push to `shevchukdp/rada_client` (main). The GitHub PAT never entered the repo or `.git/config` (one-off push URL, removed afterwards).

### Спотстріл-автоматизація: LFS + GitHub Release + `snapshot fetch` / Snapshot automation: LFS + GitHub Release + `snapshot fetch`

- **UKR:** Швидкий офлайн-старт: `rada-client snapshot fetch [--name] [--to]` — завантажує опублікований snapshot із GitHub Release (≈70 MB) і відновляє індекс без побудови (в порівнянні з ~106 с імпорту дамп). Новий модуль `rada-core::github` — мінімальний публічний GitHub REST-клієнт (без авторизації). LFS: `*.sqlite.gz` ідуть через Git LFS (v3.8.1), перший спотстріл `snapshots/2026-10/zakon-index-2026-10.sqlite.gz` (70 MB) — у репо. Перший реліз «Snapshot 2026-10» з асетом. Скрипт місячної автоматизації `C:\rada\publish-snapshot.ps1` (snapshot → LFS-commit → push → release + asset).
- **ENG:** Fast offline start: `rada-client snapshot fetch [--name] [--to]` — downloads the published snapshot from a GitHub Release (~70 MB) and restores the index without a rebuild (vs ~106 s dump import). New `rada-core::github` module — a minimal public GitHub REST client (no auth). LFS: `*.sqlite.gz` goes through Git LFS (v3.8.1); first snapshot `snapshots/2026-10/zakon-index-2026-10.sqlite.gz` (70 MB) is in the repo. First release "Snapshot 2026-10" with the asset. Monthly automation script `C:\rada\publish-snapshot.ps1` (snapshot → LFS commit → push → release + asset).

### Веб-ІНТЕРФЕЙС: `rada-web` + аноатації / Web UI: `rada-web` + annotations

- **UKR:** Новий четвертий модуль `rada-web`: локальний HTTP-сервер (`tiny_http`, без async-рантайму, без зовнішніх dll) + вбудована одна сторінка (vanilla JS, жодних зовнішніх ресурсів — exe лишається повністю портативним). Ендпоінти: `/api/status`, `/api/search` (FTS5 + усі фільтри + сторінкування), `/api/dict/{stan|typs|organs}`, `/api/doc/{nreg}/card` і `/text?edition=` (живий API з pacing 5 с/запит), `/api/arrivals` (живий фід нових документів), `/api/resolve` (назва → nreg), `/api/annotations` (збереження / підсвічування / коментарі — зберігаються в самому індексі), `/api/export/{nreg}?fmt=txt|md|html` та `/api/export/bulk` (до 50 документів одним файлом). UI: пошукова форма, читач із перемикачем редакцій, вкладка нових документів, експорт сторінки. Нова команда CLI: `rada-client web [--port 8742] [--open]`. Аноатації — окремий модуль `rada-db::annotations` (таблиця `annotations`, схема v2). Перевірено наживо: пошук/анотації/картка/текст/експорт (md і bulk) — усе 200. Тестів: 27/27 (нових — 7).
- **ENG:** New fourth module `rada-web`: a local HTTP server (`tiny_http`, no async runtime, no external dlls) + one embedded HTML page (vanilla JS, zero external assets — the exe stays fully portable). Endpoints: `/api/status`, `/api/search` (FTS5 + all filters + paging), `/api/dict/{stan|typs|organs}`, `/api/doc/{nreg}/card` and `/text?edition=` (live API, 5 s pacing per request), `/api/arrivals` (live feed of updated docs), `/api/resolve` (name → nreg), `/api/annotations` (bookmarks / highlights / comments — stored inside the index itself), `/api/export/{nreg}?fmt=txt|md|html` and `/api/export/bulk` (up to 50 docs in one file). UI: search form, reader with an edition switcher, an arrivals tab, whole-page export. New CLI command: `rada-client web [--port 8742] [--open]`. Annotations live in a standalone `rada-db::annotations` module (the `annotations` table, schema v2). Verified live: search / annotations / card / text / export (md and bulk) — all 200. Tests: 27/27 (7 new).

### Опубліковано до GitHub, об'єднано історії / Pushed to GitHub, histories merged

- **UKR:** Локальний `main` (20 файлів, 4 435 вставлень) успішно пошито до GitHub. Вхідний пуш-URL використовував одноразовий PAT; скрипт-запускник видалив себе після виконання. На remote був чужий початковий коміт (`2c0ec47`, placeholder-README) — його об'єднано командою `git merge -s ours --allow-unrelated-histories`: збережено НАШЕ дерево, обидві історії тепер в одній гілці. Додано remote `origin` без токенa (токен і далі лише в одноразових URL-ах).
- **ENG:** The local `main` (20 files, 4,435 insertions) was successfully pushed to GitHub. The inbound push URL used a one-time PAT; the launcher script self-deleted after running. The remote had a foreign initial commit (`2c0ec47`, placeholder README) — it was merged with `git merge -s ours --allow-unrelated-histories`: our tree was kept and both histories now live in one branch. A token-free `origin` remote was added (the token is still used only via one-time URLs).

### Реліз снапшоту опубліковано; `snapshot fetch` перевірено наживо; exe упаковано (іконка + версія) / Snapshot release published; `snapshot fetch` verified end-to-end; exe packaged (icon + version info)

- **UKR:** Опубліковано GitHub Release «Snapshot 2026-10» (id 424120543, тег `snapshots/2026-10`) з асетом `zakon-index-2026-10.sqlite.gz` (70 MB); завантаження асета йде на `uploads.github.com` з URL із `upload_url` (бо `api.github.com/.../assets` віддає 404), надійно через `curl.exe --data-binary`. E2E-перевірка `snapshot fetch` у чистій директорії: 69 MB за 42.6 с; `status` відновленого індексу — **295 340 документів** (~248 MB). Пакування: іконку та VERSIONINFO вбудовано в exe. Крейт `winres` на GNU-інструментарії не спрацював (ресурс потрапляє в `libresource.a`, який GNU ld ніколи не підключає — секції `.rsrc` немає). Рішення: `build.rs` сам генерує `resource.rc` (ICON + VERSIONINFO), компілює його `windres` і передає `resource.o` лінкеру через `cargo:rustc-link-arg-bins` (вільний .o завжди потрапляє в бінарник). Перевірено: `.rsrc` у секціях exe є, `FileVersionInfo` показує FileDescription / ProductName / 0.1.0, іконка рендериться з exe. Розмір release-exe: 8.59 MB. Тести: 27/27.
- **ENG:** The GitHub Release "Snapshot 2026-10" (id 424120543, tag `snapshots/2026-10`) is now published with the `zakon-index-2026-10.sqlite.gz` asset (70 MB); asset uploads must target `uploads.github.com` using the URL parsed from `upload_url` (`api.github.com/.../assets` 404s), done reliably through `curl.exe --data-binary`. End-to-end verification of `snapshot fetch` in a clean directory: 69 MB in 42.6 s; `status` of the restored index reports **295,340 documents** (~248 MB). Packaging: the exe icon and VERSIONINFO are now embedded. The `winres` crate fails on the GNU toolchain (the resource lands in `libresource.a`, which GNU ld never pulls in — no `.rsrc` section). Solution: `build.rs` generates `resource.rc` itself (ICON + VERSIONINFO), compiles it with `windres`, and hands `resource.o` to the linker via `cargo:rustc-link-arg-bins` (a loose .o is always included in the binary). Verified: `.rsrc` present in the exe sections, `FileVersionInfo` shows FileDescription / ProductName / 0.1.0, and the icon renders from the exe. Release exe size: 8.59 MB. Tests: 27/27.

### GUI: новий кратель `rada-gui` (egui) + випуск 0.2.0 / GUI: new crate `rada-gui` (egui) + release 0.2.0

- **UKR:** Новий п'ятий кратель `rada-gui` (eframe/egui 0.29, glow-бекенд, іконка з `assets/icon/rada-client-256.png`) — повний застосунок в одному вікні, темна тема (#1A2233 / золото #FFB300, UKR-підписи): «Головна» (стан індексу, «Оновити наживо», завантаження+розгортання снапшота з прогресом), «Пошук» (локальний FTS5, фільтри стан/тип/дата/номер, пагінація, Enter), «Читалка» (жива картка + текст, вибір редакції, експорт .md/.txt, «на сайті», панель анотацій — закладка/коментар/виділення, зберігаються в індексі), «Оновлення» (живий фід `r.tsv`, рядки → читалка). Архітектура: egui однопоточний — усе блокуюче (SQLite, темпований живий API, завантаження снапшота) на короткочасних воркер-потоків, `mpsc`-потовок повідомлень, генерційні лічильники скидають застарілі відповіді; темпування живих запитів спільне через `Arc<RadaClient>` (за замовч. 5 с). Додано: `rada-core::github::download_with_progress` (стрім-завантаження з прогресом) та `rada-db::annotations::all` (бакап анотацій перед розгортанням снапшота). Аргументи запуску: `--db`, `--pace`, `--start <home|search|arrivals>`, `--query «текст»`, `--nreg 2041-15`. Версія воркспейсу → **0.2.0**; release: `rada-client-gui.exe` 12.46 MB + `rada-client.exe` 8.61 MB; dist `C:\rada\dist\rada-client-0.2.0\` (zip 8.2 MB) і GitHub-реліз `v0.2.0`. Перевірено наживо на індексі 295 340 док.: пошук «закон» → 8 574 результати; читалка `--nreg 2041-15` — картка, текст, редакція, панель анотацій; «Оновлення» — 520 документів з фіду. Тести: 27/27.
- **ENG:** New fifth crate `rada-gui` (eframe/egui 0.29, glow backend, window icon from `assets/icon/rada-client-256.png`) — the whole app in one native window, dark theme (#1A2233 / gold #FFB300, UKR labels): Home (index status, live sync, snapshot download+restore with progress), Search (local FTS5, status/type/date/number filters, paging, Enter-to-run), Reader (live card + text, edition picker, .md/.txt export, "on the site" link, annotations panel — bookmark/comment/highlight, stored in the index), Arrivals (live `r.tsv` feed, rows open the reader). Architecture: egui is single-threaded, so every blocking job (SQLite, paced live API, snapshot download) runs on short-lived worker threads feeding an `mpsc` message pump; per-request generation counters drop stale responses; live-request pacing is shared through `Arc<RadaClient>` (default 5 s). Added: `rada-core::github::download_with_progress` (streaming download with progress) and `rada-db::annotations::all` (annotations backup before a snapshot restore). Launch args: `--db`, `--pace`, `--start <home|search|arrivals>`, `--query "text"`, `--nreg 2041-15`. Workspace version → **0.2.0**; release: `rada-client-gui.exe` 12.46 MB + `rada-client.exe` 8.61 MB; dist `C:\rada\dist\rada-client-0.2.0\` (8.2 MB zip) and GitHub release `v0.2.0`. Live-verified on the 295,340-doc index: search "закон" → 8,574 hits; reader `--nreg 2041-15` with card, text, edition and annotations panel; Arrivals — 520 feed documents. Tests: 27/27.

---

## Налаштування для подальших змін / Rule for future changes

- **UKR:** Кожен суттєвий змін (фича, фікс, рефакторинг, документ) — новий пункт угорі цього файлу, завжди двоязиково (UKR + ENG).
- **ENG:** Every significant change (feature, fix, refactor, docs) gets a new entry at the top of this file, always bilingual (UKR + ENG).
