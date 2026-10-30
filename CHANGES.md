# CHANGES / ЗМІНИ

Журнал змін проєкту, двоязиково: **UKR** та **ENG**.
Bilingual change log for this project: **Ukrainian** and **English**.

---

## 2026-10-30

### 0.10.56: `annots` у CLI, «експорт .csv» обраного у вебі та «Документи за роками» на головній / CLI `annots`, web favorites CSV & home years histogram

- **UKR:** 1) **`annots <НРЕГ>` у CLI**: анотації одного документа просто з індексу — той самий формат рядків, що у `recent-annots`; `--kind коментар|закладка|виділення` (ще й без регістру), `--limit ≤500`, `--json` і `--out` за загальним стандартом. 2) **«експорт .csv» на сторінці «Обране» у веб-клієнті**: показаний список (фільтр + сортування) виїжджає файлом у форматі CSV самої програми — з BOM; кнопка гасне, коли показувати нічого. 3) **«Документи за роками» на головній сторінці програми**: останні 15 років з індексу («2026 · 5492»…) — третій двійник `years` поряд із CLI та вебом. 4) Тести: 394/394.
- **ENG:** 1) **`annots <NREG>` in the CLI**: one document's annotations straight from the index — the same row format as `recent-annots`; `--kind коментар|закладка|виділення` (case-insensitive too), `--limit ≤500`, `--json`/`--out` to the common standard. 2) **An «експорт .csv» on the web client's Favorites page**: the visible list (filter + sort) exports in the desktop app's own CSV format with BOM; the button dims when there is nothing to show. 3) **A «Документи за роками» card on the desktop home page**: the last 15 years from the index («2026 · 5492»…) — the third `years` twin beside the CLI and the web. 4) Tests: 394/394.

### 0.10.55: `docs --org` у CLI, «Документи за роками» у веб-клієнті та Ctrl+Shift+T для вкладок / CLI `docs --org`, web years bar chart & Ctrl+Shift+T tabs

- **UKR:** 1) **`docs --org <ORG>` у CLI**: список документів одного видавця з індексу, найновіші попереду — продовження `orgs`: той самий ключ видавця та та сама цифра в «разом» (перевірено на org 1: 308 = 308); `--year РРРР`, `--limit ≤500`, `--json` і `--out` за загальним стандартом. 2) **«Документи за роками» на сторінці веб-клієнта**: міні-гістограма з пропорційними смужками — веб-двійник CLI `years` (`/api/years`), останні 15 років; зникає після першого пошуку, як і картка свіжих документів. 3) **Ctrl+Shift+T повертає щойно закриту вкладку читалки** (стек до 10 записів, як у браузерів). 4) Тести: 386/386.
- **ENG:** 1) **`docs --org <ORG>` in the CLI**: one publisher's documents straight from the index, newest first — the drill-down companion to `orgs`: the same publisher key and the same «разом» number (verified on org 1: 308 = 308); `--year YYYY`, `--limit ≤500`, `--json`/`--out` to the common standard. 2) **A «Документи за роками» mini bar chart on the web client's landing page**: proportional bars, the whole index — the CLI `years` twin via `/api/years`, the last 15 years; it retires after the first search like the fresh-docs card. 3) **Ctrl+Shift+T brings the last closed reader tab back** (a 10-entry redo stack, browser-style). 4) Tests: 386/386.

## 2026-10-29

### 0.10.54: `orgs --year` у CLI, «Свіжі документи» у веб-клієнті та Ctrl+W для вкладок / CLI `orgs --year`, web fresh-docs card & Ctrl+W tabs

- **UKR:** 1) **`orgs --year РРРР` у CLI**: розклад видавців тепер фільтрується роком прийняття — «Видавці документів 2024 року:» раховує лише акти з повною датою цього року; `--json` додає «year», `--out` за загальним стандартом. 2) **«Свіжі документи (за датою)» на сторінці веб-клієнта**: той самий список, що й на головній програмі, — свіжі акти з офлайн-індексу (`/api/fresh`), рядок відкриває читалку; зникає після першого пошуку. 3) **Ctrl+W закриває активну вкладку читалки** (разом із «×»); підказка «клавіші» оновлена. 4) Тести: 378/378.
- **ENG:** 1) **`orgs --year YYYY` in the CLI**: the publisher histogram now filters by adoption year — «Видавці документів 2024 року:» counts only acts with a full date of that year; `--json` gains «year», `--out` follows the common standard. 2) **A «Свіжі документи (за датою)» card on the web client's landing page**: the same list as the desktop home — the freshest acts from the offline index (`/api/fresh`), a row opens the reader; it disappears after the first search. 3) **Ctrl+W closes the active reader tab** (besides «×»); the shortcuts sheet is updated. 4) Tests: 378/378.

### 0.10.53: вкладки читалки, `years` у CLI та «кеш» у веб-результатах / reader tabs, CLI `years` & web cache badges

- **UKR:** 1) **Вкладки у читалці**: кожен відкритий акт стає вкладкою — перемикання зберігає текст, пошук по сторінці й вибрану редакцію; **Ctrl+Tab / Ctrl+Shift+Tab** гортають вкладки, **×** закриває; повторно відкритий акт просто активує свою вкладку; відкриті вкладки (до 12) повертаються після перезапуску миттєвими оболонками. 2) **`years` у CLI**: офлайн-розклад документів по роках закритого періоду (`--from РРРР --to РРРР`), з «разом» і «без дати»; `--json` і `--out` за загальним стандартом. 3) **Значок «· кеш» у веб-результатах пошуку**: той самий, що й у програмі, — акти з уже кешованим текстом відкриваються без мережі. 4) Тести: 373/373.
- **ENG:** 1) **Reader tabs**: every opened act becomes a tab — switching keeps its text, find query and chosen edition; **Ctrl+Tab / Ctrl+Shift+Tab** cycle tabs, **×** closes one; reopening an act just activates its tab; the open set (up to 12) returns as instant shells after a restart. 2) **`years` in the CLI**: an offline per-year breakdown over a closed range (`--from YYYY --to YYYY`) with «разом» and «без дати», plus `--json`/`--out` to the common standard. 3) **A «· кеш» badge on web search rows** — same as the desktop app's: acts whose text is already cached open without network. 4) Tests: 373/373.

### 0.10.52: точні дати в CLI, «Друк (простий)» та статистика індексу в підвалі веб-клієнта / exact CLI dates, plain print & web footer index stats

- **UKR:** 1) **Виправлено датну математику в CLI**: `arrivals --since` раніше плутав сусідні дні (кінець місяця зливався з наступним, від лютого до березня хибка сягала 3 днів); тепер лічба днів — за пролептичним григоріанським календарем, як у всього решти світу. 2) **«Друк (простий)» у читалці**: та сама друкована версія, але без форматування — тільки текст, без класів, золотих акцентів і підсвічувань виділень. 3) **Підвал веб-клієнта показує стан індексу**: «rada-client v… · 295 340 документів · 5 тіл у кеші · оновлено: 25.10.2026» (коли дату імпорту невідомо — «невідомо»). 4) Тести: 364/364.
- **ENG:** 1) **CLI date math fixed**: `arrivals --since` used to confuse adjacent days (month ends collapsed into the next day, and Feb→Mar drifted by up to 3 days); day counting now follows the proleptic Gregorian calendar like everywhere else. 2) **«Друк (простий)» in the reader**: the same print-ready view without formatting — plain text only, no structural classes, gold accents or highlight marks. 3) **The web client footer shows index facts**: «rada-client v… · 295 340 документів · 5 тіл у кеші · оновлено: 25.10.2026» («невідомо» when the import date is unknown). 4) Tests: 364/364.

### 0.10.51: «Свіжі документи» на головній, `recent` у CLI та A+/A-/A0 у веб-читалці / home fresh-docs card, CLI `recent` & web reader font controls

- **UKR:** 1) **Картка «Свіжі документи (за датою)» на головній**: 14 найсвіжіших карток за датою прийняття просто з індексу — без мережі; клік відкриває читалку. 2) **`recent` у CLI**: офлайн-список найсвіжіших документів за датою прийняття (`--days 1–365`, типово 7; відлік від найновішої дати індексу, не від годинника; `--limit` до 500), `--json` і `--out` за загальним стандартом. Команда анотацій перейменована на `recent-annots` — зрозуміліше, бо `recent` тепер про документи. 3) **A+/A-/A0 у веб-читалці**: розмір тексту кнопками або з клавіатури (Ctrl+= / Ctrl+- / Ctrl+0), той самий діапазон 60–250%, що й у програмі, вибір зберігається. 4) Тести: 356/356.
- **ENG:** 1) **A «Свіжі документи (за датою)» card on the home page**: the 14 freshest cards by adoption date, straight from the index — no network; one click opens the reader. 2) **`recent` in the CLI**: an offline freshest-documents list by adoption date (`--days 1–365`, default 7, anchored to the index's newest date rather than the system clock; `--limit` up to 500), with `--json` and `--out` to the common standard. The annotation stream moved to `recent-annots` — clearer now that `recent` is about documents. 3) **A+/A-/A0 in the web reader**: resize the text with buttons or Ctrl+= / Ctrl+- / Ctrl+0, the same 60–250% range as the desktop app, the choice persists. 4) Tests: 356/356.

### 0.10.50: «копія» статей у веб-читалці, `months` у CLI та Ctrl+F на новинках/улюблених / web article copy chips, `months` CLI command & Ctrl+F on arrivals/favorites

- **UKR:** 1) **«копія» біля кожного заголовка у веб-читалці**: у списку «Зміст» кожна «Стаття/Розділ/Глава» отримала свою позначку «копія» — текст заголовка йде в буфер із блимкою «скопійовано»; сама позначка не потрапляє ані в буфер, ані в підсвічування пошуку по сторінці, а наддовгих актах (понад 400 заголовків) вона не показується. 2) **`months` у CLI**: розклад документів по місяцях одного року (`--year РРРР`, типово поточний) — завжди 12 рядків + «разом», повністю офлайн; `--json` і `--out` за загальним стандартом. 3) **Ctrl+F тепер і на «Новинках», і в «Улюбленому»**: фокус у полі текстового фільтра цих сторінок — як уже було для читалки та пошуку. 4) Тести: 348/348.
- **ENG:** 1) **A «копія» chip on every web-reader heading**: in the «Зміст» outline each «Стаття/Розділ/Глава» heading gained its own copy chip — the heading text goes to the clipboard with a «скопійовано» flash; the chip's own label never reaches the clipboard or the page's find highlights, and it is hidden in documents with over 400 headings. 2) **`months` in the CLI**: a per-month breakdown of one year's documents (`--year YYYY`, current year by default) — always 12 rows plus «разом», fully offline, with `--json` and `--out` to the common standard. 3) **Ctrl+F now works on arrivals and favorites too**: it focuses those pages' text filter, as it already did for the reader and search. 4) Tests: 348/348.

### 0.10.49: статистика тексту у веб-читалці, «кеш» у результатах пошуку та `--out` усюди в CLI / reader doc-stats, cache badges & CLI --out everywhere

- **UKR:** 1) **Статистика під заголовком акта у веб-читалці**: «43 733 слів · 340 212 симв. · ~110 хв» — той самий вигляд і та сама методика, що й у програмі; ховається, поки текст не завантажений. 2) **«· кеш» біля результатів пошуку в програмі**: позначка на тих актах, текст яких уже є в офлайн-кеші — відкриються без мережі. 3) **CLI доведений до єдиного стандарту `--out`**: `status` і `cache` (статистика) отримали запис у файл, а `cache --search --json`, `arrivals`, `resolve --json`, `text` та `annot export` — спільні будівники тіла й нотаток; файли збігаються з stdout байт у байт. 4) Тести: 342/342.
- **ENG:** 1) **A stats line under the web reader's title**: «43 733 слів · 340 212 симв. · ~110 хв» — the same wording and counting rules as the desktop app; hidden until the text loads. 2) **A «· кеш» mark on the app's search rows** flags acts whose text is already cached for offline reading. 3) **The CLI now meets one uniform `--out` standard**: `status` and `cache` (stats) can write files, and `cache --search --json`, `arrivals`, `resolve --json`, `text` and `annot export` share the body builders and notes — files match stdout byte-for-byte. 4) Tests: 342/342.

---

## 2026-10-29

### 0.10.48: `statuses` з CLI, стійкий вигляд Обраних та надійні лічби фільтрів у веб / statuses histogram, durable favorites view & robust facet counts

- **UKR:** 1) **CLI `statuses`**: офлайн розподіл документів за статусами («Чинний [5]: 122522 …») з міткою зі словника «стани» і підсумком «всього документів»; `--json` і `--out FILE` (таблиця з BOM / JSON без BOM). 2) **Вигляд Обраних у програмі тепер стійкий**: текстовий фільтр, чип виду, сортування й «групувати» повертаються після перезапуску. 3) **Лічби збігів у випадаючих списках фільтрів веб-пошуку** стали надійнішими: некоректні дані фільтрів більше не ламають пошук, мітки списків, чипів і друку лишаються чистими. 4) Тести: 328/328.
- **ENG:** 1) **CLI `statuses`**: an offline document-status distribution («Чинний [5]: 122522 …») with dictionary labels and a «всього документів» footer; `--json` and `--out FILE` (BOM table / BOM-less JSON). 2) **The desktop Favorites view is now durable**: the text filter, kind chip, sort and «групувати» toggle come back after a restart. 3) **Facet counts in the web search dropdowns got sturdier**: a malformed facet payload can no longer break a healthy search, and filter/chip/print labels stay clean. 4) Tests: 328/328.

---

## 2026-10-29

### 0.10.47: CSV-експорт оновлень у веб, `orgs` з CLI та Ctrl+Enter на Оновленнях / arrivals CSV export, orgs histogram & arrivals Ctrl+Enter

- **UKR:** 1) **«експорт .csv» на «Нових документах» у веб**: завантажує саме показаний перелік (текстовий фільтр і «лише нові» враховуються), той самий формат, що й у програмі — UTF-8 з BOM, рядок заголовків, правильне лапкове екранування; порожній перелік — файл із одним заголовком. 2) **CLI `orgs [--limit N]`**: офлайн рейтинг видавців за кількістю документів (словник «органи» + лічба по індексу), «всього документів» у підсумку; `--json` і `--out FILE` (таблиця з BOM / JSON без BOM). 3) **Ctrl+Enter на сторінці Оновлень** відкриває перший показаний документ у читалці — спільний код фільтрації з CSV-експортом; «клавіші» оновлено. 4) Тести: 322/322.
- **ENG:** 1) **An «експорт .csv» button on the web «Нові документи» feed**: downloads exactly the visible list (text filter and «лише нові» apply) in the app's own format — UTF-8 with BOM, header row, proper quote escaping; an empty list still yields a header-only file. 2) **CLI `orgs [--limit N]`**: an offline publisher ranking by document count (the «органи» dictionary joined with index counts) plus a «всього документів» footer; `--json` and `--out FILE` (BOM table / BOM-less JSON). 3) **Ctrl+Enter on the arrivals page** opens the first visible document in the reader, sharing one filter pipeline with the CSV export; the «клавіші» sheet updated. 4) Tests: 322/322.

---

## 2026-10-29

### 0.10.46: швидкі редакції у веб-читалці, `dates` з CLI та Ctrl+Enter у пошуку / edition quick-nav, dates histogram & Ctrl+Enter

- **UKR:** 1) **Кнопки «попередня»/«наступна» біля списку редакцій у веб-читалці**: переходять редакціями по колу — поточна ↔ dated, тим самим кроком, що й випадаючий список; ховаються, коли редакція одна, і блокуються на час завантаження тексту. 2) **CLI `dates`**: офлайн розподіл документів за роками прийняття (та сама колонка, що фільтри `search --date-from/--date-to`) — «без дати» окремим рядком і вердикт «найновіший рік»; `--json` і `--out FILE` (таблиця з BOM / JSON без BOM). 3) **Ctrl+Enter на сторінці пошуку** відкриває перший результат у читалці; рядок додано у «клавіші». 4) Тести: 316/316.
- **ENG:** 1) **«попередня»/«наступна» buttons beside the web reader's editions list**: they cycle editions with wrap-around — поточна ↔ dated — through the same path as the dropdown; hidden when a doc has a single edition and disabled while the text is fetching. 2) **CLI `dates`**: an offline adoption-year histogram (the very column `search --date-from/--date-to` filters) — an «без дати» line plus a «найновіший рік» verdict; `--json` and `--out FILE` (BOM table / BOM-less JSON). 3) **Ctrl+Enter on the search page** opens the top hit in the reader; the «клавіші» sheet gained the row. 4) Tests: 316/316.

---

## 2026-10-29

### 0.10.45: підказки при порожньому пошуку у веб, `hits` з CLI та часові мітки журналу / empty-state suggestions, hits export & journal stamps

- **UKR:** 1) **Порожній результат пошуку у веб тепер не порожній**: «нічого не знайдено» плюс чипи «скинути фільтри», «шукати в кеші» та по чипу на кожен увімкнений фільтр, який блокує відповіді — кожен чистить саме свій фільтр і повторює пошук. 2) **CLI `hits TERM`**: список знайдених документів офлайн у txt або markdown (`--format md`) — номер, назва, посилання на сайт, статус і дати; `--limit` до 500; `--out FILE` (таблиця з BOM); мережі не чіпає. 3) **«Журнал дій» на Головні** тепер із часовими мітками `[HH:MM]` (UTC) на кожному рядку й кнопкою «очистити». 4) Тести: 310/310.
- **ENG:** 1) **An empty web search result is no longer empty**: under «нічого не знайдено» it offers «скинути фільтри», «шукати в кеші» and one chip per blocking filter — each clears just its own filter and re-searches. 2) **CLI `hits TERM`**: the result list, fully offline, as txt or markdown (`--format md`) — numbering, title, site link, status and dates; `--limit` up to 500; `--out FILE` (BOM table); never touches the network. 3) **The home «Журнал дій» now stamps every line `[HH:MM]` (UTC)** and gained an «очистити» button. 4) Tests: 310/310.

---

## 2026-10-29

### 0.10.44: рядок прогресу jobs у «Даних», дії рядків Обраних та доведений `card` / job progress line, favorites row actions & tidy card

- **UKR:** 1) **Живий рядок прогресу у веб-«Даних»**: поки завантаження індекса/знімка/резервної копії триває, рядок показує поточну фразу (`«картка 1234/56789: …»`), а по завершенню — лише результат; жодного зайвого таймера. 2) **Рядки Обраних у програмі** тепер такі самі, як у пошуку: «цилата», «копія» (номер) і «сайт» (акт на zakon.rada.gov.ua) — поряд з відкриттям і видаленням. 3) **CLI `card`** доведений до стандарту: тіла stdout і файлу будуються спільним кодом, `--json` отримав власну нотатку «[card] записано JSON», невдала картка не пише файлу. 4) Тести: 303/303.
- **ENG:** 1) **A live progress line in the web «Дані» panel**: while a sync/snapshot/backup job runs, its row shows the current progress phrase (`«картка 1234/56789: …»`); finished rows show just the result — no extra timers. 2) **Desktop Favorites rows** now match the search rows: «цилата», «копія» (copy nreg) and «сайт» (open the act on zakon.rada.gov.ua) beside open and delete. 3) **CLI `card`** brought to the shared standard: stdout and file bodies built by one code path, `--json` gained its own «[card] записано JSON» note, and a failed card fetch writes no file. 4) Tests: 303/303.

---

## 2026-10-29

### 0.10.43: «Зміст» складається у веб, `favorites` з CLI та .txt-експорт Обраних у програмі / collapsible outline, CLI favorites & favorites .txt export

- **UKR:** 1) **«Зміст» у веб-читалці тепер складається**: клік по заголовку ховає список і дописує «— згорнуто»; вибір запам'ятається (навіть після перезавантаження сторінки), а стрибки по заголовках лишились ті самі. 2) **CLI `favorites [--kind] [--sort]`**: весь список анотацій офлайн — груповання за номером (типово), або «за назвою», або найновіші; `--kind` приймає і українські мітки (коментарі/виділення/закладки); `--json` (групований або плаский) і `--out FILE` (таблиця з BOM / JSON без BOM). 3) **«експорт .txt» на Обраних у програмі** — саме те, що бачиш: фільтр, чип виду й сортування враховуються, як у вебі. 4) Тести: 298/298.
- **ENG:** 1) **The web reader's «Зміст» is now collapsible**: clicking the header hides the list and appends «— згорнуто»; the choice persists across documents and page reloads, and the heading jumps stay exactly as they were. 2) **CLI `favorites [--kind] [--sort]`**: the whole annotation list, offline — grouped by number (default), by title, or newest first; `--kind` also accepts the Ukrainian labels (коментарі/виділення/закладки); `--json` (grouped or flat) and `--out FILE` (BOM table / BOM-less JSON). 3) **An «експорт .txt» button on the app's Favorites** downloads exactly what's on screen — text filter, kind chip and sort all apply, web parity. 4) Tests: 298/298.

---

## 2026-10-29

### 0.10.42: «цилата» в Обраних (програма і веб) та `recent` з CLI / favorites citation & recent annotations

- **UKR:** 1) **«цилата» на кожному рядку Обраних** — і в програмі, і у веб: копіює повне посилання на акт (««Назва» // Офіційний вебпортал парламенту України. URL: …») у той самий формат, що й читалка; у веб ще й блимає «скопійовано» на кнопці. 2) **CLI `recent [--limit N]`**: найсвіжіші анотації з усіх документів одним списком (як «новіші» в Обраних програми) — назва або номер, вид, час і текст; `--json` і `--out FILE` (таблиця з BOM / JSON без BOM). Офлайн. 3) Тести: 291/291.
- **ENG:** 1) **A «цилата» button on every Favorites row — in the app and the web**: copies the full reference (««Title» // Офіційний вебпортал парламенту України. URL: …») in exactly the reader's format; the web button also flashes «скопійовано». 2) **CLI `recent [--limit N]`**: the freshest annotations across all documents in one list (the app Favorites' «новіші» view) — title or number, kind, time and text; `--json` and `--out FILE` (BOM table / BOM-less JSON). Fully offline. 3) Tests: 291/291.

---

## 2026-10-29

### 0.10.41: швидкі чипи статусу у веб, `backfill --out` та експорт порівняння редакцій / status quick chips, backfill --out & compare export

- **UKR:** 1) **Чипи статусу під фільтрами веб-пошуку**: до трьох частих статусів зі словника одним кліком (без розгорнутого списку) плюс «Усі»; чипи беруться з самого списку статусів, підсвічують обраний і повторюють пошук; працюють і для посилань `?status=`. 2) **CLI `backfill --out FILE`**: підсумковий звіт доповнення кеша тепер їде у файл (таблиця з BOM / `--json` без BOM), рядки прогресу лишаються у stdout; порожній пошук — лише BOM. 3) **«експорт звіту» у панелі «Порівняти редакції»** програми: зберігає порівняння двох редакцій у .txt — з назвою акта, номером, обома редакціями й усіма рядками звіту. 4) Тести: 285/285.
- **ENG:** 1) **Status quick chips under the web search filters**: up to three frequent dictionary statuses in one click (no dropdown) plus «Усі»; built from the status select itself, they highlight the active choice and re-run the search, and stay correct for `?status=` deep links. 2) **CLI `backfill --out FILE`**: the cache-fill summary now lands in a file (BOM table / BOM-less `--json`) while progress lines stay on stdout; a zero-hit run writes only the BOM. 3) **An «експорт звіту» button in the desktop «Порівняти редакції» panel** saves the two-edition comparison as .txt — act title, number, both editions and every report line. 4) Tests: 285/285.

---

## 2026-10-29

### 0.10.40: лічильник збігів у веб-пошуку, `dict-stats` та Alt+↑/↓ для редакцій / find match counter, dict-stats & edition hotkeys

- **UKR:** 1) **«N з M» у пошуку по тексту веб-читалки**: лічильник показує, який збіг зараз і скільки їх усього; «нічого не знайдено» замість тиші, ховається при порожньому запиті й переході на інший акт. 2) **CLI `dict-stats`**: офлайн звіт заповненості словників довідників — «стани: 10 / типи: 138 / органи: 799» + загальна кількість; `--json` і `--out FILE` (таблиця з BOM / JSON без BOM). 3) **Alt+↑/↓ у читалці програми** гортають редакції: поточна ↔ dated по колу, один вивірений запит. 4) Тести: 280/280.
- **ENG:** 1) **An «N з M» counter in the web reader's find bar**: which match is current and how many there are; «нічого не знайдено» instead of silence, hidden on an empty query or a new act. 2) **CLI `dict-stats`**: an offline report of reference-dictionary population — «стани: 10 / типи: 138 / органи: 799» plus a total; `--json` and `--out FILE` (BOM table / BOM-less JSON). 3) **Alt+↑/↓ in the desktop reader** cycle editions: поточна ↔ dated with wrap-around, one paced request. 4) Tests: 280/280.

---

## 2026-10-29

### 0.10.39: текстовий експорт обраних у веб, спільний JSON анотацій та Ctrl+M / favorites .txt export, shared annotation JSON & Ctrl+M

- **UKR:** 1) **«експорт .txt» на Обраних у веб**: завантажує саме те, що бачиш — поточний фільтр, чип виду і сортування враховуються; перший рядок «Обрані анотації — X з Y», далі кожна анотація у форматі «[вид] назва (номер)» і текст. 2) **CLI `annot`**: `list --json` і `search --json` тепер будуються одним спільним кодом (ті самі сім полів, той самий вигляд байт у байт) із спільною нотаткою «[annot] записано JSON»; нульова таблиця `--out` перевірена — лише BOM. 3) **Ctrl+M у читалці програми** перемкнути форматований/простий текст одним клажем; рядок додано у «клавіші». 4) Тести: 274/274.
- **ENG:** 1) **An «експорт .txt» button on the web Favorites page** downloads exactly what is on screen — the text filter, kind chip and sort all apply; a «Обрані анотації — X з Y» header followed by «[kind] title (nreg)» lines with the annotation text. 2) **CLI `annot`**: `list --json` and `search --json` now share one body builder (the same seven fields, byte-identical output) and one «[annot] записано JSON» note; the zero-hit `--out` table was audited — BOM only. 3) **Ctrl+M in the desktop reader** toggles форматований/простий text mode with a single keypress; the «клавіші» sheet gained the row. 4) Tests: 274/274.

---

## 2026-10-29

### 0.10.38: експорт актів із веб-читалки, `Ctrl+B` та доведений до тями `related` / web export buttons, Ctrl+B bookmark & tidy related

- **UKR:** 1) **Експорт у веб-читалці**: кнопки `.md` / `.txt` / `.html` завантажують поточний акт з браузера без зайвих запитів (тіло бере з уже завантаженого) — у той самий формат, що й CLI `export`; назви файлів без небезпечних символів. 2) **Ctrl+B у програмі** зберігає закладку поточного акта прямо з читалки — без клацання по панелі анотацій. 3) **CLI `related`** доведений до спільного стандарту: `--json` будується одним кодом зі stdout, `--out` при нулі пише лише BOM (рядок «не знайдено» лишається у stdout), нотатки — чисті функції. 4) Тести: 269/269.
- **ENG:** 1) **Web reader export**: `.md` / `.txt` / `.html` buttons download the current act straight from the browser (reusing the already-loaded body) in exactly the CLI `export` formats, with sanitized filenames. 2) **Ctrl+B in the app** bookmarks the open act right from the reader — no annotation-panel clicking. 3) **CLI `related`** brought to the shared standard: `--json` built by one code path with stdout, a zero-link `--out` writes only the BOM (the «не знайдено» line stays on stdout), notes are pure fns. 4) Tests: 269/269.

---

## 2026-10-29

### 0.10.37: веб-читалка тепер працює офлайн, повний `--out` для словників та Alt+1–5 / offline web reader, dict --out everywhere & Alt+digit pages

- **UKR:** 1) **Офлайн-стійкість веб-читалки**: якщо живий запит тіла не вдався, сервер тепер віддає **кешовану** копію (як у програмі) і підсвічує підказку «кешовано» в шапці читалки; успішне читання поповнює кеш. 2) **CLI `dict`**: кожен офлайн-режим тепер чесно пише у файл — `--json` будується спільним кодом зі stdout (таблиця з BOM / JSON без BOM), нотатки «[dict] … записано» для всіх режимів. 3) **Alt+1…Alt+5 у програмі** — миттєвий перехід між Головна / Пошук / Читалка / Оновлення / Обране (Читалка — лише з відкритим актом); рядок додано у «клавіші». 4) Тести: 265/265.
- **ENG:** 1) **The web reader works offline now**: when the live body fetch fails the server serves the **cached** copy (desktop parity) and the reader shows a «кешовано» hint; successful reads refill the cache. 2) **CLI `dict`**: every offline mode now writes files honestly — `--json` built by one shared body with stdout (BOM table / BOM-less JSON), with «[dict] … записано» notes across all modes. 3) **Alt+1…Alt+5 in the app** jump straight between Головна / Пошук / Читалка / Оновлення / Обране (Читалка only with an act open); the sheet gained the row. 4) Tests: 265/265.

---

## 2026-10-29

### 0.10.36: «оновлено» на стрічці веб, перевірка бази `verify` та клавіші Ctrl+K/F5 / feed freshness, db verify & new shortcuts

- **UKR:** 1) **«оновлено: щойно / N хв тому / N год тому / N дн тому» у вебі**: поряд із лічильниками Оновлень — час останнього успішного завантаження фіда (і вручну, і автооновленням); у фоновій вкладці не вантажиться. 2) **CLI `verify`**: офлайн перевірка здоров'я індекса — цілість SQLite, зовнішні ключі, кількість документів, узгодженість FTS, дублікати nreg, кеш текстів; вердикт «база здорова» / «є проблеми» з кодом виходу; `--json` і `--out FILE` (таблиця з BOM / JSON без BOM). 3) **Нові клавіші у програмі**: Ctrl+K — одразу до Пошуку з фокусом на запиті; F5 — перезавантажити текст читалки. Обидві — у віконці «клавіші». 4) Тести: 258/258.
- **ENG:** 1) **The web arrivals feed gains an «оновлено: щойно / N хв тому / N год тому / N дн тому» label**: the time of the last successful fetch (manual or auto), beside the counters; backgrounded tabs stay quiet. 2) **CLI `verify`**: an offline database health check — SQLite integrity, foreign keys, document count, FTS consistency, duplicate nregs, text cache; a «база здорова» / «є проблеми» verdict with a matching exit code; `--json` and `--out FILE` (BOM table / BOM-less JSON). 3) **New desktop shortcuts**: Ctrl+K jumps straight to Пошук with the query focused; F5 re-fetches the reader text. Both listed in the «клавіші» sheet. 4) Tests: 258/258.

---

## 2026-10-29

### 0.10.35: кнопка «Догори» у читалках та вивантаження кешованих тіл у теку / back-to-top in both readers & offline cache dump

- **UKR:** 1) **«Догори» у програмній читалці**: глибоко в довгому акті внизу зліва з'являється плаваюча кнопка з відсотком прочитаного («Догори · 47%») — один клік повертає до назви. 2) **«Догори» у веб-читалці**: кнопка з'являється після 800 px прокрутки і плавно веде вгору; при відкритті нового акта ховається. 3) **CLI `cache --dump DIR`**: офлайн вивантаження **всіх** кешованих тіл документів у теку файлами `{nreg}.txt` (кешовані редакції — `{nreg}.{редакція}.txt`), кожен з BOM; «кеш текстів порожній» — якщо немає що експортувати. Мережі не чіпає. 4) Тести: 253/253.
- **ENG:** 1) **A «Догори» button in the desktop reader**: deep in a long act a floating button appears at the bottom-left showing reading progress («Догори · 47%») — one click jumps back to the title. 2) **The web reader gains its own «догори»**: the same navigation relief — it appears past 800 px of scroll and smooth-scrolls up (hidden again when a new act opens). 3) **CLI `cache --dump DIR`**: offline export of **every** cached document body into a folder as `{nreg}.txt` files (cached editions land as `{nreg}.{edition}.txt`), each with a BOM; an empty cache just says so. Never touches the network. 4) Tests: 253/253.

---

## 2026-10-28

### 0.10.34: «клавіші» у програмі, зміст документа з CLI та шапка друку у веб / desktop shortcuts sheet, document outline & print masthead

- **UKR:** 1) **Кнопка «клавіші» у верхній панелі програми** відкриває віконце зі всіма справжніми гарячими клавішами (Ctrl+F, Enter/Shift+Enter, Esc, Alt+стрілки/PageUp-Down, Ctrl+Plus/Minus/0); той самий список тепер і в діалозі «Про програму». 2) **CLI `toc <nreg>`**: зміст **кешованого** документа офлайн — Розділи/Глави ліворуч, Статті з відступом; `--json` і `--out FILE` (таблиця з BOM / JSON без BOM); мережі не чіпає. 3) **Шапка друку у веб**: надрукована сторінка тепер має офіційну шапку — назва, «реєстр. № …», «редакція від дд.мм.рррр» (якщо обрано) та «URL: … · надруковано: дд.мм.рррр». 4) Тести: 247/247.
- **ENG:** 1) **A «клавіші» button in the desktop topbar** opens a sheet listing every real shortcut (Ctrl+F, Enter/Shift+Enter, Esc, Alt+arrows/PageUp-Down, Ctrl+Plus/Minus/0); the About dialog now shows the same list. 2) **CLI `toc <nreg>`**: the outline of the **cached** document, fully offline — Розділи/Глави flush left, Статті indented; with `--json` and `--out FILE` (BOM table / BOM-less JSON); never touches the network. 3) **A print masthead in the web UI**: the printed page gains the act title, «реєстр. № …», «редакція від дд.мм.рррр» (when an edition is chosen) and «URL: … · надруковано: дд.мм.рррр». 4) Tests: 247/247.

---

## 2026-10-28

### 0.10.33: «клавіші» у веб, пошук по анотаціях з CLI та «цилата» у читалці / shortcuts popover, annotation search & reader citation

- **UKR:** 1) **Кнопка «клавіші» у веб**: списочок усіх справжніх гарячих клавіш (Ctrl+F, Alt+←/→, ←/→ у списку, Enter/Shift+Enter, Esc) — закривається клавішею Esc або кліком поза ним. 2) **CLI `annot search TERM`**: пошук по всіх анотаціях одразу по трьох полях — текст, вид і номер документа; `--json` і `--out FILE` (таблиця з BOM / JSON без BOM), рядки тієї ж форми, що й `annot list`. 3) **«Цилата» у читалці програми**: кнопка копіює повне посилання — ««Назва» // Офіційний вебпортал парламенту України. Редакція від дд.мм.рррр, URL: …» — у тому самому форматі, що й у вебі. 4) Тести: 241/241.
- **ENG:** 1) **A «клавіші» button in the web UI**: a popover listing every real keyboard shortcut (Ctrl+F, Alt+←/→, ←/→ in the list, Enter/Shift+Enter, Esc) — closed by Escape or an outside click. 2) **CLI `annot search TERM`**: greps all annotations across three fields at once — text, kind and nreg; with `--json` and `--out FILE` (BOM table / BOM-less JSON), rows identical to `annot list`. 3) **A «цилата» button in the desktop reader**: copies the full reference — ««Title» // Офіційний вебпортал парламенту України. Редакція від дд.мм.рррр, URL: …» — in exactly the web format. 4) Tests: 241/241.

---

## 2026-10-28

### 0.10.32: поділ посиланням у веб, grep текстів з CLI та перехід до пошуку з словника / shareable links, body grep & dictionary jump-to-search

- **UKR:** 1) **Посилання на сторінку у веб**: URL з `#doc=номер` одразу відкриває акт, `#q=запит` — одразу шукає; адреса оновлюється без сміття в історії браузера — таке посилання можна копіювати колезі. 2) **CLI `text <nreg> --grep TERM [--context N]`**: рядки тіла з підсвіткою пошуку у стилі grep — «N: рядок», сусідні рядки навколо збігів, «--» між блоками; з `--out` файл отримує відфільтрований текст. 3) **Клік по значенню в пошуку словників на Головна** застосовує відповідний фільтр (стан/тип/видавець) і одразу шукає документи. 4) Тести: 236/236.
- **ENG:** 1) **Shareable links in the web UI**: a URL with `#doc=<nreg>` opens that act directly, `#q=<term>` searches on load; the address updates without polluting browser history — send the link to a colleague. 2) **CLI `text <nreg> --grep TERM [--context N]`**: grep-style body lines — «N: line», N neighbour lines around each match, «--» between blocks; `--out` writes the filtered text. 3) **Clicking a dictionary-search hit on Home** applies that status/type/publisher filter and searches immediately. 4) Tests: 236/236.

---

## 2026-10-28

### 0.10.31: словниковий пошук на Головна, підсвічування в кеш-пошуку та звіт «stats» / home dictionary search, cache-hit highlighting & stats report

- **UKR:** 1) **Пошук у словниках на Головна**: поле під лічильниками словників миттєво знаходить значення стану/типу/видавця за підрядком і показує їх з ідентифікаторами (««Міністерство юстиції» (45)», до 8 на словник + «… ще N»). 2) **Підсвічування у «Пошук у тексті»**: слова запиту тепер виділені й у назвах, і у фрагментах кешованих тіл — тією самою позначкою, що і у звичайній видачі. 3) **CLI `stats`**: офлайн звіт стану одним рядком на джерело — індекс (файл, розмір, імпорт/синк), документи, анотації, три словники, кеш текстів; `--json` і `--out FILE` (таблиця з BOM / JSON без BOM). 4) Тести: 231/231.
- **ENG:** 1) **Dictionary search on the Home page**: a box under the dictionary counters instantly finds status/type/publisher values by substring and lists them with their ids (««Міністерство юстиції» (45)», up to 8 per dictionary plus «… ще N»). 2) **Highlighting in «Пошук у тексті»**: query words are now marked in cache-hit titles *and* snippets, with the same find-in-text marker the results list uses. 3) **CLI `stats`**: a one-line-per-source offline health report — index file/size/last sync, documents, annotations, the three dictionaries, the text cache; plus `--json` and `--out FILE` (BOM table / BOM-less JSON). 4) Tests: 231/231.

---

## 2026-10-28

### 0.10.30: кнопка «цилата» у веб-читалці, час перегляду в «Останніх документах» та картка у файл / citation button, home visit times & card to file

- **UKR:** 1) **«Цилата» у веб-читалці**: кнопка копіює повне посилання — ««Назва» // Офіційний вебпортал парламенту України. Редакція від дд.мм.рррр, URL: …»; для поточної редакції рядок редакції пропускається. 2) **«Останні документи» на Головна показують час відкриття**: «… · щойно / N хв тому / N год тому / N дн тому» (старі записи без мітки лишаються як є). 3) **CLI `card --out FILE`** тепер за спільним стандартом: таблиця — з BOM, `--json` — без BOM, тіло файла й stdout будуються одним кодом, нотатка «[card] {nreg} → {file}». 4) Тести: 225/225.
- **ENG:** 1) **A «цилата» button in the web reader**: copies the full reference — ««Title» // Офіційний вебпортал парламенту України. Редакція від дд.мм.рррр, URL: …»; the edition line is omitted for the current edition. 2) **Home «Останні документи» chips show when each was opened**: «… · щойно / N хв тому / N год тому / N дн тому» (older entries without a stamp stay as they were). 3) **CLI `card --out FILE` now follows the shared standard**: table with a BOM, `--json` BOM-less, file and stdout built by one code path, «[card] {nreg} → {file}» note. 4) Tests: 225/225.

---

## 2026-10-28

### 0.10.29: автооновлення стрічки у веб, офлайн-експорт документа та масштаб читалки з клавіатури / feed auto-refresh, offline document export & keyboard zoom

- **UKR:** 1) **«Автооновлення» в Оновленнях веб**: чип увімкнено — фід освіжається щохвилини, доки вкладка відкрита (у фоновій вкладці не вантажиться); вибір пам'ятає браузер. 2) **CLI `export <nreg>`**: офлайн-експорт **кешованого** тіла документа у `md` (типово) / `txt` / `html` — з заголовком і номером; `--out FILE` пише з BOM, без нього — у stdout; мережі не чіпає взагалі; «документ не кешовано» — підказка прочитати його спершу. 3) **Масштаб читалки з клавіатури**: Ctrl++ / Ctrl+- / Ctrl+0 (крок 10%, 60–250%, зберігається), з підказкою «читалка: N%». 4) Тести: 222/222.
- **ENG:** 1) **«Автооновлення» chip in the web arrivals feed**: once enabled the feed refreshes every minute while the tab is open (backgrounded tabs stay quiet); the choice is remembered by the browser. 2) **CLI `export <nreg>`**: offline export of the **cached** document body as `md` (default) / `txt` / `html` with a title and nreg header; `--out FILE` writes with a BOM, otherwise stdout; it never touches the network, and an uncached document suggests reading it first. 3) **Reader zoom from the keyboard**: Ctrl++ / Ctrl+- / Ctrl+0 (10% steps, clamped 60–250%, persisted) with a «читалка: N%» toast. 4) Tests: 222/222.

---

## 2026-10-28

### 0.10.28: фільтр в Обраному веб, кеш-пошук у файл та час свіжості фіда / web favorites filter, cache search to file & feed freshness time

- **UKR:** 1) **Фільтр в Обраному веб**: поле «фільтр тексту, назви чи номера…» миттєво звужує список (текст анотації, назва та nreg), поруч — «показано X з Y», як у стрічці оновлень. 2) **CLI `cache --search TERM --out FILE`**: повнотекстовий звіт по кешу у файл (з BOM; порожній результат — лише BOM), `--json` — об'єкт без BOM; stdout мовчить. 3) **Час оновлення фіда у програмі**: поряд із лічильником «N документів» — «оновлено: щойно / N хв тому / N год тому / N дн тому». 4) Тести: 216/216.
- **ENG:** 1) **Text filter in web Favorites**: a «фільтр тексту, назви чи номера…» box narrows the list instantly (annotation text, title and nreg) with a «показано X з Y» counter beside it, mirroring the arrivals feed. 2) **CLI `cache --search TERM --out FILE`**: the offline full-text report written to a file (with BOM; an empty result leaves only the BOM), `--json` writes a BOM-less object; stdout stays silent. 3) **Feed freshness in the desktop app**: next to the «N документів» counter appears «оновлено: щойно / N хв тому / N год тому / N дн тому». 4) Tests: 216/216.

---

## 2026-10-28

### 0.10.27: підсвічування запиту у видачі, запам'ятьовані фільтри та анотації у файл / query highlighting, remembered filters & annotations to file

- **UKR:** 1) **Підсвічування слів запиту у веб-пошуку**: збіги в назвах результатів набувають фону пошуку в тексті («giu» + цілі слова, кирилиця враховується); короткі службові слова ігноруються. 2) **Фільтри пам'ятаються**: стан, тип і видавець останнього пошуку відновлюються при наступному запуску (разом із сортуванням). 3) **CLI `annot list --out FILE`**: таблиця анотацій у файл (з BOM; порожній результат — лише BOM), `--json` — масив без BOM; stdout мовчить. 4) Тести: 212/212.
- **ENG:** 1) **Query-word highlighting in the web results**: matches inside result titles get the find-in-text background (whole-word, Unicode-aware; short filler words are ignored). 2) **Filters are remembered**: the status, type and publisher of the last search are restored on the next launch (alongside the sort). 3) **CLI `annot list --out FILE`**: the annotation table written to a file (with BOM; empty result leaves only the BOM), `--json` writes a BOM-less array; stdout stays silent. 4) Tests: 212/212.

---

## 2026-10-28

### 0.10.26: навігація по видачі у веб-читалці, словники у файл та пошук за номером в Обраному / reader result navigation, dictionaries to file & favorites nreg filter

- **UKR:** 1) **«попередній / наступний» у веб-читалці**: документи крокуються списком останньої видачі (і кеш-пошуку) з лічильником «3 / 20»; Alt+ліворуч/праворуч — те саме з клавіатури; кнопки вимикаються на краях. 2) **CLI `dict --out FILE`**: таблиця словників у файл (з BOM), `--json` — без BOM; без `--kind` — усі три словники з окремим рядком-нотаткою на кожен; звіт `--fresh` лишається у stdout. 3) **Фільтр Обраного шукає й за номером**: введіть `1556` — і залишаються анотації документа з цим nreg (текст, назва та nreg — три поверхні пошуку). 4) Тести: 208/208.
- **ENG:** 1) **«попередній / наступний» in the web reader**: documents step through the last results list (cache search included) with a «3 / 20» counter; Alt+Left/Right does the same; buttons disable at the ends. 2) **CLI `dict --out FILE`**: the dictionary table written to a file (with BOM), BOM-less with `--json`; without `--kind` all three dictionaries land in one file with a per-kind note; the `--fresh` report stays on stdout. 3) **The Favorites filter now also matches nreg**: type `1556` and only annotations of that document remain — text, title and nreg are the three search surfaces. 4) Tests: 208/208.

---

## 2026-10-28

### 0.10.25: сортування та групи в Обраному веб, видача пошуку у файл та лічильники на чипах / web favorites sort & groups, search to file & chip counters

- **UKR:** 1) **Обране у веб — як у програмі**: чипи «новіші / старіші / за назвою» (вибір пам'ятається браузером) і «групувати» — заголовки документів із кліком для згортання («— згорнуто») та «розгорнути всі». 2) **CLI `search --out FILE`**: таблиця результатів у файл (з BOM), `--json` — без BOM; фасети лишаються у stdout; без `--out` — без змін. 3) **Лічильники на чипах Обраного**: «усі (9) · коментарі (2) · виділення (2) · закладки (1)» — оновлюються миттєво. 4) Тести: 204/204.
- **ENG:** 1) **Web Favorites now match the desktop page**: «новіші / старіші / за назвою» sort chips (the choice is remembered by the browser) and a «групувати» toggle — clickable per-document headers collapse groups («— згорнуто»), «розгорнути всі» restores. 2) **CLI `search --out FILE`**: the results table written to a file (with BOM), BOM-less with `--json`; facets keep printing to stdout; without `--out` behavior is unchanged. 3) **Favorites chip counters**: «усі (9) · коментарі (2) · виділення (2) · закладки (1)», updated instantly. 4) Tests: 204/204.

---

## 2026-10-28

### 0.10.24: швидкі періоди дати, кнопки на рядках видачі у веб та свіжі фіди за днями / date presets, web hit buttons & dated arrivals filter

- **UKR:** 1) **Швидкі періоди в пошуку**: чипи «7 днів / місяць / рік» біля полів дати ставлять «Дата з» від найсвіжіших завантажених даних без відкриття календаря. 2) **Кнопки на результатах у веб**: у кожному рядку видачі — «копія» (реєстраційний номер у буфер, «скопійовано» на мить) і «сайт» (акт на zakon.rada.gov.ua в новій вкладці). 3) **CLI `arrivals --since N`**: лише документи за останні N днів (дата береться з `edYYYYMMDD` посилання, інакше з тексту публікації; без дати — залишаються); працює з `--json` та `--out`. 4) Тести: 200/200.
- **ENG:** 1) **Quick date presets in search**: «7 днів / місяць / рік» chips by the date fields set «Дата з» relative to the freshest fetched data, no calendar needed. 2) **Per-hit buttons in the web results**: each row gets «копія» (nreg to clipboard, brief «скопійовано» flash) and «сайт» (the act on zakon.rada.gov.ua in a new tab). 3) **CLI `arrivals --since N`**: only documents from the last N days (date derived from the link's `edYYYYMMDD`, else the publication text; undated docs are kept); honors `--json` and `--out`. 4) Tests: 200/200.

---

## 2026-10-28

### 0.10.23: згортання груп в Обраному, режим кеш-пошуку у веб та посилання у файл / favorites group collapse, web cache mode & related to file

- **UKR:** 1) **Згортання груп в Обраному**: клік по золотому заголовку групи (або кнопка «згорнути») ховає рядки цієї групи; з'являється «розгорнути всі». 2) **Режим кеш-пошуку у веб**: після «Пошук у тексті» під полями горить смуга «пошук у кешованому тексті · документів: N» з кнопкою «× звичайний пошук», що повертає звичайну видачу зі сортуванням; звичайний пошук і помилки завжди вимикають режим. 3) **CLI `related <nreg> --out FILE`**: таблиця посилань у файл (з BOM), з `--json` — JSON без BOM; stdout без `--out` не змінився. 4) Тести: 195/195.
- **ENG:** 1) **Collapsible groups in Favorites**: clicking a gold group header (or its «згорнути» button) hides that group's rows, and «розгорнути всі» appears to restore them all. 2) **Cache-search mode in the web UI**: after «Пошук у тексті» a bar shows «пошук у кешованому тексті · документів: N» with a «× звичайний пошук» button that returns to regular results with sorting; a regular search or any error always leaves the mode. 3) **CLI `related <nreg> --out FILE`**: the link table written to a file (with BOM), or BOM-less JSON with `--json`; stdout without `--out` is unchanged. 4) Tests: 195/195.

---

## 2026-10-28

### 0.10.22: порівняння редакцій у читалці, групи дат в Оновленнях та коментарі з файла / reader edition compare, arrivals date groups & comments from file

- **UKR:** 1) **«Порівняти редакції» у читалці**: кнопка під вибором редакції відкриває панель А/В; для кожної редакції — «N слів · M симв. · ~K хв», нижче — «різниця: ±N слів» (тексти беруться з кешу або наживо). 2) **Групи дат в Оновленнях веб**: стрічка розбита заголовками «дд.мм.рррр» (з посилання на редакцію), клік по заголовку згортає/розгортає групу, документи без дати — наприкінці під «без дати». 3) **CLI `annot add --text-file ФАЙЛ`**: довгий коментар з UTF-8 файла (обрізається, помилки «читання {file}» / «файл тексту порожній»); обидва способи одразу — «вкажіть текст або --text-file, не обидва». 4) Тести: 191/191.
- **ENG:** 1) **«Порівняти редакції» in the desktop reader**: a button under the edition picker opens an A/B panel; each edition shows «N слів · M симв. · ~K хв», below — «різниця: ±N слів» (texts come from the cache or live). 2) **Date groups in the web arrivals feed**: the list is split under «дд.мм.рррр» headers (taken from the edition link); clicking a header collapses/expands the group and dateless docs land last under «без дати». 3) **CLI `annot add --text-file FILE`**: a long comment from a UTF-8 file (trimmed, with «читання {file}» / «файл тексту порожній» errors); supplying both forms at once → «вкажіть текст або --text-file, не обидва». 4) Tests: 191/191.

---

## 2026-10-28

### 0.10.21: кешування текстів за запитом, панель посилань у веб та пам'ять сортування / on-demand text caching, web related panel & sort memory

- **UKR:** 1) **CLI `backfill --query "TERM"`**: завантажує й кешує тексти топ- результатів локального пошуку, щоб читати їх офлайн; по одному рядку на документ («[2/10] nreg — 12 345 симв.» / «уже в кеші» / «помилка»), `--dry-run` без мережі, `--json`, `--limit N` (до 25); помилки не переривають процес. 2) **«Пов'язані документи» у веб**: підказка «шукаю посилання…», «посилань на інші акти не знайдено», «не вдалося отримати посилання» з кнопкою «повторити»; повторний клік ховає список, відкриття нового документа скидає панель. 3) **Сортування пошуку пам'ятається**: вибране впорядкування (Назва / Дата / Редакція / Релевантність) зберігається між запусками. 4) Тести: 186/186.
- **ENG:** 1) **CLI `backfill --query "TERM"`**: fetches and caches the texts of the top local-search hits so later reading is offline; one line per document («[2/10] nreg — 12 345 симв.» / «уже в кеші» / «помилка»), `--dry-run` without network, `--json`, `--limit N` (up to 25); per-doc failures never abort the run. 2) **«Пов'язані документи» in the web reader**: a «шукаю посилання…» hint, «посилань на інші акти не знайдено» for empty results, «не вдалося отримати посилання» with a «повторити» retry button; clicking the button again collapses the list and opening another document resets the panel. 3) **Search sort is remembered**: the chosen ordering (Назва / Дата / Редакція / Релевантність) now persists across launches. 4) Tests: 186/186.

---

## 2026-10-28

### 0.10.20: історія запитів у веб, словники з файла та скорочення в «Про програму» / web query history, dictionary from file & shortcut docs

- **UKR:** 1) **«Недавні запити» у веб**: під рядком фільтрів чипи останніх 10 запитів (сховище браузера); клік повторює пошук, «очистити» чистить історію. 2) **CLI `dict --from-file ФАЙЛ --kind K`**: завантаження словника з локального дамп-файла (той самий формат, що й portal .txt: id + табуляція + назва в Windows-1251) без жодного запиту до мережі; **`dict --fresh`** — швидкий звіт «stan: 10 записів / …: порожньо» без перевантаження (і `{"fresh":{…}}` з `--json`). 3) **«Про програму»**: розділ «Гарячі клавіші» — Ctrl+F, Enter, Alt+стрілки, Alt+PageUp/PageDown. 4) Тести: 182/182.
- **ENG:** 1) **«Недавні запити» in the web UI**: chips of the last 10 queries under the filter row (browser storage); a click re-runs the search, «очистити» clears the history. 2) **CLI `dict --from-file FILE --kind K`**: seeds a dictionary from a local dump file (the portal's own .txt format: id + tab + Windows-1251 label) with zero network; **`dict --fresh`** prints a quick «stan: 10 записів / …: порожньо» report without refetching (plus `{"fresh":{…}}` with `--json`). 3) **«Про програму»** now has a «Гарячі клавіші» section — Ctrl+F, Enter, Alt+arrows, Alt+PageUp/PageDown. 4) Tests: 182/182.

---

## 2026-10-28

### 0.10.19: сортування Обраного, порівняння редакцій у веб та офлайн-resolve / favorites sorting, web edition compare & offline resolve

- **UKR:** 1) **Сортування в Обраному**: чипи «новіші / старіші / за назвою» — рядки (і групи) переупорядковуються миттєво, без запитів. 2) **«Порівняти редакції» у веб-читалці**: кнопка біля «Текст» відкриває панель із двома редакціями; для кожної — «N слів · M симв. · ~K хв», нижче — «різниця: ±N слів». 3) **CLI `resolve` тепер офлайн перший**: точний nreg і FTS-пошук назви з локального індексу (сіра лінія «локально»), мережа — лише коли індекс нічого не знайшов; нові `--limit N`, `--json` та `--out FILE` (таблиця з BOM, JSON без BOM). 4) Тести: 179/179.
- **ENG:** 1) **Sorting in Favorites**: «новіші / старіші / за назвою» chips instantly reorder rows (and groups) with no queries. 2) **«Порівняти редакції» in the web reader**: a button by «Текст» opens a panel with two editions; each shows «N слів · M симв. · ~K хв», below — «різниця: ±N слів». 3) **CLI `resolve` is now offline-first**: exact nreg and FTS title lookup from the local index (a muted «локально» line), network only when the index is silent; new `--limit N`, `--json` and `--out FILE` (BOM'd table, BOM-less JSON). 4) Tests: 179/179.

---

## 2026-10-28

### 0.10.18: контекстне меню видачі, бейджі «НОВЕ» у веб та анотації в файл / hit context menu, web new badges & annotations to file

- **UKR:** 1) **Правий клік на результаті пошуку**: меню «Відкрити в читалці / Відкрити на сайті / Копіювати реєстраційний номер / Копіювати назву / Позначити для файлів». 2) **«НОВЕ» в Оновленнях веб**: переглянуті раніше nreg зберігаються в браузері (до 500); свіжі позначаються золотим «НОВЕ», поруч «нових: N» і вимикач «лише нові». 3) **CLI `annot export --out FILE`**: JSON анотацій у файл без BOM (stdout мовчить); стара форма `annot export FILE` працює як раніше. 4) Тести: 174/174.
- **ENG:** 1) **Right-click on a search hit**: a menu with «Відкрити в читалці / Відкрити на сайті / Копіювати реєстраційний номер / Копіювати назву / Позначити для файлів». 2) **«НОВЕ» badges in the web arrivals feed**: viewed nregs are remembered in the browser (cap 500); fresh ones get a gold «НОВЕ» pill, with a «нових: N» counter and a «лише нові» toggle. 3) **CLI `annot export --out FILE`**: annotation JSON written to a file, BOM-less (stdout stays silent); the legacy `annot export FILE` form behaves exactly as before. 4) Tests: 174/174.

---

## 2026-10-28

### 0.10.17: класична світла тема за замовчуванням + фільтр оновлень у веб та перевірка імпорту / light classic Windows default, web arrivals filter & import preflight

- **UKR:** 1) **«Класична» тепер світла тема Windows**: сірий фон, білі панелі, чорний текст, синій акцент Windows — саме вона зустрічає перший запуск (раніше за замовчуванням була темна). «Авто» для темної системи тепер дає темну тему. 2) **Фільтр в Оновленнях веб**: поле «назва, номер чи публікація…» фільтрує завантажений фід без мережі, з «×» і «показано X з Y». 3) **Перевірка імпорту в Обраному**: перед імпортом JSON — вікно «Перевірка імпорту» (скільки записів увійде, скільки битих і чому, перші 5 помилок); імпортується лише після підтвердження. 4) **Картка документа**: рядки «тип» і «видавник» зі словниковими назвами замість id. 5) Тести: 171/171.
- **ENG:** 1) **«Класична» is now a light classic Windows theme**: gray face, white panels, black text, Windows-blue accent — this is what greets the first launch (the default used to be dark); «Авто» on a dark system now picks a dark theme. 2) **Arrivals filter in the web UI**: a «назва, номер чи публікація…» box filters the loaded feed with no network, with a «×» clear and «показано X з Y». 3) **Import preflight in Favorites**: importing a JSON now opens a «Перевірка імпорту» report (entries accepted, records rejected and why, first 5 errors) and imports only on confirm. 4) **Document card**: «тип»/«видавник» rows with dictionary labels instead of raw ids. 5) Tests: 171/171.

---

## 2026-10-28

### 0.10.16: фільтри-чипи у веб, звіт статистики та кнопка «сайт» / web filter chips, stats report & site buttons

- **UKR:** 1) **«Активні фільтри» у веб**: після пошуку рядок чипів із кожним увімкненим фільтром («Статус: Чинний ×», «Дата від: 2024-01-01 ×», «№ 2041 ×»); клік знімає фільтр і перевиконує пошук, «скинути все» чистить усі (поле заголовка лишається). 2) **CLI `stats`**: офлайн-звіт індексу — документи, розмір, останній імпорт, найновіші дані, словники, кеш текстів і знімок оновлень; `--json` для скриптів; відсутній індекс — не помилка. 3) **Кнопка «сайт» у результатах GUI**: кожен рядок видачі тепер має «копія» і «сайт» — акт відкривається на zakon.rada.gov.ua в новій вкладці. 4) Тести: 160/160.
- **ENG:** 1) **«Активні фільтри» in the web UI**: after a search, a chip row shows every enabled filter («Статус: Чинний ×», «Дата від: 2024-01-01 ×», «№ 2041 ×»); a click clears it and re-runs the search, «скинути все» clears them all (the title box stays). 2) **CLI `stats`**: an offline index report — documents, size, last import, newest data, dictionaries, text cache and the arrivals snapshot; `--json` for scripts; a missing index is not an error. 3) **A «сайт» button on GUI hits**: every result row now carries «копія» and «сайт», opening the act on zakon.rada.gov.ua in a new tab. 4) Tests: 160/160.

---

## 2026-10-28

### 0.10.15: зняття фільтрів одним кліком та фід у файл / one-click filter clearing & feed to file

- **UKR:** 1) **«Активні фільтри» на сторінці пошуку**: рядок чипів із кожним увімкненим фільтром («Статус: Чинний ×», «Дата від: 01.01.2024 ×», «№ 2041 ×»…); клік знімає фільтр і перевиконує пошук, «скинути все» чистить усі. 2) **CLI `arrivals --out FILE`**: табличний перелік (з «новими» і приміткою про обрізання) пишеся у файл UTF-8 з BOM для Excel; `--json --out` — JSON без BOM. 3) Тести: 157/157.
- **ENG:** 1) **«Активні фільтри» on the search page**: a chip row showing every enabled filter («Статус: Чинний ×», «Дата від: 01.01.2024 ×», «№ 2041 ×»…); a click clears it and re-runs the search, «скинути все» clears them all. 2) **CLI `arrivals --out FILE`**: the table listing (new markers + truncation note) is written to a UTF-8-with-BOM file for Excel; `--json --out` writes BOM-less JSON. 3) Tests: 157/157.

---

## 2026-10-28

### 0.10.14: копіювання у веб, CSV оновлень і картка в файл / web copy-button, arrivals CSV & card to file

- **UKR:** 1) **Веб-читалка**: кнопка «Копіювати текст» (з буфером-запасним шляхом) і тихий рядок статистики «43 733 слів · 340 212 симв. · ~110 хв» у панелі експорту. 2) **CSV в Оновленнях**: кнопка «CSV» зберігає показаний (з урахуванням фільтра і «лише нові») перелік у файл UTF-8 з BOM — Excel відкриває кирилицю правильно. 3) **CLI `card`**: тепер показує людський переказ (номер, стан, дати, редакції); `--json` — звичний JSON, `--out FILE` пише картку (JSON чи переказ) у файл. 4) Тести: 155/155.
- **ENG:** 1) **Web reader**: a «Копіювати текст» clipboard button (with a textarea fallback) and a muted stats line «43 733 слів · 340 212 симв. · ~110 хв» in the export bar. 2) **Arrivals CSV**: the «CSV» button saves the currently shown list (filter and «лише нові» applied) as UTF-8 with BOM — Excel opens Cyrillic correctly. 3) **CLI `card`**: now prints a human summary (number, status, dates, editions); `--json` keeps the JSON, and `--out FILE` writes the card (JSON or summary) to a file. 4) Tests: 155/155.

---

## 2026-10-28

### 0.10.13: пошук у словниках, кеш у підвалі та чищення кеша / dictionary search, footer cache stats & cache purge

- **UKR:** 1) **CLI `dict --search TERM`** (і `--json`): фільтр назв словників stan/typs/organs без мережі — локально заповнені словники не перевантажуються (сіра лінія «локально»), групувальні заголовки «stan (стани):» та ін. 2) **Кеш у підвалі веб**: статус-рядок сторінки тепер показує «кеш текстів: N тіл · X.X MB». 3) **Чищення кеша в GUI**: на головній біля рядка «кеш текстів» — кнопка «очистити» з вікном підтвердження (анотації й індекс не чіпаються). 4) Тести: 151/151.
- **ENG:** 1) **CLI `dict --search TERM`** (plus `--json`): a label filter over the stan/typs/organs dictionaries with no network — locally populated dictionaries are not refetched (a muted «локально» line), with «stan (стани):» group headers. 2) **Cache in the web footer**: the page status line now shows «кеш текстів: N тіл · X.X MB». 3) **GUI cache purge**: next to the home «кеш текстів» row, an «очистити» button with a confirmation dialog (annotations and the index are untouched). 4) Tests: 151/151.

---

## 2026-10-28

### 0.10.12: зміст у веб-читалці, файл із CLI та кеш на головній / web reader TOC, CLI text --out, home cache row

- **UKR:** 1) **«Зміст» у веб-читалці**: кнопка «Зміст (N)» над текстом акту; список «Розділ/Глава/Стаття» будується з уже відмальованих заголовків і плавно прокручується до вибраного (схована, якщо заголовків менше двух). 2) **CLI `text --out FILE`**: запис тексту акту (живого чи з кешу) у файл UTF-8 з створенням тек; у консоль — «записано N симв. → файл». 3) **«Кеш текстів» на головній**: рядок із кількістю кешованих тіл і їх об'ємом (кеш поповнюється при читанні). 4) Тести: 148/148.
- **ENG:** 1) **«Зміст» in the web reader**: a «Зміст (N)» button above the act; the «Розділ/Глава/Стаття» list is built from the already-rendered headings and smooth-scrolls to the picked one (hidden below two headings). 2) **CLI `text --out FILE`**: writes the act text (live or cached) to a UTF-8 file, creating parent dirs; the console gets «записано N симв. → файл». 3) **«Кеш текстів» on the home page**: a row with the number of cached bodies and their size (the cache fills as you read). 4) Tests: 148/148.

---

## 2026-10-28

### 0.10.11: фільтр оновлень у пам'яті та фасети в CLI / remembered arrivals filter & CLI facets

- **UKR:** 1) **Фільтр Оновлень зберігається**: введений текст-фільтр стрічки (назва/номер/публікація) запам'ятовується між запусками (settings.ini), кнопка «×» чистить і його. 2) **CLI `search --facets`**: після таблиці результатів друкуються «Фасети:» — Статус/Тип/Видавник з назвами словників і кількістю (top-6), з пом'якшеною помилкою замість падіння; `--json --facets` додає ключ `facets`. 3) Тести: 146/146.
- **ENG:** 1) **The arrivals filter is remembered**: the feed's text filter (title/number/publication) persists across launches (settings.ini); «×» clears it for good. 2) **CLI `search --facets`**: after the hit table the command prints «Фасети:» — Status/Type/Publisher with dictionary labels and counts (top-6), degrading to a muted error instead of failing; `--json --facets` adds a `facets` key. 3) Tests: 146/146.

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
