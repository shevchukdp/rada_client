# CHANGES / ЗМІНИ

Журнал змін проєкту, двоязиково: **UKR** та **ENG**.
Bilingual change log for this project: **Ukrainian** and **English**.

---

## 2026-10-28

### 0.10.10: Обране групується та клавіші читалки / grouped favorites & reader page keys

- **UKR:** 1) **Обране за документами**: анотації згруповані за актом — золотий заголовок «Назва (кількість)» з кнопкою «відкрити», рядки всередині групи; поруч — поле фільтра (текст або назва), чипи «усі/коментарі/виділення/закладки» і вимикач «групувати». 2) **Клавіші читалки**: Alt+PageUp / Alt+PageDown гортають документи списку результатів (разом з наявними Alt+←/→), кнопки ‹ › підказують скорочення. 3) Тести: 145/145.
- **ENG:** 1) **Favorites grouped by document**: annotations are grouped per act — a gold header «Title (count)» with an «відкрити» button and nested rows; beside it a filter box (text or title), «усі/коментарі/виділення/закладки» kind chips and a «групувати» toggle. 2) **Reader keys**: Alt+PageUp / Alt+PageDown step through the results list (in addition to Alt+←/→); the ‹ › buttons now hint the shortcut. 3) Tests: 145/145.

---

## 2026-10-28

### 0.10.9: фасети в GUI, глибокі посилання у веб та час читання / GUI facets, web deep links & reading time

- **UKR:** 1) **Фасети в GUI-пошуку**: після пошуку з'являються чипи «Статус/Тип/Видавник» з кількістю результатів («Чинний (36)»); клік встановлює фільтр і перевиконує пошук, повторний клік знімає. 2) **Глибокі посилання у веб**: URL відтворює стан — `?q=&sort=&date_from=&date_to=&status=&typ=&org=&number=` повертає точний результат пошуку після F5 чи за переданим посиланням, `?doc=<номер>` одразу відкриває акт у читалці. 3) **Час читання у читалці**: панель показує кількість слів, символів та оцінку часу читання (≈400 слів/хв), напр. «43 733 слів · 340 212 симв. · ~110 хв». 4) Тести: 140/140.
- **ENG:** 1) **Facets in the GUI search**: after a search, «Статус/Тип/Видавник» chips appear with per-option counts («Чинний (36)»); a click sets that filter and re-runs the search, a second click clears it. 2) **Web deep links**: the URL mirrors the state — `?q=&sort=&date_from=&date_to=&status=&typ=&org=&number=` reproduces the exact result set after F5 or a shared link, and `?doc=<nreg>` opens the act straight in the reader. 3) **Reading time in the reader**: the toolbar shows word count, character count and an estimated reading time (≈400 wpm), e.g. «43 733 слів · 340 212 симв. · ~110 хв». 4) Tests: 140/140.

---

## 2026-10-28

### 0.10.8: CLI-фід оновлень та друк у кольорах теми / CLI arrivals feed & theme-colored printing

- **UKR:** 1) **CLI `arrivals`**: живий фід змінених документів (`r.tsv`) з дифом проти збереженого знімка — нові позначаються `*`, є `--save` (зберегти фід для наступного дифу), `--json` і `--limit N`. 2) **Друк у кольорах теми**: веб-друк (`Друк`) тепер передає поточну тему — заголовки і лінії друкуються її акцентним кольором на білому папері (темні теми не витрачають чорнило). 3) Тести: 137/137.
- **ENG:** 1) **CLI `arrivals`**: the live updated-documents feed (`r.tsv`) diffed against the saved snapshot — new docs are marked `*`, with `--save` (persist the feed for the next diff), `--json` and `--limit N`. 2) **Theme-colored printing**: the web «Друк» button now passes the current theme — headings and rules print in its accent color on white paper (dark themes do not waste ink). 3) Tests: 137/137.

---

## 2026-10-28

### 0.10.7: зміст читалки, «нові» оновлення та фільтри-фасети / reader outline, new-arrival badges & dict facets

- **UKR:** 1) **«Зміст» у читалці**: розкривна панель зі всіма «Розділ/Глава/Стаття» акту (у Кримінальному кодексі — 604); один клік — перехід до заголовка. 2) **«Нові» в Оновленнях**: фід порівнюється з раніше переглянутим — свіжі документи підсвічуються золотим, лічильник «нових» і фільтр «лише нові» (перелік переглянутого зберігається). 3) **Фасети у веб-пошуку**: після пошуку випадаючі списки «Статус/Тип/Видавник» показують кількість результатів для кожного варіанта («Чинний (1204)»); підсумки рахуються SQL-групуванням з тією ж логікою фільтрів. 4) **Виправлено**: список «Статус» у веб не заповнювався (ендпоінт словника 404). 5) Тести: 134/134.
- **ENG:** 1) **Reader outline («Зміст»)**: a collapsible panel listing every «Розділ/Глава/Стаття» heading of the act (604 in the Criminal Code); one click jumps to it. 2) **«New» in the arrivals feed**: the feed is diffed against what you saw before — fresh docs are gold, with a «нових» counter and a «лише нові» filter (the seen list is persisted). 3) **Dict facets in the web search**: after a search the «Статус/Тип/Видавник» dropdowns show per-option result counts («Чинний (1204)»), computed by SQL GROUP BY with the same filter logic. 4) **Fixed**: the web «Статус» dropdown was never populated (dictionary endpoint 404). 5) Tests: 134/134.

---

## 2026-10-28

### 0.10.6: пошук у кеші в GUI / cache full-text search in the GUI

- **UKR:** 1) **«Знайти в кеші» на сторінці пошуку**: кнопка запускає офлайн повнотекстовий пошук у кешованих тілах документів (тільки що прочитані акти); результати з'являються нижче списку — назва з індексу, номер, кількість збігів і курсивний фрагмент контексту; клік відкриває акт у читалці. 2) **CLI читалки**: прапець `--cache-search TERM` відкриває сторінку пошуку з одразу виконаним пошуком у кеші. 3) Тести: 122/122.
- **ENG:** 1) **«Знайти в кеші» on the search page**: the button runs the offline full-text scan over cached document bodies (acts previously read); results appear below the hit list with the index title, nreg, occurrence count and an italic context snippet; clicking a row opens the act in the reader. 2) **Launcher flag**: `--cache-search TERM` opens the search page with the cache scan already running. 3) Tests: 122/122.

---

## 2026-10-28

### 0.10.5: повнотекстовий пошук у кеші та фільтри дат у веб / offline cache full-text search & web date filters

- **UKR:** 1) **Пошук у кешованому тексті (офлайн)**: новий рушій `textcache::search_bodies` шукає підрядок у всіх кешованих тілах документів з урахуванням регістру Unicode (кирилицю SQLite `LIKE` не складає), ранжує за кількістю збігів і дає фрагмент контексту. CLI: `cache --search TERM [--limit N]`; веб: кнопка «Пошук у тексті» на сторінці пошуку (`GET /api/cache/search?q=`) зі збігами, назвами з індексу та клацанням для відкриття. 2) **Фільтри дат у веб**: поля «від/до РРРРММДД» — дійсні 8-значні значення надсилаються як `date_from`/`date_to`, некоректні ігноруються й підсвічуються підказкою (пошук ніколи не блокується). 3) Тести: 121/121.
- **ENG:** 1) **Offline full-text search over the cached texts**: a new `textcache::search_bodies` engine scans every cached document body with Unicode case folding (SQLite `LIKE` does not fold Cyrillic), ranks by occurrence count and returns a context snippet. CLI: `cache --search TERM [--limit N]`; web: a «Пошук у тексті» button on the search page (`GET /api/cache/search?q=`) with hit counts, index titles and click-to-open. 2) **Web date filters**: «від/до РРРРММДД» inputs — valid 8-digit values go out as `date_from`/`date_to`, invalid ones are ignored with a muted hint (the search is never blocked). 3) Tests: 121/121.

---

## 2026-10-28

### 0.10.4: історія запитів, копіювання тексту, зв'язки у друці / query history, copy-text, related in print

- **UKR:** 1) **Історія запитів**: під полями пошуку з'являються чипи «недавні запити» (до 6) — клік повторює пошук; кнопка «очистити» стирає історію (зберігається між сесіями). 2) **Копіювання тексту**: кнопка «копіювати текст» у читалці копіює весь акт у буфер обміну з підтвердженням. 3) **Друк (веб)**: у друковану версію додається секція «Пов'язані документи» (кнопка «Друк» тепер запитує їх з одним additional live-запитом; без мережі друк друкується як раніше). 4) Тести: 117/117.
- **ENG:** 1) **Query history**: «недавні запити» chips (up to 6) under the search filters — click repeats a search; «очистити» clears the list (persisted across sessions). 2) **Copy text**: a «копіювати текст» button in the reader copies the whole act to the clipboard with a toast. 3) **Print (web)**: the print view gains a «Пов'язані документи» section (the «Друк» button now adds one extra live request; offline it prints exactly as before). 4) Tests: 117/117.

---

## 2026-10-28

### 0.10.3: виділення мишею та related у CLI / mouse-selection highlights & CLI related

- **UKR:** 1) **Виділення мишею в читалці**: чип «Виділення мишею» вмикає режим, у якому текст можна виділяти курсором (подвійний клік виділяє слово/абзац); кнопка «виділити» зберігає виділене як золоте виділення-анотацію (обмеження 500 символів, порожні виділення не зберігаються). 2) **CLI**: команда `related <nreg> [--json]` — список пов'язаних документів з офлайн-назвами з індексу. 3) Тести: 113/113.
- **ENG:** 1) **Mouse-selection highlights in the reader**: the «Виділення мишею» chip enables a mode where the text is selectable by mouse (double-click selects a word/paragraph); the «виділити» button saves the selection as a gold highlight annotation (capped at 500 chars, empty selections ignored). 2) **CLI**: `related <nreg> [--json]` prints the related-document list with offline index titles. 3) Tests: 113/113.

---

## 2026-10-28

### 0.10.2: пов'язані документи / related documents

- **UKR:** 1) **«Пов'язані документи» у читалці**: під текстом акту з'являється список посилань на інші документи, які він згадує (розбирається жива сторінка zakon.rada.gov.ua; один виміряний запит на документ). Назви беруться з офлайн-індексу, якщо документ є в базі; клік по рядку відкриває акт у читалці. При помилці мережі — кнопка «оновити». 2) **Веб**: кнопка «Пов'язані документи» в панелі документу (`GET /api/related/{nreg}`) із тим самим списком і переходами. 3) Тести: 110/110 (+1 ігнорований live-тест перевіряє розбір на реальному Кримінальному кодексі — 50 посилань).
- **ENG:** 1) **Related documents in the reader**: below the act text a list of links to other acts it references (parsed from the live zakon.rada.gov.ua page; one paced request per document). Titles come from the offline index when the act is in the database; clicking a row opens it in the reader. On a network error there is an «оновити» retry. 2) **Web**: a «Пов'язані документи» button in the document pane (`GET /api/related/{nreg}`) with the same list and navigation. 3) Tests: 110/110 (+one ignored live test validates the parser on the real Criminal Code — 50 links).

---

## 2026-10-28

### 0.10.1: повна побудова індексу у веб та посилання на розширений пошук / 0.10.1: full-dump index build in web & portal advanced-search link

- **UKR:** 1) **Веб «Дані» → «Повна побудова індексу»**: кнопка «Побудувати з офіційного дампу» запускає фонову задачу `POST /api/jobs/init`, що завантажує офіційний дамп `doc.txt` (≈51 МБ) і перебудовує локальний індекс у межах однієї транзакції (помилка — відкат, наявний індекс і анотації цілі). Хід виконання видно у панелі задач; побудова блокує бекап/відновлення/снимок на час роботи (і навпаки). 2) **Посилання на розширений пошук**: у «Пошуку» — кнопка «Розширений пошук на порталі», що відкриває zakon.rada.gov.ua у браузері (з поточним запитом, закодованим в UTF-8). 3) Тести: 105/105.
- **ENG:** 1) **Web «Дані» → full index build**: a «Побудувати з офіційного дампу» button starts a `POST /api/jobs/init` background job that downloads the official `doc.txt` dump (≈51 MB) and rebuilds the local index inside one transaction (failure rolls back; the existing index and annotations stay intact). Progress shows in the jobs panel; a build excludes backup/restore/snapshot while it runs (and vice-versa). 2) **Advanced-search link**: the search page gains a «Розширений пошук на порталі» button opening zakon.rada.gov.ua in the browser (carrying the current query, UTF-8 encoded). 3) Tests: 105/105.

---

## 2026-10-28

### 0.10.0: умовний HTTP-кеш, пауза наживо, тека експорту, передперегляд обраного / 0.10.0: conditional HTTP cache, live pacing control, export folder memory, favorites preview

- **UKR:** 1) **Економія трафіку**: повторні запити словників і стрічки «Оновлення» використовують умовні HTTP-запити (ETag / If-None-Match, Last-Modified / If-Modified-Since) — сервер відповідає 304 і тіло не завантажується повторно; особливо важливо з урахуванням ліміту ~200 МБ/добу. 2) **Пауза live-запитів керована наживо**: кнопки «−»/«+» у верхній панелі (1–60 с) змінюють паузу без перезапуску й зберігаються між сесіями (CLI `--pace` має пріоритет над збереженою). 3) **Програма пам'ятає теку експорту**: усі діалоги збереження/відкриття (експорт читалки, списку, вибраного, бекапи, імпорт JSON, дамп) відкриваються в останній використаній теці. 4) **Передперегляд у «Обраному»**: для закладок без тексту показується курсивний уривок (≈160–200 символів) з офлайн-кешу тексту — і в GUI, і у веб. 5) Тести: 101/101.
- **ENG:** 1) **Bandwidth savings**: repeated dictionary and arrivals-feed fetches now use conditional HTTP (ETag / If-None-Match, Last-Modified / If-Modified-Since) — the server answers 304 and the body is never re-downloaded; important given the ~200 MB/day limit. 2) **Live pacing is controllable at runtime**: «−»/«+» buttons in the top bar (1–60 s) change the pause without restart and persist across sessions (CLI `--pace` overrides the stored value). 3) **The app remembers the export folder**: every save/open dialog (reader/list/checked exports, backups, JSON import, dump) starts in the last used folder. 4) **Favorites preview**: bookmarks without text show an italic ~160–200-char snippet from the offline text cache — in both the GUI and the web UI. 5) Tests: 101/101.

---

## 2026-10-28

### 0.9.9: номерний пошук, фід офлайн, резервні копії у веб, CLI export / 0.9.9: number search, offline arrivals, web backups, CLI export

- **UKR:** 1) **Пошук за номером офлайн**: рядок вигляду `1556-18`, `1178-2022-п`, `n0423500-26` у полі «Заголовок» відкриває документ напряму в читалці (точний пошук `by_nreg`, без мережі); у веб — те саме через `?q=` (`nreg_exact`). 2) **Фід «Оновлення» тепер офлайн-доступний**: останній знімок зберігається в індексі, при недоступній мережі показується з позначкою «збережене» + час; додано фільтр за назвою/номером/публікацією. 3) **Веб «Дані» → «Резервна копія»**: створення копій (`POST /api/jobs/backup`), список (`GET /api/backups`), відновлення з вибраного файлу з підтвердженням (`POST /api/jobs/restore`) — анотації повертаються, пошкоджений файл видаляється, безпечна заміна БД. 4) **CLI**: `export <nreg> --fmt txt|md|html [--edition] [--out]` — спершу кеш, потім наживо; без картки експортується TXT з кешу. 5) Тести: 92/92.
- **ENG:** 1) **Offline number search**: a query shaped like `1556-18`, `1178-2022-п` or `n0423500-26` opens the document straight in the reader (exact `by_nreg`, no network); the web UI mirrors this via `?q=` (`nreg_exact` auto-open). 2) **The arrivals feed works offline**: the last snapshot is stored in the index and shown with a «збережене» chip + save time when the network fails; new substring filter by title/number/publication. 3) **Web «Дані» → backup area**: create (`POST /api/jobs/backup`), list (`GET /api/backups`) and restore (`POST /api/jobs/restore`, confirm-protected) snapshots — annotations are re-attached, failed unpacks are removed, the db swap is crash-safe. 4) **CLI**: `export <nreg> --fmt txt|md|html [--edition] [--out]` — cache first, then live; card-less offline txt export. 5) Tests: 92/92.

---

## 2026-10-28

### 0.9.8: експорт вибраного у файли та розумніший FTS / 0.9.8: checked-results file export & smarter FTS

- **UKR:** 1) **Експорт вибраного окремими файлами**: чекбокс біля кожного результату, «вибрати все»/«зняти», «Вибране у файли .md/.txt» — обрані документи записуються в обрану теку окремими нумерованими файлами `NNN - №документа - назва.ext`; тексти беруться з кешу, за потреби — наживо (з поповненням кешу); лічильник ходу та фінальний тіст. 2) **Якісніший FTS-пошук**: запит із 2+ слів тепер звужує (AND) замість «або», останнє слово — з префіксом («кодек» знаходить «кодексу»); якщо AND нічого не дав — автоматичне повернення до OR. Приклад на реальній базі: «податковий митний» — 1 документ замість тисяч. 3) Тести: 82/82 (+5 FTS).
- **ENG:** 1) **Checked-results export to individual files**: a checkbox on every result row, "вибрати все"/"зняти", and "Вибране у файли .md/.txt" — the checked documents are written to a chosen folder as individual numbered files `NNN - docnumber - title.ext`; texts come from the offline cache, falling back to paced live fetches (which refresh the cache); live progress plus a summary toast. 2) **Smarter FTS**: multi-word queries now narrow (AND) instead of OR, with the last token prefix-matched («кодек» finds «кодексу»); if AND yields nothing, the search automatically falls back to OR. On the real index: «податковий митний» → 1 document instead of thousands. 3) Tests: 82/82 (+5 FTS).

---

## 2026-10-28

### 0.9.7: офлайн-кеш текстів і простий режим читання / 0.9.7: offline text cache & plain-text mode

- **UKR:** 1) **Офлайн-кеш текстів** (схема БД v3, таблиця `docs_text`): успішне live-завантаження зберігає текст документа (за редакціями); якщо живий доступ не вдався — читалка та CLI `text` віддають кешовану копію з позначкою «кешовано». 2) **CLI**: `cache` — статистика (рядки, розмір), `cache --purge [nreg]` — очищення. 3) **Простий текст**: перемикач «Форматований/Простий» у читалці (GUI) і кнопка «Простий» у веб — вимикає стилізацію заголовків/розділів/статей; вибір зберігається. 4) Тести: 77/77 (+6 кеш, +.round-trip `raw`, +перевірки веб-кнопки).
- **ENG:** 1) **Offline text cache** (DB schema v3, `docs_text` table): every successful live fetch stores the document text (per edition); when the live fetch fails, the reader and CLI `text` serve the cached copy with a «кешовано» chip. 2) **CLI**: `cache` prints stats (rows, size), `cache --purge [nreg]` clears it. 3) **Plain-text mode**: a «Форматований/Простий» toggle in the GUI reader and a «Простий» button in the web UI disable heading/section/article styling; the choice persists. 4) Tests: 77/77 (+6 cache, +`raw` round-trip, +web button checks).

---

## 2026-10-28

### 0.9.6: друк документів / document printing

- **UKR:** 1) **Друк у читалці**: кнопка «Друк» у панелі експорту створює друковану HTML-версію документа у тимчасовому файлі й відкриває її у системному браузері (друк — Ctrl+P там; в egui немає принтера). 2) **Веб**: кнопка «🖨 Друк» у читалці відкриває `/api/export/{nreg}?print=1` — той самий документ інлайн, з друкованими полями (`@page 18/16 мм`) і авто-викликом вікна друку після завантаження. 3) Тести: 71/71 (+web: print-wrap і перевірка кнопки на сторінці).
- **ENG:** 1) **Reader printing**: a «Друк» button in the export bar renders the document's print-ready HTML to a temp file and opens it in the system browser (print with Ctrl+P there — egui has no printer API). 2) **Web**: a «🖨 Друк» button in the reader opens `/api/export/{nreg}?print=1` — the same document served inline with print margins (`@page 18/16 mm`) and the print dialog auto-invoked on load. 3) Tests: 71/71 (+web: print-wrap and the on-page button check).

---

## 2026-10-28

### 0.9.5: діалог «Про програму» та версія у веб / 0.9.5: About dialog & web version

- **UKR:** 1) **Діалог «Про програму»**: кнопки в рядку стану та на «Головній» — версія, шлях і розмір індексу, кількість документів, дата найновіших отриманих даних, останній імпорт, словники, лічильник live-запитів і пауза, джерела даних (відкриті дані Ради, CC BY 4.0, ліміт ~200 МБ/добу) та посилання (GitHub, zakon.rada, data.rada). 2) **Веб**: `/api/status` тепер віддає `version`, у футері сторінки — «rada_client v…». 3) Тести: 69/69 (+тест версії веб-статусу).
- **ENG:** 1) **"About" dialog**: buttons in the status bar and on Home — version, index path and size, document count, freshest fetched data date, last import, dictionaries, live-request counter and pacing, data sources (Rada open data, CC BY 4.0, ~200 MB/day limit) and links (GitHub, zakon.rada, data.rada). 2) **Web**: `/api/status` now returns `version` and the page footer shows "rada_client v…". 3) Tests: 69/69 (+web status version test).

---

## 2026-10-28

### 0.9.4: сортування результатів — клікабельні стовпці / 0.9.4: sortable result columns

- **UKR:** 1) **Сортування у пошуку**: рядок «Сортування» над результатами — «Назва ▲/▼», «Дата ▼/▲» (типово найновіші першими), «Редакція ▼» (дата поточної редакції), «Релевантність» (ранг FTS для запитів із 2+ слів). 2) **Веб**: клікабельні заголовки стовпців над результатами + параметр `?sort=` у `GET /api/search` (date_desc|date_asc|nazva_asc|nazva_desc|podd_desc|relevance). 3) **CLI**: `search --sort <вид>` з тими самими ідентифікаторами. 4) Сортування назв — без урахування регістру (NOCASE), стабільна прив'язка за nreg. 5) Тести: 68/68 (+5 у ядрі — усі напрямки, fallback, парсер; +4 у веб — параметр sort і заголовок сторінки).
- **ENG:** 1) **Search sorting**: a «Сортування» row above results — «Назва ▲/▼», «Дата ▼/▲» (newest-first by default), «Редакція ▼» (current-edition date), «Релевантність» (FTS rank for 2+ word queries). 2) **Web**: clickable column headers over the results plus a `?sort=` parameter on `GET /api/search` (date_desc|date_asc|nazva_asc|nazva_desc|podd_desc|relevance). 3) **CLI**: `search --sort <kind>` with the same identifiers. 4) Title sorting is case-insensitive (NOCASE) with a stable nreg tie-break. 5) Tests: 68/68 (+5 core — every direction, fallback, parser; +4 web — sort param and page header).

---

## 2026-10-28

### 0.9.3: зручність читання й дати — масштаб шрифту, календар, надійний синк, «Обране» у веб / 0.9.3: reading & dates — font zoom, calendar picker, robust sync, web Favorites

- **UKR:** 1) **Масштаб тексту читалки**: кнопки «A−»/«A+» і «100%» (клік — скидання); заголовок і всі абзаци масштабуються 60–250% і зберігаються в settings.ini. 2) **Календар дат** у пошуку («…» біля полів «Дата з/по»): назви місяців українською, тижні з понеділка; недоступні дати пізніше **найновіших успішно отриманих з ради даних** (макс. `org_date` в індексі, з фолбеком на останній імпорт); переставлені «з/по» авто-виправляються. 3) **Надійний синк**: оновлення картки більше не обнулює `dokid`/`org_num`/`organs` наявного документа (беруться лише ненульові значення; картка тепер несе `orgnum`) — номерний пошук більше не ламається після sync. 4) **Веб**: вкладка «Обране» — усі анотації з фільтрами, «→ текст», видаленням, редагуванням, JSON експорт/імпорт (`GET /api/annotations/all|export`, `POST …/import`). 5) Тести: 59/59 (+календар, парсер дат, збереження при sync, форми веб-ендпоінтів).
- **ENG:** 1) **Reader font zoom**: «A−»/«A+» buttons and a «100%» indicator (click resets); the title and every paragraph scale 60–250% and persist in settings.ini. 2) **Date calendar** in search («…» beside the date fields): Ukrainian month names, Monday-first weeks; dates after the **freshest data successfully fetched from rada** (max `org_date`, falling back to last import) are disabled; a reversed from/to range is auto-corrected. 3) **Robust sync**: a card refresh no longer zeroes an existing document's `dokid`/`org_num`/`organs` (only non-empty values are taken; the card now carries `orgnum`) — number search survives syncs. 4) **Web**: an «Обране» tab — all annotations with kind filters, «→ текст», delete, inline edit, JSON export/import (`GET /api/annotations/all|export`, `POST …/import`). 5) Tests: 59/59 (+calendar math, date parser, sync preservation, web endpoint shapes).

---

## 2026-10-28

### 0.9.2: переносні анотації — JSON експорт/імпорт / 0.9.2: portable annotations — JSON export/import

- **UKR:** 1) **Експорт/імпорт анотацій** у портативний JSON — кнопка «⬇ Експорт .json» на сторінці «Обране» та команди CLI `annot export <файл>` / `annot import <файл>`. 2) Формат сумісний між GUI й CLI; імпорт приймає масив або `{"annotations":[...]}`, пропускає биті рядки, не чіпає наявні (дописує нові). 3) нові функції `annotations::export_json` / `import_json`. 4) Тести: 51/51 (+2 на round-trip та стійкість імпорту).
- **ENG:** 1) **Export/import annotations** to portable JSON — a «⬇ Експорт .json» button on the Favorites page and CLI commands `annot export <file>` / `annot import <file>`. 2) The format is compatible between GUI and CLI; import accepts an array or `{"annotations":[...]}`, skips malformed rows and appends without touching existing ones. 3) New `annotations::export_json` / `import_json`. 4) Tests: 51/51 (+2 for round-trip and import robustness).

---

## 2026-10-28

### 0.9.1: керування даними з браузера + повна побудова індексу в GUI / 0.9.1: browser-driven data management + full index build in the GUI

- **UKR:** 1) **Веб**: панель «Дані» — кнопки «Оновити наживо» та «Отримати снапшот» запускають фонові задачі прямо в браузері (нові ендпоінти `POST /api/jobs/sync|snapshot`, `GET /api/jobs`), зі статусом, журналом ходу й захистом від повторного запуску; після завершення лічильники оновлюються. 2) **GUI**: розділ «Повна побудова індексу (init)» на Home — завантаження офіційного `doc.txt` (~51 MB) через виміряний клієнт або імпорт з локального файлу дампу; upsert не чіпає анотацій. 3) Підтвердження перед побудовою, події у журналі дій. 4) Тести: 49/49 (реєстр задач, форми відповідей, сторінка).
- **ENG:** 1) **Web**: the «Дані» panel — «Оновити наживо» and «Отримати снапшот» start background jobs right from the browser (new endpoints `POST /api/jobs/sync|snapshot`, `GET /api/jobs`) with status, progress lines and double-run guard; footer counters refresh on completion. 2) **GUI**: a «Повна побудова індексу (init)» section on Home — downloads the official `doc.txt` (~51 MB) through the paced client or imports a local dump file; the upsert leaves annotations untouched. 3) Confirmation before the build, events in the action log. 4) Tests: 49/49 (job registry, response shapes, page markers).

---

## 2026-10-27

### 0.9.0: резервне копіювання індексу прямо з Home / 0.9.0: index backup & restore from Home

- **UKR:** 1) **Резервна копія** на головній сторінці: «Зберегти резервну копію» стискає поточний індекс у файл `.sqlite.gz` (той самий формат, що й місячні снапшоти), «Відновити з файлу ...» розгортає обрану копію замість поточної бази з перевіркою цілісності. 2) **Анотації зберігаються** в обох напрямках — перед відновленням копіюються й повертаються після нього; відновлення — з вікном підтвердження. 3) Файл копії отримує мітку дати в імені (UTC). 4) Тести: 43/43 (+тест датовання імен копій).
- **ENG:** 1) **Backup section on the Home page**: «Зберегти резервну копію» gzip-compresses the current index into a `.sqlite.gz` file (same format as the monthly snapshots); «Відновити з файлу ...» restores a chosen copy over the current database with an integrity check. 2) **Annotations survive** both directions — backed up before a restore and re-attached after; restoring asks for confirmation. 3) Backup file names carry a UTC date stamp. 4) Tests: 43/43 (+backup name stamping test).

---

## 2026-10-27

### 0.8.0: форматування текстів — читалка розрізняє структуру акта / 0.8.0: structured texts — the reader now understands act anatomy

- **UKR:** 1) **Структурне читання** в GUI: заголовок акта (ВЕЛИКИМИ), назва «Про …», блоки «(Відомості …)» та `{Із змінами …}` (приглушені), «РОЗДІЛ/ГЛАВА» (золоті), «Стаття N» (напівжирний більший шрифт), пункти «1.»/«2)» (відступ) — все класифікується на льоту. 2) **HTML-експорт** тепер структурований (`<p class=dt/ds/mt/sec/art/num>` зі стилями) замість суцільного `<pre>`; gold-виділення зберігаються. 3) **Веб**: той самий поділ рядків через класи `p-doctitle/p-subtitle/p-meta/p-section/p-article/p-num`, кольори з CSS-змінних теми; пошук і золоті мітки рахуються в межах рядків. 4) Тести: 42/42 (новий тест класифікації).
- **ENG:** 1) **Structured reading** in the GUI: ALL-CAPS act header, «Про …» name line, «(Відомості …)»/`{Із змінами …}` blocks (muted), «РОЗДІЛ/ГЛАВА» (gold), «Стаття N» (bold, larger), «1.»/«2)» items (indented) — classified on the fly. 2) **HTML export** is now structured (`<p class=dt/ds/mt/sec/art/num>` with styles) instead of one flat `<pre>`; gold highlights preserved. 3) **Web**: the same line classification via `p-doctitle/p-subtitle/p-meta/p-section/p-article/p-num` classes colored from theme variables; find and gold marks work per line. 4) Tests: 42/42 (new classification test).

---

## 2026-10-27

### 0.7.0: 17 кольорових тем — теми одночасно в GUI й веб / 0.7.0: Colors — 17 color themes in GUI and web

- **UKR:** 1) **17 кольорових тем**: Класична, Біла, Чорна, Інкверт, Авто (системна), «Матриця», opencode, Nord, Dracula, Solarized Dark/Light, Gruvbox Dark, Monokai, One Dark, Tokyo Night, Catppuccin, GitHub Dark — ідентичні набори в Desktop і веб. 2) **GUI**: вибір теми — у шапці («Тема: …»), збереження в settings.ini, «Авто» стежиться за системною theme без перезапуску; усі статусні кольори, виділення та рамки тепер тематичні. 3) **Веб**: сторінка переписана на CSS-змінні, Тема у шапці, збереження в localStorage; gold-виділення читабельні в кожній темі. 4) Новий модуль `themes` із тестами. 5) Тести: 41/41.
- **ENG:** 1) **17 color themes**: Classical, White, Black, Invert, Auto (system), Matrix, opencode, Nord, Dracula, Solarized Dark/Light, Gruvbox Dark, Monokai, One Dark, Tokyo Night, Catppuccin, GitHub Dark — identical sets in Desktop and web. 2) **GUI**: theme picker in the header («Тема: …»), persisted in settings.ini; «Auto» follows the OS without restart; status chips, highlights and frames are now theme-driven. 3) **Web**: page rebuilt on CSS custom properties, header theme selector, localStorage persistence before first paint; gold annotation marks stay readable in every theme. 4) New `themes` module with tests. 5) Tests: 41/41.

---

## 2026-10-27

### 0.6.0: Обране, видавник, живі тексти — сторінка анотацій, фільтр видавця, експорт текстів / 0.6.0: favorites, publisher filter, live texts — annotation page, org filter, bulk text export

- **UKR:** 1) **«Обране»**: окрема сторінка з усіма збереженими анотаціями (коментарі, виділення, закладки) з усіх документів — фільтри за типом, відкриття документа в один клік, видалення. 2) **Фільтр «Видавник»** у пошуку — комбо з текстовим фільтром назв по словнику органів. 3) **«Тексти»**: живий експорт всієї сторінки результатів (повні тексти, з паузами, лічильник прогресу, анотації в кінці). 4) **HTML-експорт** документа — самодостатній файл із золотими виділеннями й списком анотацій. 5) **Shift+Enter** — до попереднього збігу пошуку. 6) CLI-команда **`annot`** (list/add/del — той самий сховище, що GUI/веб). 7) **Паритет веб**: інтерфейс резолвера, редагування анотацій просто у списку (`PUT /api/annotations`), кнопки «ще раз», фільтр видавця, валідація дат, Ctrl+F на сторінці пошуку, панель картки документа, автооновлення «Нових документів», перехід «→ текст»; виправлено залиплі підсвітки пошуку та «трикутні» гліфи на кнопках. 8) Тести: 38/38.
- **ENG:** 1) **Favorites page**: every saved annotation across all documents — filter by kind, one-click open, delete. 2) **Publisher filter** in search (combo with name filter over the organs dict). 3) **«Тексти»**: live bulk export of the results page (full documents, paced, progress counter, annotations appended). 4) **HTML export** of a document — a single self-contained file with gold highlights and the annotation list. 5) **Shift+Enter** steps to the previous find match. 6) CLI **`annot`** command (list/add/del — the same store as GUI/web). 7) **Web parity**: resolver UI, inline annotation editing (`PUT /api/annotations`), «ще раз» retry buttons for failed card/text, publisher filter, date-format validation, Ctrl+F on the search page too, document card details, auto-loaded «Нові документи», «→ текст» jump; also fixed a stale-find-highlights bug and delete-button glyphs. 8) Tests: 38/38.

---

## 2026-10-27

### 0.5.0: клавіатура й останні документи — швидкі клавіші, список останніх, редагування анотацій / 0.5.0: keyboard & recents — shortcuts, recent-documents chips, annotation editing

- **UKR:** 1) **Гарячі клавіші**: Ctrl+F фокусує «знайти в тексті» (читалка) або поле «Заголовок» (Пошук, не відкриває віконного пошуку), Esc у полі пошуку очищає запит, Alt+← і Alt+→ гортають документи у межах списку результатів прямо з читалки. 2) **«Останні документи»**: на «Головній» — чіп-список щойно відкритих документів (до 10 у settings.ini, показуємо 6; старі записи v0.4 стають першим чіпом автоматично). 3) **Редагування анотацій**: кнопка «змінити» при кожній анотації — редактор на місці, без перестворення; виділення зберігають позицію в тексті. 4) **Веб**: той самий набір клавіш (Ctrl+F, Esc, ←/→ між результатами), рядок «Останні» у пошуку (localStorage, до 8), і виправлено «залиплі» підсвітки при скиданні запиту. 5) API `annotations::set_text`; тести: 37/37.
- **ENG:** 1) **Keyboard shortcuts**: Ctrl+F focuses the find-in-text field (reader) or the title field (search — the OS find window is suppressed), Esc in the find field clears the query, Alt+← / Alt+→ page through the search-results list without leaving the reader. 2) **"Recent documents"**: Home shows chips of the last opened documents (up to 10 in settings.ini, 6 rendered; pre-v0.5 "last document" entries migrate into the first chip). 3) **Annotation editing**: a «змінити» button per annotation — inline editor, no delete-and-retype; highlights keep their text position. 4) **Web**: the same keyboard set (Ctrl+F, Esc, ←/→ across the results list), a "Recent" row under the search form (localStorage, up to 8), plus stale find-highlight clearing is fixed. 5) New `annotations::set_text` API; tests: 37/37.

---

## 2026-10-27

### 0.4.0: зручність і паритет — копіювання ідентифікаторів, «Продовжити», веб-паритет, фікс snapshot fetch / 0.4.0: convenience & parity — copy ids, "Continue", web parity, snapshot-fetch fix

- **UKR:** 1) **Копіювання ідентифікаторів**: кнопка «копія» при кожному результаті пошуку та «копіювати» біля «рег. №» у читалці — nreg летить у буфер обміну. 2) **«Продовжити»**: додаток зберігає останній відкритий документ і останній запит (%LOCALAPPDATA%\rada-client\settings.ini) і на «Головній» показує картку «Продовжити: №… — назва»; поле пошуку підвантажує останній запит. 3) **Паритет веб-інтерфейсу**: у браузерному перегляді документа — золоті виділення анотацій просто в тексті, панель «знайти в тексті» (підрахунок збігів, ⌃/⌄, автогортіння), кнопки копіювання nreg; усе без нових залежностей. 4) **Виправлено snapshot fetch**: пошук свіжого снапшота тепер шукає *.sqlite.gz по всіх релізах, а не лише в «останньому» — релізи програми більше не перекривають доступ до індексного снапшота. 5) Testi: 34/34.
- **ENG:** 1) **Copy identifiers**: a «копія» button on every search result and «копіювати» next to «рег. №» in the reader — nreg goes straight to the clipboard. 2) **"Continue"**: the app persists the last opened document and the last query (%LOCALAPPDATA%\rada-client\settings.ini); Home shows a "Continue: №… — title" card, and the search field pre-loads the last query. 3) **Web UI parity**: gold annotation highlights in the browser text, a find-in-text bar (match counter, prev/next, autoscroll) and clipboard buttons — no new dependencies. 4) **snapshot fetch fixed**: fresh-snapshot lookup now scans all releases for the *.sqlite.gz asset instead of only the "latest" release — app releases no longer shadow the index snapshot. 5) Tests: 34/34.

---

### 0.3.0: читалка-level функції — виділення в тексті, пошук у тексті, гортіння результатів / 0.3.0: reader power features — in-text highlights, find-in-text, results paging

- **UKR:** 1) **Виділення видно прямо в тексті**: збережені анотації «виділення» тепер підсвічуються золотим фоном усередині тексту документа; у панелі анотацій з'явився кнопка «» текст» з автогортінням до потрібного речення. 2) **Пошук у тексті** (find-bar): поле в читалці, реєстр_case-insensitive пошуку кирилицею, лічильник «збіг i/N», кнопки » та «, усі збіги підсвічуються блакитним, поточний — яскравішим, з автогортінням. 3) **Перегортання результатів прямо з читалки**: header показує «‹ 3 / 57 ›» — кнопки ‹/› відкривають сусідній документ зі сторінки результатів пошуку, без повернення на сторінку Пошук. 4) **Експорт з анотаціями**: `.md`/`.txt` експорт документа тепер містить секцію «Примітки та виділення» в кінці. 5) Пайплайн снапшотів: `publish-snapshot.ps1` тепер виконує `dict` до стиснення — щомісячні снапшоти міститимуть словники з коробки. Тести: 31/31 (додано 4: філби пошуку, стилізацію сегментів, експорт-додаток).
- **ENG:** 1) **Highlights visible in the text**: saved "highlight" annotations are now rendered with gold backgrounds inside the document body; the annotations panel gained a "» текст" (jump-to-passage) button with auto-scroll to the sentence. 2) **Find-in-text** (find-bar): a field in the reader, case-insensitive Cyrillic matching, a "збіг i/N" counter, » / « buttons, all matches dim-cyan, the current match bright-cyan, with auto-scroll. 3) **Results paging from the reader**: the header shows "‹ 3 / 57 ›" — ‹/› open the adjacent document from the search results page without going back. 4) **Exports include annotations**: `.md`/`.txt` export now ends with a "Примітки та виділення" (notes) section. 5) Snapshot pipeline: `publish-snapshot.ps1` now runs `dict` before compression — monthly snapshots will ship with dictionaries pre-loaded. Tests: 31/31 (4 new: find spans, segment styling, export appendix).

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

### 0.2.1: словники наживо (фікс typ.txt), експорт списку пошуку, `dict` в CLI / 0.2.1: live dicts (typ.txt fix), search-list export, CLI `dict`

- **UKR:** 1) **Фікс словників**: сервер публікує словник типів як `typ.txt` (не `typs.txt`) — раніше завантаження давало 404 і весь `refresh_dicts` падав на першому ж збігу; тепер мапінг `typs → typ.txt` у `rada-core::api` та `refresh_dicts` стала tolerant-функцією (повертає список не завантажених словників замість помилки на весь синк; `rada-cli sync` друкує попередження). 2) **Автозавантаження словників у GUI**: якщо індекс існує, а словники порожні (наприклад, після снапшоту), на старті автоматично виконується живий запит 3 словників (темповано) + toast-повідомлення; фільтри «Стан/Тип» в пошуку одразу робочі (перевірено: stan 10 / typs 138 / organs 799 записів). 3) **Експорт списку пошуку (GUI)**: нові кнопки «Експорт .md / .txt» на сторінці Пошук — зберігають поточну сторінку результатів (назви, №, стан, номер, редакція) через діалог збереження; офлайн, без живих запитів. 4) **Нова CLI-команда `dict`**: `rada-client dict [--kind stan|typs|organs]` — завантажити словники з живого API (зручно для снапшот-пайплайну: словники можна запакувати в щомісячний снапшот). 5) README оновлено під GUI-first 0.2.x. Тести: 27/27.
- **ENG:** 1) **Dictionaries fix**: the server publishes the types dictionary as `typ.txt` (not `typs.txt`) — previously the fetch 404'd and the whole `refresh_dicts` aborted on the first miss; now `rada-core::api` maps `typs → typ.txt`, and `refresh_dicts` is tolerant (returns the list of failed kinds instead of failing the entire sync; `rada-cli sync` prints a warning). 2) **GUI auto dict-load**: when the index exists but the dicts are empty (e.g. after a snapshot restore), startup automatically performs the 3 paced live dict requests + a toast; the Search "Status/Type" filters work immediately (verified: stan 10 / typs 138 / organs 799 entries). 3) **Search-list export (GUI)**: new "Export .md / .txt" buttons on the Search page save the current results page (titles, nreg, status, number, edition) through a save dialog; fully offline, no live requests. 4) **New CLI command `dict`**: `rada-client dict [--kind stan|typs|organs]` — live-load the dictionaries (handy for the snapshot pipeline: dicts can be packed into the monthly snapshot). 5) README updated for the GUI-first 0.2.x layout. Tests: 27/27.

---

## Налаштування для подальших змін / Rule for future changes

- **UKR:** Кожен суттєвий змін (фича, фікс, рефакторинг, документ) — новий пункт угорі цього файлу, завжди двоязиково (UKR + ENG).
- **ENG:** Every significant change (feature, fix, refactor, docs) gets a new entry at the top of this file, always bilingual (UKR + ENG).
