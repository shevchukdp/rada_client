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

### Опубліковано до GitHub, об'єднано історії / Pushed to GitHub, histories merged

- **UKR:** Локальний `main` (20 файлів, 4 435 вставлень) успішно пошито до GitHub. Вхідний пуш-URL використовував одноразовий PAT; скрипт-запускник видалив себе після виконання. На remote був чужий початковий коміт (`2c0ec47`, placeholder-README) — його об'єднано командою `git merge -s ours --allow-unrelated-histories`: збережено НАШЕ дерево, обидві історії тепер в одній гілці. Додано remote `origin` без токенa (токен і далі лише в одноразових URL-ах).
- **ENG:** The local `main` (20 files, 4,435 insertions) was successfully pushed to GitHub. The inbound push URL used a one-time PAT; the launcher script self-deleted after running. The remote had a foreign initial commit (`2c0ec47`, placeholder README) — it was merged with `git merge -s ours --allow-unrelated-histories`: our tree was kept and both histories now live in one branch. A token-free `origin` remote was added (the token is still used only via one-time URLs).

---

## Налаштування для подальших змін / Rule for future changes

- **UKR:** Кожен суттєвий змін (фича, фікс, рефакторинг, документ) — новий пункт угорі цього файлу, завжди двоязиково (UKR + ENG).
- **ENG:** Every significant change (feature, fix, refactor, docs) gets a new entry at the top of this file, always bilingual (UKR + ENG).
