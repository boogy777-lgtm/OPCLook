## 2026-10-30 — R2.ROUTE/R2.REUSE: источники OPC Foundation и вход LLM

По запросу владельца исследованы публичные OPC UA Part 4/5 и официальный
информационный обзор Classic DA. Подготовлен [routing table](llm-update-routing.md):
триггер -> существующий механизм/владелец -> исходники -> тесты.
Он связан из AGENTS.md; вход не требует восстанавливать обсуждение из чата.

Уточнения перечислены в [OF01–OF12](opc-foundation-gap-review.md):
requested/revised, trigger и смысл timestamps, latest queue/overflow,
Session recovery/replay/gap, raw Write outcomes и unknown timeout, limits/aliases.
Ранее предложенное ограничение DA Subscription + Sync Read снято как перенос
частного UI Kepware: capability и постоянный канал проверяются раздельно.

На вопрос о костыле поверх SDK добавлена [карта владельцев](backend-ownership.md).
R2.0 проверяет locked библиотеку, реальное API/trace и недостающий invariant до
написания кода. Publish/recovery и COM lifecycle нельзя реализовать повторно в
Engine. Общая продуктовая очередь snapshot/read-back/Refresh/Poll остаётся в
существующем scheduler, если SDK её не предоставляет.
DA facade-кандидаты найдены, но не выбраны и не добавлены в зависимости.
Владелец затем прямо потребовал native клиентские библиотеки для UA/DA и
разрешил смену кандидата. [План native-миграции](native-backend-migration.md)
исключает самостоятельное дописывание OPC data-plane; собственный DA слой
подлежит замене, Engine сохраняет только продуктовые политики.

Проверен Cargo.lock: async-opcua и async-opcua-client 0.19.0. Moving docs.rs дал
смешанные версии source/API; точный auto-recovery охват 0.19 не объявлен
проверенным. Полные DA 2.05a/3.00 помечены Foundation Paid Members и не получены;
публичный DA 1.0 обзор не распространяется автоматически на все версии.

Добавлены нормативные O01–O35; G/D CSV не менялись.
Валидация документации: локальные ссылки, реальные пути маршрутов, стабильные
ID, отсутствие прежнего запрета C06, whitespace и согласованность поправок.
Новых продуктовых/live/Windows тестов нет, Cargo/rustfmt недоступны.

## 2026-10-30 — R2.TEST: передача агенту тестов и конфиг соединения

Запрос владельца: таблица истинности для нового чата, push и общий механизм
настроек Poll/Subscription с учётом OPC DA Kepware. «Push vs sub» трактовано
в контексте R2 как Poll vs Subscription; Push не вводится третьим режимом.

Подготовлены [нормативные таблицы](testing/acquisition-truth-tables.md):
G/D в CSV полностью перечисляют по 128 комбинаций; C/U/M задают конфигурацию,
E/S — терминальные исходы и перестановки, T — время, A — adapter obligations.
Это данные задания, не результаты исполнения тестов.

[Конфиг](connection-acquisition.md) хранится по соединению в EndpointSpec;
Engine использует capability и общий scheduler. DA UpdateMode и Sync/Async
Read Method — разные оси: callback завершения Async Read не является stream.
PTC DA Communications также описывает отдельный Read after Write.
Источник DA-кода main проверен: активная группа/items, IOPCSyncIO::Read(Cache),
без готовой data callback доставки. До Device/callback реализации недоступные
режимы должны иметь причину, а не фиктивный переключатель.

Реализация UI/contract/DA/Engine остаётся открытой по R2.3. Старый DA профиль
мигрируется как Poll/Sync/CacheAllowed без автоматического read-back, сохраняя
опубликованный baseline. Новые FreshSource/read-back требуют реальной capability.
Отдельно зафиксированы snapshot профиля на эпоху, явный reconnect и отсутствие
скрытых fallback. Параллельная работа в старых local checkout не изменялась.

Проверены CSV/Markdown/ссылки/whitespace; продуктовые тесты и fmt не запускались:
Cargo/rustfmt недоступны. Live UA/Windows COM/DA в этой сессии не проверялись.

## 2026-10-30 — R2: утверждение механизма и исследование Kepware

Владелец утвердил перенос обновления значений в Engine и сохранение исходных
временных меток. Подготовлен [план R2](value-refresh-plan.md) со стадиями,
инвариантами и приёмкой. Минимальная модель отделяет pending_refresh от ключей
конкретного отправленного Read. Ответ/ошибка старого Read не снимают новую
потребность; произвольный push не подтверждает Write.

Проверены официальные PTC Subscription, Scan Mode и Re-establishing Connections,
а также OPC UA Part 4 Read/Write/DataValue/CreateMonitoredItems/initial notification.
PTC выбирает Exception/Poll как постоянный режим и при этом документирует разовое
восстановительное чтение после reconnect. Поэтому рабочее решение OPCLook:
Subscription по умолчанию, Poll явно; разовые чтения через общий Engine,
без скрытого периодического опроса поверх подписки. Начальное уведомление
получает время на доставку, затем читаются только недостающие ключи.
Отдельно описаны maxAge, происхождение Read/Subscription и отсутствие
доказательства физического действия по серверному read-back.

Текущие poll_only/fallback и фильтрация push в CLI требуют согласования с backend;
одного флага или переноса HashSet недостаточно. Код этим коммитом не менялся.
База main: c4d073bb302447ba621e478f27f2036303dfe3b2; незакоммиченный #16 не доступен.

Проверка документации: whitespace diff, локальные ссылки, Markdown-ограждения.
Cargo/rustfmt недоступны; fmt, сборки и runtime-тесты не выполнялись. Новых
измерений производительности и live UA/DA нет. R2.1–R2.6 остаются открытыми.

## 2026-10-29 — ширина столбцов таблицы

Симптом (владелец): ширину столбцов в таблице тегов нельзя менять мышью, ползунок не ловится. Причина: у `TableBuilder` не был включён `.resizable(true)` (в egui_extras 0.37 по умолчанию `false`), а `Column::initial` сам по себе ресайз не включает — ручки не было вовсе. Решение: включён ресайз; внешний горизонтальный `ScrollArea` по-прежнему прокручивает широкую таблицу, перетаскивание заголовка в центр остаётся перестановкой столбцов, а полоса у правой границы отдана ресайзу (egui_extras регистрирует её после ячейки заголовка). Регрессия `column_separator_drag_resizes_the_column` — настоящие pointer-события: нажатие у правой границы, сдвиг на 60 px, отпускание; ширина должна вырасти. Измерено: fmt; GUI library 230, ui_polish_table 20, interaction 236. Ширины живут в памяти egui на сессию; значения из настроек применяются при следующем запуске/сбросе состояния таблицы. Открыто: issue #15; ручная проверка на реальной Windows-мыши не выполнялась.

## 2026-10-29 — значения после рекурсивного добавления

Симптом (владелец): после «Добавить дочерние» строки появляются, но значения не приходят. Разбор: при активной UA-подписке Engine подавляет периодический `Read` (опрашиваются только poll-only теги), а приложение полагалось на начальное `DataChange`-уведомление сервера. По OPC UA Part 4 сервер обязан прислать начальное значение в первом `Publish` после `CreateMonitoredItems`; на практике часть стеков шлёт только изменения, а медленный первый `Publish` задерживает снимок. Поэтому значение появлялось только при изменении тега. На CODESYS симптом не воспроизводится: 1214 из 1217 строк заполнились за ~8 с без чтения. Решение: после ответа `AddItems` приложение один раз читает подтверждённые (Ok) ключи — `src/app/events.rs`; регрессия `add_items_reply_reads_confirmed_keys_once`. Измерено: fmt; GUI library 229, interaction 236; живой CODESYS — рекурсивное добавление `Objects`: 1265 строк, 1241 со значением за ~3 с после завершения задачи. Открыто: issue #13; реальный DA-сервер и сервер владельца не проверены.

Там же (issue #14): колонка «Время изменения» оставалась пустой, если сервер не отдаёт `SourceTimestamp`, а только `ServerTimestamp`. Маппинг `crates/backend/src/bff/ua/values.rs` теперь берёт `source_timestamp.or(server_timestamp)` (источник — время изменения, серверное — резерв); это влияет и на чтения, и на push. Регрессии `server_timestamp_fills_missing_source_timestamp`, `source_timestamp_wins_over_server_timestamp`. Измерено: backend 270; на CODESYS после фикса время есть у всех строк со значением (1244 из 1265, включая 3 строки с Bad-значением), до фикса — 1229 из 1241.

## 2026-10-28 — Rust 1.99 baseline

Все семь крейтов объявляют минимальный Rust 1.99; локальный инструмент и оба задания CI закреплены на 1.99.1 с rustfmt. Устаревший `Atomic::fetch_update` заменён на `try_update` с сохранением порядка памяти и логики. Измерено: fmt и Windows i686 workspace all-targets typecheck с `--locked --offline` прошли (ресурсы GUI и manifest helper пропущены для typecheck), 2 теста route_service на Linux прошли. Windows runtime и CI точного HEAD определяются внешними проверками; предыдущие результаты Rust 1.95 остаются историческими.

## 2026-10-28 — backend G/H checkpoint

GUID parsing now uses fixed storage, subnet iteration uses inclusive ranges, and UA sessions retain parsed node IDs with keyed monitor lookup. A live sample-server test checks repeated reads, write and invalid ID. UA helper extraction removes the worker/session/io dependency cycle; chunked reads belong to the session. Connection and PKI, request/type loading, item management, and subscription implementation are separated into modules around the owned session. It initially changed Add/Remove no-connection labels; `981725c` restores them, with direct regression and existing live test passing. DA workers own one tagged session while the router retains isolation; routing classification is shared. Contract and network files were split by responsibility without changing public paths. Linux network tests 37, DA router 18, contract 29, locale 11 and compile-fail doctests 3 passed; Windows backend all-target typecheck and Linux CLI boundary check passed. Full Linux backend rerun passed 218/218 including live UA and B15 proxy; Windows i686 workspace all-target typecheck passed with check-only resource/manifest flags. Exact-head CI and real Windows/DA/UAC validation remain open; GUI/Settings H remains open.

## 2026-10-28 — F GUI hot paths

Implemented bounded and cached table/tree rendering, lazy visible-row formatting, virtual journal with full hover/export details, bounded array children, and revision-based row index invalidation including equal-length replacement. Linux optimized headless median CPU wall: static 10k 2206.8 µs, 100 value updates + frame 2462.6 µs, 5000 journal entries 494.5 µs; baseline and p95, allocations, fixture hash and commands are in `docs/reports/refactor-frame-baseline.md`. The figures are measurements of this instrumented host, not GPU FPS or resident memory. Five before/after scenes reviewed, including CJK and helper IPC error. Windows i686 typecheck passed with cross-check flags; Windows runtime, physical DA/UAC and exact-head CI remain open. Remaining backend G and structural H/Settings work remain open.

# Дев-лог

## 2026-10-27 — аудит и дерево серверов UA
- Закрыто: T1–T8 (дерево сервер → URL → безопасность, слияние результатов
  поиска, DiscoveryUrls из FindServers). Измерено: 16 тестов стенда
  (ua_tree, i18n, merge) зелёные; живой FindServers против sample-сервера
  async-opcua вернул 1 сервер, 12 endpoints (4 из них Aes — клиент их не знает).
- Открыто: 35 пунктов аудита (U1–U15, D1–D7, A1–A13).
- Не проверено: полная сборка под `i686-pc-windows-msvc` (нет тулчейна).

## 2026-10-27 — X1: проверка проекта на Linux-хосте
- Закрыто: X1. `scripts/check-linux.sh` зелёный (`fmt`, `check --all-targets`,
  `test --lib`, `test --test '*'`, target `x86_64-unknown-linux-gnu`,
  `CARGO_TARGET_DIR=/home/claude/target-linux`; `.cargo/config.toml` не менялся).
- Измерено: lib 242 passed / 0 failed / 0 ignored; интеграционные 122 passed /
  0 failed / 23 ignored. Ignored: 22 теста `#[ignore]` «нужен живой DA/UA-сервер»
  (были и раньше), 1 новый `cfg_attr(not(windows), ignore)` —
  `network_tab_refresh_button_queries_windows_inventory` (нужен
  GetAdaptersAddresses). Тесты `da_variant` (roundtrip VARIANT/SAFEARRAY) на Linux
  не компилируются вместе с модулем — они только Windows.
- Решения: (1) на не-Windows `bff::da` — заглушка `src/bff/da_unsupported.rs`
  (тот же `spawn`), отвечает `EventKind::Error { «OPC DA поддерживается только в
  Windows (COM)» }` на каждую команду, как и Windows-воркер молчит на
  `Disconnect`/`ConfigureUaSubscription`/`ScanUaNetwork`/`CancelUaScan`;
  (2) `vt_name` вынесена в `bff/vt.rs`, чтобы UI не зависел от COM-модуля;
  (3) `net.rs`, `bff/net.rs`, `bff/mdns.rs`, `da_route.rs` уже имели
  `cfg(windows)`/`cfg(not(windows))`, менялись только атрибуты `allow(dead_code)`.
- Побочный эффект тестов: ui-тесты создают каталог `opclook/pki` в рабочей
  папке (не в `.gitignore`) — не коммитить.
- Не проверено: сборка под `i686-pc-windows-msvc` (нет тулчейна), CI.

## 2026-10-27 — DA: D1, D2, D3, D6, D7
- Закрыто (ветка `fix/da`, по коммиту на пункт; D6 и D7 в одном): D1 — CLSID
  удалённого сервера берётся у удалённого OPCEnum (`IOPCServerList`, затем
  `IOPCServerList2`), затем из кэша обнаружения, затем из локального реестра
  (`bff/da_clsid.rs`); D2 — маршрутизатор `bff/da_router.rs`: поток на сессию и
  на обнаружение (не более 8), `catch_unwind`, ровно один ответ на каждую
  команду в очереди, ограниченный join при выходе; D3 — `CoInitializeSecurity`
  (`RPC_C_AUTHN_LEVEL_NONE`, `RPC_C_IMP_LEVEL_IDENTIFY`) один раз на процесс,
  при `RPC_E_TOO_LATE` причина идёт в журнал; D6 — успех «Теста связи» только при
  `RUNNING`; D7 — «версия сервера X.Y.Z, групп на сервере: N».
- Измерено: `scripts/check-linux.sh` зелёный: lib 264 passed (было 242: +5 `da_clsid`,
  +2 `da_status`, +15 `da_router`), интеграционные 122 passed без изменений; всего 386 passed, 0 failed.
- Решения: при отказе `CoInitializeSecurity` работа продолжается (процесс мог уже
  настроить безопасность до старта потока); неудача создания потока сессии
  отвечает `Error` на `Connect`.
- Не проверено: COM-часть `bff/da.rs` (D1 glue, D2 `DaHost`, D3, D6/D7) не
  компилируется на Linux — проверит CI `i686-pc-windows-msvc`. Тестов для D3 нет.
- Не сделано: A9 (bff/da) — единый формат кодов ошибок затрагивает `model.rs`,
  `i18n.rs` и `ua`; решение за согласованием с агентами `ua`/`app`.

## 2026-10-27 — сведение потоков ua, app, da
- Сведены ветки `fix/da`, `fix/app`, `fix/ua` в `ua-discovery-tree`; конфликт
  только в таблице аудита (статусы объединены построчно).
- Добавлено координатором: K1 — Connect, истёкший на стороне приложения, сразу
  шлёт воркеру `Disconnect` (иначе поздно подключившаяся сессия оставалась на
  сервере; регрессионный тест падал без фикса); CI выводит упавшие
  интеграционные тесты аннотацией.
- Измерено: Linux-гейт 500 passed / 0 failed / 24 ignored. CI под Windows на
  `e29c748` (до слияния ua) — зелёный; на `0fa55b7` один раз падал шаг
  интеграционных тестов при том же коде — причина не установлена (лог
  недоступен), повтор зелёный.
- Открыто: см. «Открыто» в `docs/status.md`.

## 2026-10-27 — поток rest (A9, U15 UI, U13 подписи, F1)
- A9: бэкенд UA/DA больше не строит русские строки: `enum BackendMsg` (158 вариантов,
  подстановки именованные, вложенные причины — `Box`) в `bff/model.rs`; `i18n::backend_text`
  (RU/EN/ZH, исчерпывающий `match`); приложение локализует событие при приёме
  (`app/events.rs`, причина строки — `DataValue.reason`). Тексты библиотек/ОС идут
  подстановками без перевода. Тест-сканер `no_hardcoded_ui_strings` запрещает кириллицу
  в литералах `src/bff/**` вне тестов (сканер научен пропускать сырые строки и
  символьные литералы в тестовых модулях).
- U15: комбинированный поиск — один loopback-запрос; поле «LDS-хост» (`host[:порт]`,
  в `settings.json`); `SearchUaNetwork { bind, lds }`, `LdsTarget` со своей привязкой.
- U13: подпись «…(только на эту сессию)» и подсказка во всех трёх языках.
- F1: настройки UI-тестов — на поток (`settings::thread_dir`), PKI — на процесс
  (`use_private_pki_dir`), создание клиента под замком (`build_client`).
- Измерено: Linux-гейт 500 → 522 passed, 0 failed, 24 ignored.
- Не проверено компиляцией: Windows-части A9 (см. «Открыто» в `docs/status.md`).

## 2026-10-27 — CI под Windows на PR #4
- Аннотации CI теперь снимают ANSI-цвета cargo и показывают хвост лога, если
  шаблоны не совпали (ba6f807) — без этого ошибка компиляции не была видна.
- A9, Windows-часть: `da.rs:578` клал `String` в `Result<_, BackendMsg>` (E0308)
  — 20e5291.
- F1/D3: `CoInitializeSecurity` один раз на процесс (0e475a4). Каждый `Gateway`
  вызывал его заново; второй получал `RPC_E_TOO_LATE` и писал строку журнала,
  которую ловил `gateway_shutdown::send_after_shutdown_returns_error` в зависимости
  от порядка старта двух параллельных тестов. Регрессионный тест — сам этот
  Windows-тест (код только под Windows).

## 2026-10-27 — решения владельца
- Сертификаты, найденные при поиске серверов, остаются в общем `trusted` (без изменений в коде).
- Сборный коммит `8319e34` остаётся; история `main` не переписывается.

## 2026-10-27 — отзыв владельца: теги по endpoint, клавиатура, структуры (V1, V2, K3, V3)
- Закрыто: V1 — таблица показывает теги активной вкладки (каждый endpoint — свой
  список); V2 — UA-сессия = URL + политика + режим, вкладка показывает безопасность;
  K3 — окно записи открывается с выделенным значением, Tab/F6, Ctrl+Tab,
  Ctrl+PgUp/PgDn, Ctrl+N, исправлен Ctrl+A → Delete после фокуса в дереве; V3 —
  структуры UA раскрываются в дерево полей.
- Решения: (1) без активной вкладки таблица показывает все строки — строка не
  пропадает без вкладки-владельца; (2) Tab забирается у egui только когда клавиатурой
  владеет дерево или таблица, иначе egui ходит по виджетам как раньше; (3) типы
  структур грузятся один раз после подключения, только из пространств имён ≠ 0
  (стандартные типы клиент знает сам), бюджет 30 с; отказ — строка в журнале,
  структуры остаются текстом; (4) запись структур по-прежнему не поддерживается
  (понятная ошибка с именем типа).
- Измерено: регрессионные тесты падали на `main` до исправления — выделение
  («4277» вместо «77»), повтор после ошибки («42abc»), Ctrl+A+Delete (осталось 2
  строки); живой тест V3 без загрузки типов не получает `Value::Struct`. Linux-гейт
  534 passed (lib 381, интеграционные 153), 0 failed, 24 ignored. PNG таблицы с
  раскрытой структурой и двумя вкладками одного сервера просмотрен.
- Не проверено: Windows-сборка `da_variant.rs` (новая ветка `Value::Struct` в
  `default_vt`) — проверит CI; серверы до OPC UA 1.04 (без DataTypeDefinition) —
  структуры у них остаются текстом.

## 2026-10-27 — P1: теги большого дерева перестают обновляться
- Воспроизведено: живой тест, 2000 тегов пачками по 100 (как «добавить
  дочерние»): после 1000-го сервер отвечает `BadTooManyMonitoredItems` на каждый
  вызов CreateMonitoredItems — лимит элементов на одну подписку. Клиент держал одну
  подписку на сессию; такие строки показывали ошибку и, пока подписка активна, не
  опрашивались — значение появлялось только после ручного чтения. «Плавающий»: зависит
  от размера дерева и лимитов конкретного сервера.
- Решения: (1) сверх лимита — следующая подписка с тем же интервалом и обработчиком
  (не более 32 на сессию); (2) `BadTooManyOperations` — размер вызова делится пополам и
  запоминается для сессии; (3) если новая подписка не создаётся или ничего не берёт —
  теги «только опрос»: событие `PollOnly`, строка в журнале, приложение читает их
  каждым интервалом опроса и при активной подписке; переподключение решает заново;
  (4) удаление и смена интервала/зоны нечувствительности — по каждой подписке.
- Измерено: тест на 2000 тегах падал до исправления (ошибка на ключах 1001–2000);
  после — 1500 тегов через две подписки, 300 тегов при лимите 30 на вызов, 300 при
  одной подписке на 200 (100 «только опрос», сообщены) — все зелёные.

## 2026-10-27 — UI1–UI10: полировка интерфейса (три потока)
- Закрыто: UI1–UI10 (подробности и цифры потоков — `docs/reports/ui-table.md`,
  `ui-nav.md`, `ui-theme.md`). Таблица: плашки качества, нейтральные значения, числа
  справа, подсветка изменения ~1 с, нарисованный треугольник раскрытия, пустая таблица с
  действием. Вкладки/левая панель/дерево: вкладки с подчёркиванием и замком безопасности,
  панель иконок, мягкое выделение сервера, пустое дерево с «Обновить (F5)». Тема/шкала:
  светлая/тёмная/как в системе (настройка), шкала 12/13/15 и отступы 8×6, журнал по
  умолчанию, панель скрипта компактнее.
- Сведение: конфликты только в `docs/audit-2026-10-27.md` (строки пунктов объединены по
  id) и в инициализаторах `Row` (`poll_only` из P1 + `changed_at` из UI4 — оба поля).
  После сведения шаг строк таблицы вырос до 30 px (строка 24 + общий зазор 6) —
  таблица держит свой зазор 2 px (`TABLE_ROW_GAP`).
- Решения потоков, меняющие поведение: `show_log` по умолчанию включён; F5 на пустом
  дереве обновляет корень; «×» неактивной вкладки виден по наведению; тема применяется
  по «Сохранить».
- Измерено: PNG сводного окна в тёмной и светлой темах просмотрены. Цифры гейта —
  в `docs/status.md`.
- Не проверено: Windows-сборка — CI; перетаскивание на пустую таблицу с кнопкой — только
  по построению и старым DnD-тестам.


## 2026-10-28 — начат первый этап Rust-ревью

Проверены находки B1–B20 и оба P; исправления и ограничения перечислены в
`refactoring-status.md`. Измерено на Linux: 398 тестов библиотеки прошли, один
диагностический отфильтрован; integration binaries прошли с отмеченными ignored.
Новый тест TrustLease прошёл отдельно. Windows i686 проверен `cargo check --tests`
с явным пропуском ресурсов/тестового манифеста; исполнение под Windows и COM/UAC
остаются для CI/стенда. Helper теперь проверяет PID обоих концов pipe, SID клиента
и использует BCrypt для имени; незавершённое ограничение времени IPC записано в
статусе. Перенос слоёв ещё не выполнен.


## 2026-10-28 — начало переноса слоёв

Текущие исходники backend перенесены в отдельный крейт вместе с исправлениями
этапа B1–B20; контракт и локализация отделены, Engine обслуживает оба клиента.
Компиляция Linux тестовых целей и отдельного CLI проверена. Измерено: Engine
4 passed, CLI unit 2 passed, GUI `ui_interactions` 138 passed, 1 ignored;
полный гейт и живой CLI ещё ожидают проверки. Блокирующая подготовка DA-маршрутов и helper
остаётся открытой задачей, как и остальные этапы `refactoring-plan.md`.


### 2026-10-28: граница DA и CLI

Маршрутные описания перенесены в contract, платформенный код — в backend;
перенос не устраняет синхронную подготовку в GUI. Измерено на Linux: CLI 4
и Engine 5 тестов прошли; 12 route-тестов backend прошли.
Запись JSON ограничена по фактическим сериализованным байтам; закрытый канал
stdout завершает клиент без диагностической ошибки. Незатребованная ошибка
чтения больше не освобождает слот ожидаемого ответа. CI для `2a6d017`
прошёл на Linux и Windows; аппаратный DA/UAC не проверен.

Helper exchange ограничен 3 секундами и 16 КиБ; потеря pipe инвалидирует
helper-owned leases. Измерено: 14 Linux route-тестов прошли; Windows i686
backend `cargo check --tests` прошёл, реальный pipe/UAC/DA не запускался.

GUI DA discovery/connect переданы Engine и ограниченному route service;
ошибки локализуются из типизированных событий. Измерено на Linux: GUI lib 186,
Engine 11, locale 9 и тест реального кадра App::update прошли. Проверка
реального Windows pipe/UAC/DA и живого CLI остаётся открытой.

Измерено на Linux: 2 live subprocess-теста CLI прошли против sample UA;
проверены browse/read/write/read, три принудительных poll, push после 3 секунд
покоя при timeout 1 секунда, Ctrl+C, отсутствие секретов в ошибке и BrokenPipe.

Таймауты GUI-запросов формирует Engine через типизированные события; поздние
ответы отбрасываются. Измерено: Engine 12, GUI lib 186 и locale 9 прошли.

Engine подтверждает AddItems по результатам каждого ключа и не опрашивает
ошибочные или просроченные добавления; ранний PollOnly и удаление до ответа
покрыты регрессиями. Измерено: 16 тестов Engine и 2 live CLI-теста прошли.
Повторный полный гейт на текущем head выполняется.

Измерено для `a7d07ab`: полный Linux gate прошёл; Windows i686 workspace
`cargo check --all-targets` прошёл с test-only пропуском ресурсов/манифеста.
Финальный Windows CI и реальный DA/UAC ещё не выполнены. PNG GUI не рендерился;
его проверка и настоящий 10k frame/allocation benchmark переданы следующему
UI/performance этапу. Grace-дренирование того же future при timeout сохранить.


2026-10-28, этап D: guard для COM строк, массивов, VARIANT, статуса и группы;
helper token/LocalAlloc/impersonation и системной библиотеки DNS; Windows
safety lint расширен до изменённых платформенных модулей. Добавлены Windows
регрессии B11/B12/B19 и resource cleanup, пока только typecheck. Локально
измерено: locale 9/9, K1 GUI 1/1, diagnostics `cargo check`; финальный CI
текущего head и внешние DA/UAC проверки открыты. См. матрицу
`docs/refactoring-status.md`.

Этап D продолжен: group guard владеет группой до shutdown/drop; BYREF и
неизвестные/несовместимые layout SAFEARRAY отклоняются, глубина вложенности
ограничена 32. Это ограничения форматов, не реализованная поддержка BYREF.
Измерено на Linux: backend lib 189 passed, 0 failed, 0 ignored. Новые Windows
регрессии проверены типами и ожидают выполнения в CI.

Завершение этапа D: SAFEARRAY передаётся в VARIANT только после успешных
Unaccess/Unlock; Drop повторяет неудачные операции и пытается уничтожить массив.
При постоянном сбое OLE освобождение не гарантировано. Windows fault-тесты
пока только typecheck, выполнение ожидает CI; timeout pipe worker ограничен
временем жизни завершающегося helper процесса.

### 2026-10-28 — тип фильтра Browse

Фильтр классов узлов сделан явным enum от состояния сессии до команды и UA
маски. Выбор Variables сохраняет Objects для доступа к вложенным переменным.
Остальные пункты типизации A/C ещё открыты; полный gate и Windows runtime не
считаются выполненными этой локальной проверкой.

Синтетический Views root использует общий идентификатор и метод contract;
контрактный тест прошёл на Linux, Windows GUI проверен только по типам.

Частичные старые settings загружаются с defaults вместо полной потери файла;
сохранённые интервалы по-прежнему ограничиваются при загрузке. Linux regression
1/1 и Windows GUI typecheck прошли; остальные пункты C остаются открыты.

Порядок колонок типизирован при загрузке; legacy quoted/plain значения,
дедупликация, fallback и формат сохранения покрыты settings тестами (46/46
Linux), взаимодействие диалога проверено отдельно (1/1 Linux). Windows —
только typecheck, полный gate не выполнен.

Качество строки теперь различает серверный код и локальные состояния ожидания,
нет данных и ошибки; коды FFFFFFFE/FFFFFFFD больше не совпадают с плейсхолдером.
Linux UI polish 17/17, stale interactions 5/5, CLI unit 5/5, Windows typecheck;
рендер PNG и общий gate остаются открытыми.

Settings хранит типизированный Lang и сохраняет совместимые коды языка;
старые синонимы и неизвестный код проверены на границе. Linux settings 46/46,
меню языка 1/1 и Windows GUI typecheck прошли.

Сервис DA использует общий Duration refresh; отношения lease и UA keepalive
проверяются при компиляции. Linux тест констант 1/1, Windows typecheck прошли;
прочие временные границы и полный gate остаются открыты.

Сетевой выбор настроек хранит AdapterId/IpAddr и разбирает старые строки один
раз при загрузке. Невалидные значения остаются unselected; stale GUID не
удаляется автоматически. Linux settings 46/46, network-tab 5 passed / 1
Windows-only ignored; Windows проверен по типам.

DA VarType отделён от UA типов и использует именованные константы в GUI,
контракте и VT-таблице backend. Проверены Linux VT 1/1, K3 UI 1/1 и Windows
typecheck; реальный COM runtime остаётся открытым.

Первый шаг общего UaUrl проверяет вход до журнала и сохраняет смысл пути,
query и fragment при сравнении; оставшиеся пути обнаружения и объединения ещё
не переведены. SDK не принимает scoped IPv6 URL; это не заявленная новая
поддержка. Полный gate и Windows runtime остаются открытыми.

Windows CI изолировал heap corruption в тесте некорректного layout: Drop
`VARIANT` в extension windows-rs ранее не был учтён. Исправлены двойное
уничтожение в тесте и владение BSTR при повторном чтении SAFEARRAY; тест
повторного чтения добавлен. Windows runtime исправления ещё ожидает CI.

Общий UaUrl используется в дереве, scanner и LDS вводе; идентичность сохраняет
регистр path/query и учитывает адрес и порт. Loopback LDS aliases разделены по
порту. Linux discovery 12/12, scanner 2/2, U15 8/8, UI grouping 2/2; Windows
workspace typecheck прошёл. Полный parse-once merge и остальные пункты C открыты.

Discovery URL проверяются при получении и хранятся UaUrl, с безопасной
диагностикой отброшенных записей; совпадение сессии учитывает host, порт,
path/query и security. Linux discovery 13/13, scanner 2/2, UI identity 2/2,
grouping 2/2, security-filter 10/10, locale 9/9; Windows typecheck прошёл,
реальный runtime и полный gate остаются обязательными.
Ключ URL сессии хранит валидный identity либо исходный invalid, без двух
взаимоисключающих полей. Linux identity 2/2 и Windows GUI typecheck прошли.

Исправлен знак смещения FILETIME→Unix для DA дат; 0 остаётся отсутствующим
временем. Linux checked-conversion regression 1/1; Windows FILETIME wrapper
прошёл проверку типов, runtime ещё нужен в CI.

Advertised policy/mode представляют отсутствие, известное или неизвестное
значение; неизвестные и неродной `#None` не понижаются, scanner сохраняет
несколько вариантов одного URL. Linux UA 62/62, GUI security 10/10, live CLI
2/2, Windows typecheck и Rust 1.95 contract check прошли. Windows runtime и
оставшиеся этапы открыты.

Interface и IPv6 scope типизированы как NonZero; legacy cache сохраняет
числовую форму интерфейсов, нулевой scope использует индекс IPv6 для link-local.
Linux network 35/35, CLI 6/6, settings 46/46, UI network 5 passed / 1 ignored,
Windows typecheck и Rust 1.95 contract check прошли; runtime Windows открыт.

Reconnect и request floors типизированы Duration, арифметика backoff ограничена;
значения settings/wire остаются совместимыми числами. Linux Engine 18/18,
GUI reconnect 3/3, Windows workspace typecheck прошли; общий gate открыт.

Граница Duration backoff исправлена для публичного API с наносекундной базой
и огромным максимумом; Linux regression 1/1 и Windows engine typecheck прошли.

UA URL валидатор отклоняет запрещённые SDK символы host и знак порта;
Linux parity/security test 1/1 и Windows backend typecheck прошли.

Advertised::Known(Auto), если создан вручную, отвергается в resolver и UI;
Linux regressions 1/1 + 1/1 и Windows workspace typecheck прошли. Компактная
матрица C и последующая передача перечислены в refactoring-status. Зелёный
Windows/Linux CI 557fca20 проверил D hotfix, но не последующие C/FILETIME.

**2026-10-28 — A, ScanId:** ненулевой тип проходит через контракт, GUI и scanner; checked successor выдаёт u64::MAX один раз, затем остановка видима в диалоге и журнале (RU/EN/ZH). Измерено: Linux regression исчерпания 1/1, scanner 10/10, stale scan UI 1/1; Linux all-targets check основных пакетов и Windows i686 workspace all-targets check с test-only флагами прошли. Windows runtime, физические DA/UAC/маршруты и PNG не проверены; прочие роли и Origin/Owner открыты.

**2026-10-28 — A, исчерпание GUI:** генерация session/item прекращается после выдачи максимума; ошибка локализована и видна в статусе и журнале. Частичная партия сигналов сохраняет выданные ключи и отправляется; остаток не создаётся. Измерено: Linux две регрессии исчерпания прошли; Windows i686 workspace all-targets typecheck с test-only флагами прошёл. Типизация ролей и реальный Windows runtime остаются открыты.

**2026-10-28 — A, исчерпание request ID:** Engine проверяет наличие ID до отмены маршрутов и ожидающих запросов при Connect/Disconnect; DA-подготовка возвращает отдельную ошибку исчерпания до создания route service. Linux Engine 19/19 и Windows i686 workspace all-targets typecheck с test-only флагами прошли. Числовой RequestId и sentinel Origin/Owner ещё открыты, runtime/финальный CI не проверены.

**2026-10-28 — A, ItemKey:** ненулевой тип вместо u32 по контракту, Engine и GUI/CLI; UA/DA передают числовой handle через get, входящий UA zero отклоняется с локализованной записью в журнал. Checked successor сохраняет ранее исправленное исчерпание. Compile-fail защищает смешение ItemKey/ScanId. Linux GUI exhaustion и headless удаление строки 1/1 каждое; Linux all-targets и Windows i686 workspace all-targets typecheck с test-only флагами прошли. Реальный DA, UAC, Windows runtime и PNG всё ещё не проверены; SessionId/RequestId/Origin/Owner открыты.

Измерено после ItemKey: Linux backend 199/199, Engine 19/19, CLI 6/6; Windows backend/CLI standalone all-targets typecheck без resource skip прошёл. Первый backend run выявил тестовый fallback, вычислявший недопустимый zero; исправлен и полный набор повторно прошёл.

**2026-10-28 — изоляция PKI тестов:** backend unit tests используют временный каталог до инициализации live fixture, а не рабочий каталог/профиль. Linux regression 1/1 и Windows i686 backend tests typecheck без resource skip прошли. Runtime Windows не проверен.

**2026-10-28 — A, RequestId/Origin:** RequestId отделён от ScanId и не допускает zero, внутренние команды могут быть без токена, Event различает ответ и push. Корреляция остаётся одноразовой, timeout несёт исходный токен; проверены push при pending Read, late reply, DA route и exhaustion, включая сохранение работающего Read. UI при отказе отправки освобождает ожидания и не завершает неотправленные действия; принятая часть write batch учитывается до следующей отправки. Исправлен отсутствующий locale sample InvalidItemHandle (9/9). Измерено: Linux backend 200/200, CLI 6/6 + live 2/2, contract 25/25 + doctest 2/2, Engine 20/20, GUI unit 196/196 и headless UI interactions 139/139 (1 ignored); Windows i686 all-targets typecheck с test-only флагом и standalone backend/CLI без него прошли. Реальный Windows runtime/DA/UAC, PNG и exact-head CI не проверены; SessionId/Owner, B/E/F/G/H остаются открыты.

**2026-10-28 — A, SessionId/Owner:** discovery отделён от реальных ненулевых SessionId во всём стеке; checked successor сохраняет выдачу максимума один раз. Pending/таймауты ключуются Owner, backend отклоняет владельца не того вида, discovery timeout не воздействует на session. Engine test recorder фиксирует фактическую отправку ровно одного внутреннего Disconnect с Owner::Session и без RequestId при таймауте Connect. Исправлен найденный GUI эффект: unsolicited Values/Read Error не сбрасывает pending Read. Измерено Linux: backend 202/202, contract 26/26 + 3 doctests, locale 9/9, CLI 6/6 + live 2/2, Engine 22/22, GUI 198/198, UI interactions 139/139 (1 ignored); Windows i686 all-targets typecheck с test-only флагами, backend/CLI standalone без пропуска ресурсов и diagnostics bins Linux/Windows typecheck. A завершён; B/E/F/G/H, финальный CI, Windows runtime, физический DA/UAC, PNG и 10k baseline открыты.

**2026-10-28 — состояния диалогов B (частично).** Подтверждение владеет
обязательным действием до Yes/No; повторный Enter в кадре открытия и Enter с
модификаторами не согласуют запись. UA identity — enum; введённые имя и пароль
остаются в буферах при переключении, но не передаются для Anonymous/Certificate.
Вкладка Network хранит предыдущий протокол, поэтому при повторном открытии
восстанавливается последняя DA/UA вкладка; настройки и поля переключаемых вкладок
сохраняют прежнее поведение. Измерено Linux GUI 198 unit и 141 interaction
passed, 1 Windows-only ignored; Linux all-targets, Windows i686 workspace
all-targets typecheck с test-only флагами, fmt и CLI boundary на обоих targets
прошли. PNG, Windows runtime и финальный CI ещё не выполнены.

Тест root browse/read гонялся с ответом DA stub и нестабильно падал при
повторном прогоне. Test recorder теперь удерживает отправленный Browse pending
и проверяет единственную команду Browse и отсутствие Read; отдельный тест
прошёл, production scheduling не менялся.

**2026-10-28 — подэтап B2.** UA discovery хранит SingleHost/Network с
RequestId, Auto с непустым Host/Network/Both, отдельное ожидание подтверждения
большой подсети и активный Subnet со ScanId. Новый поиск снимает только свои
pending запросы; DA discovery отдельно отменяет только Discover, сохраняя даже
поставленный в очередь UA timeout. Ошибки и timeout отдельных шагов завершают
Auto без потери частичных результатов. ScanEnd выражает Completed, Cancelled
или Failed; для отмены и ошибки сохраняется реально полученный progress.
Headless тесты покрывают клики подтверждения и остановки, stale events, оба
направления изоляции, отказ отправки, timeout и три исхода scan. Linux backend
UA tests 62/62, contract 26/26 + 3 doctest, CLI 6/6 + live sample 2/2 прошли.
PNG, Windows runtime и финальный exact-head CI не проверены; остальные B/E/F/G/H
и baseline 10k открыты.
Измерено после финальных изменений: Linux GUI unit 204/204, UI interactions
143/143 (1 Windows-only ignored), Engine 22/22; Linux all-targets, diagnostics
bins и boundary для Linux/Windows targets прошли. Windows i686 workspace
all-targets и diagnostics bins прошли только typecheck с test-only пропуском
ресурсов/manifest; standalone backend/CLI all-targets typecheck без пропусков
прошёл. `cargo fmt --all -- --check` прошёл.

### 2026-10-28 — подэтап B3: состояния DA/CSV/Auto

Результаты DA и выбор используют `DaHit` с неизменной host identity; ручной
ProgID и fallback первого найденного сервера сохранены. CSV job различает
running, cancelled и completed; закрытие после отмены сохраняет частичный
импорт. EndpointSpec использует прямые Auto policy/mode, старые отсутствующие,
null и legacy значения принимаются; явный security не понижается. Это только
подэтап B, SessionKind и следующие состояния не затронуты.

Измерено Linux: GUI unit 204/204, UI 144/144 и 1 ignored до последних двух
добавленных DA регрессий; новые DA selection, double-click и session identity
прошли отдельно. Contract compatibility тесты 6/6, Linux GUI/CLI all-targets,
diagnostics, boundary и fmt прошли. Windows i686 workspace all-targets и
diagnostics проверены с test-only флагами, standalone backend/CLI без флагов.
После повреждения локальных metadata очищены только engine/locale target
артефакты и набор повторён. PNG renderer не собрался offline: отсутствует
`wgpu-core-deps-apple`; визуальная проверка остаётся открытой. Windows runtime,
физические DA/UAC, финальный exact-head CI, оставшиеся B/E/F/G/H и 10k baseline
не выполнены.

Повторный полный Linux набор после очистки metadata: backend 202/202,
Engine 22/22, contract 27/27 + 3 doctest, CLI 6/6 + 2/2 live sample.

CSV dialog больше не использует checked `expect`: состояние и счётчики читаются
одним snapshot через `let Some`. Linux interaction отмены и завершения 1/1,
Windows i686 GUI all-targets typecheck с test-only пропуском ресурсов и fmt
прошли; ограничения предыдущей записи сохраняются.

### 2026-10-28 — B4: SessionKind

DA больше не содержит UA View/filter/timeout/subscription; UI получает состояние
подписки и Read из Engine. Endpoint закрыт от прямой мутации, обновление
сохраняет UA выбор для того же протокола, а при смене инициализирует новый
вариант. Тестовая фикстура сохраняет прежнее время последнего опроса.
Измерено: Linux GUI unit 206 passed, UI interaction 148 passed и 1 ignored,
Linux all-targets с test-support и Windows i686 workspace all-targets typecheck
(test-only пропуск ресурсов/manifest); fmt. Windows runtime, DA/UAC, PNG,
финальный exact-head CI, 10k baseline и прочие пункты B/E/F/G/H открыты.

2026-10-28 B5 checkpoint: DA helper lifecycle now owns the pending launcher,
launched process and connected client in distinct states; display status is
computed from that state. A request deadline retains an outstanding launcher
or process for the next attempt, preventing duplicate UAC prompts. Linux route
manager tests: 17 passed, including pending launch, pipe retry, cancellation
and lease loss. Windows runtime, actual DA/UAC, shared IPC and typed errors
remain open; Windows i686 typecheck is reported separately.

2026-10-28 B5/IPC: shared protocol v2 tagged requests validate required fields
and typed IPs while accepting legacy unused null fields. IPC failures retain
kind/detail through the client and detail in route errors, including refresh,
remove and localized RU/EN/ZH output. Spawn failure marks the helper unavailable;
pipe retries retain the process. Linux targeted route 20/20, IPC 2/2, locale
10/10; Windows i686 backend/helper --tests typechecked with helper manifest
suppression for tests only. Earlier full Linux backend 203/203 and contract
29/29 preceded the final targeted tests. Actual Windows execution, DA/UAC,
exact-head CI and remaining B/E/F/G/H work are open.


### 2026-10-28 — B6: JournalMessage

Сообщение журнала теперь либо plain, либо localized с ключом и сохранёнными
аргументами. Убрана заранее форматируемая русская копия; экспорт по умолчанию
остался русским, явный экспорт и рендер используют выбранный язык, импорт
остаётся текстовым. Регрессии проверяют все три языка, экспорт/импорт и одно
форматирование аргумента. Тестовые читатели переведены на `render`.
Windows i686 GUI all-targets прошёл только typecheck с test-only пропуском
ресурсов. PNG, Windows runtime, DA/UAC и итоговый exact-head CI открыты;
оптимизации журнала отложены до измерения baseline F.

Измерено на Linux: GUI unit 208/208, headless UI interactions 148/148
(1 ignored). `cargo fmt --all -- --check` прошёл.


### 2026-10-28 — E core: typed errors and bounded shutdown

Ошибки send и UA-сервисов типизированы и локализованы; LDS возвращает
причину отказа. Поздний UA-ответ дренируется тем же future и остаётся
таймаутом. Shutdown закрывает оба канала до ожидания, не join-ит живой
thread после done, передаёт Joined/Panicked/TimedOut в UI и CLI. DA router
сохраняет панику дочернего join как typed outcome, без повторной паники.
Linux all-targets, Windows i686 workspace all-targets и Windows diagnostics
bins прошли typecheck; Windows resource/manifest флаги только для проверки.
Windows runtime, DA/UAC, live B15 proxy, CSV и exact-head CI остаются открыты.

Измерено: Linux backend 212/212, engine 22/22, locale 11/11; отдельные
shutdown-регрессии 4/4. Диагностические binaries прошли Linux и Windows
typecheck; реальное выполнение Windows не проверено.
GUI unit 209/209, gateway 2/2, UI interactions 148/148 (1 Windows-only
ignored), отдельная регрессия DA discovery cleanup panic 1/1.
Release profile перешёл с `panic = "abort"` на стандартный unwind, чтобы
паники worker могли быть обнаружены; размер бинарника может вырасти (не измерен).
CLI unit 6/6 и live 2/2 прошли на Linux; Windows i686 contract release cfg
подтвердил `panic="unwind"`. Полная release-сборка и её размер не проверены.

**2026-10-28, CSV diagnostics:** malformed quoting, missing fields and empty
item IDs are reported with physical record start lines; reports are capped at
100 with an omitted count. Blank lines remain intentional. Exact `server` +
`item_id` header recognition avoids dropping servers named `server...`.
Unresolved sessions also include the source line. RU/EN/ZH messages cover both
modal and synchronous imports. Linux targeted CSV tests and Windows i686
all-targets typecheck are used; Windows runtime remains unverified.
CSV follow-up: retain at most three import columns while still validating
quoting in discarded columns; tested a 100,000-delimiter row (measured test
input). Linux CSV tests 11/11 and Windows i686 all-targets typecheck passed.
E CI follow-up: the boundary guard rejected the GUI's new direct dependency on
`opclook-contract`, which supplies shared typed errors. Allow that explicit
edge while retaining the other boundary checks. The guard passed locally for
both Linux x86_64 and Windows i686 targets.

B15 live: sample server + bounded TCP proxy задерживает ответ на настоящий
NamespaceArray Read, сохраняя последовательность кадров. Измерено Linux 2/2
за 62.78 с, включая startup/teardown: timeout закрывает сессию без Browse на
проводе, ожидающий Read получает NoConnection; Disconnect прерывает загрузку.
Windows i686 backend all-targets typecheck прошёл, runtime не проверен.

**2026-10-28, F стенд, промежуточная проверка.** Добавлен игнорируемый тест
полного `App::update`: отдельно 10 000 строк без изменений, пакет 100 значений
с обработкой события и 5000 записей журнала; фиксируется медиана/p95 времени,
числа аллокаций и байтов. Синхронизация fixture вынесена из измерения.
Измерено: Linux debug smoke 1/1, видимое обновлённое значение проверено;
Windows i686 typecheck теста, CLI boundary Linux и Windows и fmt прошли.
Debug-цифры не считаются оптимизированным baseline; PNG и релевантные
оптимизированные измерения ещё выполняются. Runtime Windows не проверен.

**2026-10-28, PNG QA:** Linux lavapipe/wgpu, opt-in `ui-snapshots`, пять
PNG настоящего App::update просмотрены: русские и китайские глифы читаемы;
обработанная фиктивная ошибка helper с IPC timeout видна. Тест 1/1; снимки
не являются проверкой Windows helper/UAC или реального DA. Обычная сборка не
включает wgpu через эту feature.

**2026-10-28, F baseline измерен:** оптимизированный Linux test profile opt=3,
полный App::update, 120 кадров после прогрева: 10k статично 2918.9/4472.3 мкс
медиана/p95, пакет 100 Values + кадр 3312.2/6089.1 мкс, отдельный журнал 5000
75120.0/101021.1 мкс. Медианные аллокации соответственно 4472, 4530 и 90968.
Полная методика, байты, ограничения и воспроизводимая команда —
`docs/reports/refactor-frame-baseline.md`. Тест 1/1, включая видимые value/time;
CPU wall, не физический FPS. F/G/H и Windows runtime остаются открыты.


**2026-10-28, H — граница компиляции GUI:** normal dependencies GUI теперь
contract, engine и locale; backend доступен напрямую только как dev dependency
для низкоуровневых интеграционных тестов. Engine явно предоставляет безопасные
сетевые и сертификатные метаданные, VT и типизированную проверку UA security;
Gateway доступен только через test-support. Guard проверяет граф normal
зависимостей. Остальные структурные задачи H и Settings, оптимизации F/G,
Windows runtime, физический DA/UAC и финальный exact-head CI остаются открыты.

Измерено: Linux полный набор приложений и библиотек прошёл (GUI unit 211/211,
UI interactions 148/148 и 1 Windows-only ignored, backend 215/215 включая оба
B15 live, CLI live 2/2); normal GUI+CLI и отдельный CLI построены. Windows i686
workspace all-targets проверен по типам с test-only флагами; normal GUI и
diagnostics, standalone CLI проверены отдельно. Guard прошёл для Linux и Windows.
Windows runtime и ресурсы cross-check не исполнял. Native whole-workspace check
неприменим: Windows-only nethelper на Linux не компилирует windows-future.


### 2026-10-28 — GUI link-loss cleanup

The coordinator clears stale values, loading indicators, subscription projection and outstanding requests through one path on link loss; the engine owns reconnect policy. A rejected worker dispatch regression covers partial writes and UI state. Measured: Linux GUI library 214 passed; Windows i686 workspace all-target typecheck passed (cross-check flags, no Windows runtime). GUI/Settings structural work and exact-head CI remain open.


### 2026-10-28 — settings diagnostics and late data

Load returns settings with notices, saves return typed failures, and the application reports errors immediately; on exit the existing native dialog includes save failures with shutdown errors. Unsolicited values, subscription and poll-only updates from an old link are ignored until connected. Regression tests cover both changes. Measured: Linux GUI 216 library, 149 interaction (one Windows-only ignored), 14 theme and one isolation test passed; Windows i686 all-target typecheck passed with cross-check flags. Settings grouping and other GUI structure remain open.


### 2026-10-28 — grouped settings

View, UA and network preferences are grouped with flattened serialization preserving all legacy keys; existing default, clamp, security, adapter and column migration tests remain in place, with an exact top-level key regression. Measured: Linux GUI 217 library, 149 interaction (one Windows-only ignored), 14 theme tests, and Windows i686 all-target typecheck passed. Other GUI structural work and exact-head CI remain open.


### 2026-10-28 — localized network and save diagnostics

Network control messages use locale keys with technical details retained as arguments; save-blocked is typed and localized, and a failed script save does not report success. Measured: Linux GUI 219 library, 149 interactions (one Windows-only ignored), Windows i686 workspace all-target typecheck passed. Exact-head CI and Windows runtime remain open.


### 2026-10-28 — GUI coordinator

Separated event dispatch by domain, introduced WriteBatch transitions and session accessors while keeping correlation in Engine. Linux 219 library and 149 interaction tests (one Windows-only ignored) passed before the final equivalent protocol lookup simplification; Windows i686 all-target typecheck passed afterward. Exact-head CI remains open.


### 2026-10-28 — UI decomposition and images

Connection results/security controls, tree row rendering, table cell rendering and table action handling now have named components. Measured: Linux 219 GUI library, 149 interaction (one Windows-only ignored), 20 table tests and Windows i686 all-target typecheck passed. Five egui/wgpu Mesa scenes were rerendered and visually inspected in `docs/screenshots/after-h/`; the preexisting F images remain intact. Exact-head CI and Windows runtime remain open.


### 2026-10-28 — selected Clippy fixes

Applied selected clone reuse, redundant clone/borrow removal, derivable defaults and a matching-arm simplification. Measured: Linux GUI 219 library and 149 interaction tests (one Windows-only ignored) passed; Windows i686 all-target typecheck passed with cross-check flags. No zero-warning or Windows runtime claim.


### 2026-10-28 — итог реализации A–H

План A–H и исправления B1–B20 реализованы; актуальные решения, локальные проверки и внешние ограничения сведены в [матрицу](refactoring-status.md). Предыдущие записи фиксируют состояние на момент соответствующих проверок. Снимки до F, после F и после H сохранены; условия и измерения приведены в [отчёте](reports/refactor-frame-baseline.md). Exact-head CI и физический Windows DA/UAC/маршруты требуют отдельного подтверждения.


### 2026-10-28 — план следующей работы

Подготовлен [план для нового чата](next-ui-command-plan.md): тема и доступность, каталог действий, сочетания, палитра команд и локальные UI-компоненты. Это план, не реализация; A–H и базовая версия Rust остаются без изменений.


### 2026-10-28 — P1: палитры и тема

Переключатель в заголовке использует существующие переводы и сохранение, синхронизирует тему открытого черновика и сообщает об ошибке через журнал. Токены охватывают виджеты и явные цвета UI; фон плашки качества уменьшен до 8% для читаемости. Решение: при ширине менее 700 px выносить тему на отдельную строку, сохраняя прежние пункты меню видимыми; таблица на 480 px обрезается и остаётся открытой. Измерено: полный `scripts/check-linux.sh` прошёл на Linux; GUI — 220 unit, 149 interaction (один Windows-only пропущен), 20 table, 19 theme; внешний стенд — 4 unit и 19 theme тестов. Измерения контраста, снимки и границы проверки приведены в [отчёте](reports/p1-theme-validation.md). Windows i686 P1 не скомпилирован и не исполнялся, необходим CI; Windows runtime и реальные DA/UAC/маршруты не проверялись.

### 2026-10-28 — P2: действия, незавершённый этап

Каталог закрепляет три семантических ID и числовых кода, проверяет уникальность и резерв выведенных кодов. Открытие подключения через меню, панель, пустую таблицу, вкладку и клавиатуру идёт через один GUI-путь; toolbar при отключённой сессии сохраняет существующее переподключение. Browse и Write используют типизированный Engine-путь из GUI и CLI. Browse допускается при Connecting для совместимости с очередью worker; Write при Connecting отклоняется до расходования RequestId. Подтверждение записи отменяется с сообщением при смене соединения; после dispatch отозвать OPC-запись не обещаем. Автоповтор Enter и Space не подтверждает и не повторяет запись. Новая диагностика не печатает payload; canary-тесты покрывают отказы GUI/CLI, но маскирование произвольных сообщений сервера полностью не доказано. Измерено: внешний стенд — девять целевых kittest; PNG отключённого действия просмотрен. Первый CLI live прогон при параллельной GPU-сборке завершился timeout, изолированный повтор прошёл. P2 остаётся открытой до выбора и проверки ограничений payload, точного CI и проверки Windows; результаты гейта указаны в статусе.

CLI проверяет не более 64 аргументов и до 4096 байт на аргумент (существующие ограничения); GUI-поле записи и общий API типизированных значений не имеют доказанного общего лимита размера и глубины. Следующее решение должно определить совместимые пределы для текста, бинарных и вложенных значений, обработку превышения до выделения больших структур и тесты на смешанные массивы; до этого пункт остаётся открытым. В GUI известные имя пользователя и пароль маскируются в сообщениях сессии; canary проверяет журнал, но неизвестные секреты и произвольные ответы сервера не покрыты.

Измерено на итоговом коде: `scripts/check-linux.sh` прошёл, включая CLI dependency boundary и live sample UA; backend 218, CLI unit 8 и live 2, contract 30, Engine 25, locale 11, GUI library 220, UI interaction 158 (один Windows-only пропущен), navigation 16, table 20 и theme 19. Внешний стенд с `#[path]` на реальные интерактивные тесты: девять целевых тестов прошли. Предыдущий прогон выявил фикстуру, запускавшую Browse после отключения; второй сценарий теста восстановлен на Connected, после чего повторный гейт зелёный. Точный Windows i686 HEAD и runtime остаются за CI/внешним стендом.

### 2026-10-28 — P2: граница значений OPC Write

Обследованы фактические преобразования: GUI и CLI создают скалярные значения (CLI ограничивает число аргументов 64 и размер каждого 4096 байт); общий `WriteRequest` также принимает массивы и составные значения. UA backend при известном типе узла преобразует элементы к нему и проверяет диапазоны; без известного типа выводит тип элементов. Он отвергает вложенные массивы, составные значения и другие неподдержанные формы в соответствующих путях преобразования. DA поддерживает одномерные SAFEARRAY и вложенные варианты через `VT_VARIANT`; нельзя переносить UA-запрет на DA. Windows COM-код не компилировался и не исполнялся в этой среде.

Обе UA-конверсии используют общую проверку вложенных массивов в backend. Она сохраняет порядок: сначала проверка подписки, затем конверсия и коррелированный `Wrote`; Engine не заявляет раннюю проверку формы в доступности действия. Исправлен только inferred-путь: он теперь вызывает существующий проверяющий `Array::new` и отклоняет элементы разных типов или Empty с `WriteArrayBuildFailed` вместо построения некорректного UA массива. Измерено: новая регрессия на mixed массив падала до исправления и прошла после; отдельные проверки подтвердили точные тип и значения однородного массива, прежний пустой массив и конверсию по известному типу. Live sample UA подтвердил NodeNotSubscribed для неизвестного ключа, WriteMultiDim для известного и обычную запись. Эти проверки не являются доказательством совместимости со всеми серверами.

Решение по размеру остаётся открытым: новый произвольный общий лимит не введён. До закрытия нужны совместимые пределы текста, бинарных данных, числа и глубины элементов для общего API, проверка при построении/десериализации и безопасное освобождение глубоко вложенного входа; GUI/CLI не создают такие составные write payload. Маскирование охватывает известные учётные данные в диагностике, а не неизвестные секреты в произвольных сообщениях. После dispatch отмена записи не гарантируется.

Измерено после исправления UA массивов: полный `scripts/check-linux.sh` прошёл — backend 219, CLI unit 8 и live 2, contract 30, Engine 25, locale 11, GUI library 220, interaction 158 (один Windows-only пропущен), navigation 16, table 20, theme 19; отдельный Linux-стенд с `#[path]` на реальные backend-модули выполнил новую регрессию. Тесты с внешним OPC UA сервером и Windows-only проверки, помеченные ignored, не считаются выполненными. Windows i686 текущего кода не скомпилирован и не исполнялся: требуется CI точного HEAD.
## 2026-10-28 — эпоха подключения

Детерминированная последовательность A → Disconnect → B → отложенное событие A выявила принятие старого состояния подписки. Connect RequestId теперь обозначает эпоху сессии в контракте, UA callback и Engine; старые и непомеченные изменения состояния отклоняются. CLI сохраняет режим принудительного опроса. Живой UA sample подтвердил метки событий двух соединений; сетевая доставка устаревшего callback не воспроизведена. Проверки и платформенные ограничения — в refactoring-status.md.

### 2026-10-28 — интеграция R1 и P2

Эпоха Connect сохраняется в обычной отправке и подготовке DA-маршрута; потеря маршрута при Connecting отвечает на Connect, а при Connected относится к текущей эпохе. Запись недоступна без действующей эпохи, даже если состояние ещё Connected после tracked или untracked Disconnect; проверка не расходует RequestId. Детерминированная регрессия окна падала при удалении этой проверки и прошла с ней. Фикстуры сохраняют текущую эпоху, повторный Connected и ошибка Connect тестируются с реальным коррелированным запросом. При A → B отложенный Disconnected A не отменяет подтверждение записи B, потеря B отменяет его и сообщает пользователю. CLI poll отклоняет push с эпохой и без неё, сохраняя Reply Read.

Измерено: целевые Engine (27 unit и 2 doctest), GUI и CLI тесты прошли, живой UA sample и CLI live прошли в составе промежуточного gate. Один полный запуск выявил устаревшую фикстуру GUI, исправленную через реальный Connect. Последующий gate использовал устаревший GUI test binary и не был зелёным для последнего исходного состояния; после принудительной пересборки целевая регрессия прошла в двух отдельных запусках. Измерено после пересборки: полный `scripts/check-linux.sh` прошёл (backend 220, Engine 27, GUI library 222, GUI interaction 158 и один Windows-only пропущен, CLI 9 unit и 2 live); внешний Linux-стенд подключил настоящие файлы Engine через `#[path]` и выполнил 27 unit и 2 doctest. Windows i686 не скомпилирован и не исполнялся; физический DA не проверен.


### 2026-10-28 — актуализация порядка работ

План обновлён по состоянию интеграционного стека R1/P1/P2: P1 выполнен, P2 остаётся частичным, P3 существует как непроверенный незакоммиченный локальный прототип, P4–P5 не начинались. Для объединения нужны проверки CI точного HEAD; результаты PR #8 и локальных тестов не заменяют Windows CI.

### P2: отмена подтверждения с клавиатуры (2026-10-28)

Исправлен общий диалог подтверждений: при Enter на кнопке «Нет» отмена имеет приоритет перед общим Enter. До исправления headless-регрессия воспроизводила отправку Write; общий диалог также обслуживает Disconnect и RemoveSession. Защита от повторов Enter и Space сохранена. Необратимость уже отправленной записи остаётся прежней.

Ограничения payload и ранняя проверка формы остаются открытыми. Engine не владеет актуальным типом UA и не гарантирует синхронное состояние подписки backend; безусловный ранний отказ может изменить приоритет NodeNotSubscribed. В ручном диалоге введён предел 1 МиБ UTF-8 на значение и 16 МиБ на расчётный размер пачки (байты ввода плюс размер WriteRequest на каждый выбранный элемент, зависящий от архитектуры); переполнение арифметики отклоняется. Проверка выполняется до копирования ключей, разбора и построения значений. Это намеренное новое ограничение GUI: оно допускает значения значительно больше существующего CLI-предела 4096 байт, но не обещает совместимости с ранее неограниченными значениями. 16 МиБ — оценка для предотвращения размножения, а не жёсткая граница фактически выделяемой памяти. Текст уже находится в виджете; выделение памяти для вставки текста не ограничено. CLI сохраняет собственный предел и не использует этот GUI-лимит. Typed payload в других клиентах и ранняя UA-форма остаются открытыми. При прямом вызове apply_write oversized проверяется раньше доступности сессии, так что при одновременном отключении будет показана ошибка размера; в обычном UI кнопка отключена при недоступной сессии. Отказ больших ручных значений выполняется локально до отправки; порядок ошибок внутри backend и обработка допустимых значений не меняются.

Измерено: Linux all-target check, сборка GUI/CLI, CLI boundary и shared/CLI тесты с живым UA прошли; отдельно GUI library 223 и GUI integration (interaction 163, один Windows-only пропущен; navigation 16, table 20, theme 19) прошли. Внешний стенд подключил реальный тестовый файл через `#[path]` и выполнил 14 целевых тестов. Визуально просмотрен PNG подтверждения с фокусом на «Нет». Единый полный gate прервался из-за ошибки архива async-opcua-core-namespace; GUI-тесты отдельно выполнены с временным codegen-units=1 для этой зависимости. Windows i686 не скомпилирован, runtime и физический DA не проверены. P2 остаётся частичным.

### P3: сочетания и фокус в диалогах (2026-10-29)

Каталог содержит переносимый Mod+N; legacy клавиши собраны в локальный реестр с тестом конфликтов и сохранением приоритетов. Реальный порядок событий платформенных IME и нелатинских раскладок не доказан. Во время IME защищены проверенные кнопки, checkbox и выбор типа Write; общая гарантия для встроенных egui ComboBox/DragValue не заявляется. Подробности и сравнение craft-keymap — в [отчёте](reports/p3-keymap-comparison.md).

Выявлены и воспроизведены ошибки клавиатуры: Enter на отмене мог выполнить Write или сохранить настройки; при добавлении сигнала выполнялся submit после отмены; Enter на списке типа мог отправить Write, на списке сессии — добавить сигнал, на checkbox настроек — сохранить. Исправление отдаёт отмене приоритет. Для Write обычный Enter действует в поле значения или на явно выбранной кнопке, Shift/Ctrl+Enter не отправляет; для добавления обычный Enter действует в текстовом поле или на кнопке, выбор сессии не запускает действие. В настройках сохранение теперь требует явной активации кнопки «Сохранить». После событий IME фокус может уйти из поля: новый Enter без выбора поля или кнопки ничего не отправляет. Это намеренное изменение прежних клавиатурных сценариев ради безопасного и предсказуемого фокуса.

Измерено: полный Linux `scripts/check-linux.sh` прошёл; backend 220, GUI library 225, interaction 188 (один Windows-only пропущен), navigation 16, table 20, theme 19; внешний стенд с настоящим `ui_interactions.rs` выполнил 25 P3 тестов. Windows i686 не компилировался и не исполнялся, CI точного HEAD обязателен. Физические IME/раскладки и OPC DA не проверялись; P3 частичен, P2 по-прежнему открыт.

Визуально проверен `docs/screenshots/p3-shortcut-menu-ru.png`: меню показывает Ctrl+N и подписи не перекрывают друг друга. Это снимок Linux/русского интерфейса; он не подтверждает отображение и физическую клавиатуру Windows/macOS.

### P3: соответствие подписей привязкам (2026-10-29)

Каталоговое `ui.connect.open` и его Mod+N — реальное действие; F5 подготавливает контекстный Browse, а Enter/F2 открывают локальный защищённый диалог, после чего отдельно выполняется `session.write`. Подписи для F5, Enter/F2, Delete и переключения панелей теперь используют тот же реестр, что и обработчики. В конфликтном unit-тесте перебираются все действия каталога с заданной привязкой; тест включает заведомо конфликтный пример. Часть компактной навигационной подсказки (стрелки и Shift+Page) остаётся статической, пользовательских переопределений нет. Мнимую эквивалентность пункта «Добавить выбранный» и Enter в дереве удалили: на подписанном узле Enter открывает Write. Headless-тест с положительным контролем hover воспроизвёл прежнюю подсказку до исправления и подтвердил её отсутствие после.

Подстановка подписей проверена в RU/EN/ZH и на ширинах 480/1200; новый `docs/screenshots/p3-shortcut-menu-followup-ru.png` просмотрен. Результаты Windows i686, реальные IME и физические раскладки не установлены. См. [сопоставление](reports/p3-keymap-comparison.md).

Измерено на итоговых исходниках дополнения: полный Linux `scripts/check-linux.sh` прошёл; backend 220, GUI library 226, interaction 190 (один Windows-only пропущен), navigation 16, table 20 и theme 19. Внешний стенд через `#[path]` выполнил обе новые headless-регрессии. Отдельно прошли тесты подписей и конфликтов, а также визуальный рендер. Тест исходной подсказки до исправления падал. Windows i686 не компилировался и не исполнялся, требуются проверки точного HEAD в CI.

### P4 — первый срез палитры

Палитра открывается из меню и использует существующий каталог без новых зависимостей. Контекстные Browse и Write отображаются как недоступные до явного ввода типизированных параметров: фиктивный payload не создаётся. Поиск и запрос доступности не отправляют команды; перед выполнением палитра закрывается и существующий action повторно проверяет доступность. Новый shortcut открытия не добавлялся.

Class: каталоговая команда с устойчивыми ID/кодом; доменная команда без параметров остаётся вне исполняемого среза. Inv: поиск и отображение доступности не исполняют действие, выполнение повторно проверяет его после закрытия модального окна; регрессии проверяют отсутствие отправки и блокировку фонового клика. N+1: новое ActionId требует исчерпывающего выбора названия и модели доступности; при добавлении исполняемого доменного действия необходим явный источник типизированных параметров и проверка подтверждений.

Вход в палитре обрабатывается до остальных диалогов; при поступлении нового диалога палитра закрывается и события клавиатуры этого кадра не передаются фону. Предотвращается подтверждение фоновой Write тем же Enter. Кнопка закрытия сообщения защищена от IME-generated нажатий; намеренный щелчок указателем разрешён. Потерянное событие окончания IME оставляет защиту включённой до явного окончания композиции или потери фокуса окна; это ограничение зафиксировано, вместо небезопасного предположения о её завершении.

Измерено на Linux: 204 GUI interaction теста прошли, 1 Windows-only пропущен; 5 no-hardcoded тестов прошли; 14 целевых тестов прошли во внешнем Linux-стенде через `#[path]`. PNG `p4-palette-ru.png` просмотрен. Полный `scripts/check-linux.sh` и Windows i686 для P4 отдельно не выполнялись; необходим CI точного HEAD.

### P5: локальная кнопка с причиной недоступности (2026-10-29)

Выделено повторение добавления egui-кнопки с необязательной причиной: функция принимает готовую кнопку, разрешение и текст причины, возвращает исходный Response и не знает App, backend или локализации. Адаптер действия и диалог записи по-прежнему сами вычисляют доступность, подписи, политику Enter/Space/IME и отправку команд. Стиль берётся из действующих токенов egui. Для уже существующей причины недоступности сохраняются hover и accessibility description; это не добавляет причины для других состояний. Компонент применяется только к двум повторяющимся путям, отдельный крейт не создавался.

Измерено: полный Linux `scripts/check-linux.sh` прошёл с live UA; backend 220, GUI library 228, interaction 204 (один Windows-only пропущен), navigation 16, table 20, theme 19. Две проверки примитива отдельно прошли во внешнем `#[path]`-стенде: Response сохраняет одиночный/двойной клик, disabled не выполняется и существующая причина отражена в accessibility description. Старые проверки безопасности записи и IME входят в interaction suite. Рендеры `docs/screenshots/p5-button-enabled-ru.png` и `docs/screenshots/p5-button-disabled-ru.png` просмотрены. Windows i686 не скомпилирован и не исполнялся; точному HEAD нужен CI. Это один срез P5, не полное обследование UI.

### Клавиатурные регионы и безопасная запись (2026-10-29)

Class: глобальные области Header, Tags, Servers и Footer; текстовые поля и верхний диалог принадлежат собственной локальной области. Inv: Tab/Shift+Tab/F6 обходят четыре области в одном порядке, клавиатурные действия адресуются активной области, подтверждение по умолчанию выбирает No и Enter выполняет только сфокусированный ответ; проверяется headless сквозными сценариями. N+1: новая область добавляется в `ui/regions.rs` и регистрирует вход и обработку своего локального фокуса; новые диалоги участвуют в выборе верхнего владельца в `ui/dialogs/mod.rs`.

Таблица выбирает активный тег стрелками и прокручивает его в виртуализированном представлении с учётом раскрытых строк. Enter/F2 открывают только сфокусированный тег даже при множественном выделении; явная пакетная запись через существующее действие остаётся доступной. При изменении порядка адресатом остаётся ItemKey; при исчезновении тега фокус снимается. Поле записи получает фокус и выделяет текущее значение. No/Escape не отправляют запись и возвращают фокус к полю. Yes/No переключаются Left/Right, по умолчанию No; смена выбора и Enter в одном кадре не подтверждают действие. При появлении системного сообщения оно имеет приоритет, события кадра смены диалога не подтверждают новый диалог. Импорт CSV при показе сообщения временно приостанавливается и продолжает работу после его закрытия.

Header управляет настоящими меню и их пунктами; Footer предоставляет клавиатурный фокус на вкладках, кнопках и поле Script, статус доступен даже при скрытых нижних панелях. В Servers стрелки выбирают сервер, Enter активирует его, Right входит в дерево, Left из корня возвращается к списку. Пустые регионы остаются в цикле. Tab из редактора Script переключает регион и сохраняет текст. Сочетания в иных текстовых полях и модальные диалоги не перехватываются глобальным обходом.

Это первый срез: физические раскладки, реальный Windows IME, Windows i686/runtime и физический OPC DA не проверены; не заявляется проверка всех локальных контролов GUI. Точные результаты и ограничения приведены в статусе.

Исключение совместимости P1: при открытых Options доступен только селектор темы в шапке; он обновляет действующую тему и черновик. Остальные фоновые действия заблокированы. Открытый popup темы первым обрабатывает свои клавиши, после чего они не передаются Options; во время IME и одновременного pointer press кнопочная клавиатурная активация блокируется. Это сохраняет существующий сценарий P1 и предотвращает фоновое сохранение и закрытие.

Дополнение по множественному выбору: Ctrl+стрелки меняют только курсор, Ctrl+Space переключает выбранность курсора, Shift продолжает диапазон от устойчивого anchor ItemKey, Ctrl+Shift расширяет диапазон относительно исходного множества и позволяет сократить его обратно. Ctrl+A и меню «Выделить все» выбирают только текущую видимую сессию. Отдельная рамка показывает клавиатурный фокус даже на невыделенной строке. Контекстная команда строки и double-click редактируют один тег; пакетная операция названа «Изменить выбранные значения...», а окно показывает число целей. Изменение отбора, структуры или сессии сбрасывает snapshot additive gesture. Смешанные события Delete и изменения отбора в одном кадре не удаляют строки; одновременные стрелка+Enter не активируют старый узел дерева.

Измерено на итоговых исходниках: полный Linux `scripts/check-linux.sh` прошёл; GUI interaction 235 (один Windows-only пропущен), Header/Footer 7, navigation 16, table 20, theme 19. Внешний Linux-стенд подключил исходный `tests/ui_interactions.rs` через `#[path]` и выполнил 36 целевых тестов. Для тестовой зависимости async-opcua-core-namespace использовался временный `codegen-units=1`, debug=0/incremental=0; это параметры проверки, не настройки проекта. Один предыдущий gate встретил timeout CLI Browse против sample UA; одиночный повтор и финальный gate прошли. PNG `keyboard-write-ru`, `keyboard-confirm-no-ru`, `keyboard-unselected-focus-ru` отрендерены и просмотрены. Windows i686 и Windows runtime не проверены локально; требуется CI точного HEAD, реальный IME/раскладки и физический OPC DA требуют стенда.

## Передача после PR #10 (2026-10-29)

[PR #10](https://github.com/boogy777-lgtm/OPCLook/pull/10) слит в `main` коммитом `0e4d4c310cbc20cba24345657b7e3d340c7861dc`. Измерено: Linux и native Windows x86 задания CI прошли для точного HEAD PR `9d05dd2965b6c2081ef33a11cc73aa2edc9188b7`. Локальная ветка была опубликована через GitHub connector с пересозданием коммитов; дерево каждого опубликованного коммита сверено с локальным. Эти результаты не относятся к последующим изменениям документации. Физический DA/UAC/маршруты, реальные Windows IME и средства доступности остаются внешними проверками.

Обновлён README: описаны четыре области клавиатуры, различие курсора и выделения, подтверждение записи и явная пакетная операция; ограничения частичной палитры и GUI payload вынесены к пользовательским сведениям. Исторический пункт плана о незакоммиченном P3 заменён текущим состоянием; оставшаяся работа P2–P5 сохранена открытой.
## 2026-10-30 — R2.1: раннее освобождение Read и аудит SDK

Class: корреляция конкурирующих Read; invariant: завершение одного запроса не
освобождает периодический опрос, пока другой Read ещё ожидает ответа. N+1:
использована существующая таблица Engine requests и вычисление read_pending;
новый scheduler не создан. Defect: accept безусловно сбрасывал флаг после
коррелированного Values/Error. Регрессия через send/accept/due до исправления
падала на ошибочном запуске третьего Read. Исправление не вводит один Read-slot
и не реализует pending demand, read-back, конфиг или полную R2 приёмку.
Измерено: Linux Engine 28 unit и 2 doctest, внешний стенд с реальным файлом
28 unit и 2 doctest, fmt check — прошли. Windows x86 и live DA не выполнялись.

Аудит: исходники locked async-opcua-client 0.19.0 и
[тег 0.19.0](https://github.com/FreeOpcUa/async-opcua/tree/9ad28fc011002398f2e8a95696a50408a14531d9):
SDK event loop ведёт Publish и выполняет Transfer/recreate. При recreate
результат create_monitored_items игнорируется; ручной Republish API не доказывает
автоматическое восстановление пропущенных sequence. Существующий quarantine
таймаута не удалён; проверка gap и live recovery остаётся открытой.

В скачанном `opc_da` 0.3.1 callback входные указатели оборачиваются в
владеющий RemoteArray, а shallow Clone и Drop с CoTaskMemFree делают этот путь
опасным; Windows воспроизведение не проводилось. В
[rust_opc safe facade](https://github.com/Ronbb/rust_opc/tree/da44a99265f9706618bbe506c98f714f2aa46f0b)
0.4.0 есть группы и Sync Read/Write, но публичный client facade не покрывает
Browse, remote activation, subscription/async и сводит успешные HRESULT к ().
[bytehound 0.3.1](https://github.com/bytehound-labs/opc-cli/tree/3ef66a85a3c24592669ff0fc199ea85cd058c4a9)
даёт browse и sync I/O, но TagValue возвращает строковые значение/quality/локальное
время, WriteResult — bool/error, а типы записи ограничены. Эти API не сохраняют
обязательный контракт; кандидаты не добавлены и старый DA путь не удалён.
x86 совместимость и физический DA не подтверждены. Не все кандидаты исчерпаны;
библиотечное исправление или подходящий SDK остаются необходимы.

## 2026-10-30 — R2.1 продолжение: scope до реализации

Class: продуктовые цели чтения и корреляция. Invariant: старый Read не
удаляет потребность после нового Write, не больше одного Read в работе;
снимок Add и read-back проходят существующий Engine scheduler. N+1: расширяется
существующая таблица запросов/диспетчер и `due`, второй scheduler и протокольный
цикл не вводятся. Defect: App отправляет snapshot напрямую, а Engine не хранит
цели и допускает конкурирующие Read. Проверка — реальные send/accept/due,
детерминированное время, GUI/CLI и настоящий Engine Linux-стенд.

Реализован только initial-срез: Engine создаёт pending по успешным AddItems,
объединяет готовые снимки с периодическими Read, а GUI использует Engine tick.
Ранний terminal push закрывает pending, WaitingForInitialData продолжает ждать;
запрет reuse ItemKey внутри epoch и ограниченный ledger защищают регистрацию.
Одноразовый permit отличает reservation после reconnect даже при тех же ключах;
неверный protocol/permit не потребляет текущий. Повторный due не отправляет
второй Read. Начальное ожидание по ключу использует переданный frontend interval,
не revised: backend пока его не предоставляет. Это изменяет время начального
снимка, но прежний periodic fallback при Inactive/Failed/poll_only сохраняется.
Продолжающиеся Add пачки не продлевают grace для старых ключей. CLI обрабатывает
не более 64 накопленных событий между проверками due.

Измерено на Linux: Engine 36 unit + 2 doctest, contract 31 unit + 3 doctest,
GUI library 230, CLI 9 unit + 2 live UA, отдельный Engine стенд 36 unit + 2
doctest — прошли. В одном промежуточном параллельном запуске CLI browse истёк
timeout; изолированный повтор после сборки прошёл. Live тест допускает оба
наблюдения snapshot и позднего push, но требует последующее изменённое значение.
Физический DA и Windows текущего HEAD не выполнялись.

WriteReadback не подключён: UA write сводит Uncertain и Good к Ok(()), а DA
теряет положительные HRESULT, поэтому текущий результат не позволяет надёжно
распознать принятый Good. Общий конфиг, Refresh и корректное завершение каждого
ключа partial reply остаются открытыми, как и Browse fairness, полные таблицы,
native DA, timestamps/gap. Статусы не объявляются сохранёнными этим срезом.
