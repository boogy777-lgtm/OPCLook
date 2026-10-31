## 2026-10-31 — пара28: Async Gateway fixture и namespace source audit

Предыдущий опубликованный exact `5eb9912bdafedf81deb034e9aa49219a73224b1f`
прошёл все9 jobs: [SDK4](https://github.com/boogy777-lgtm/OPCLook/actions/runs/41177513731),
[COM2](https://github.com/boogy777-lgtm/OPCLook/actions/runs/41177513818),
[App2](https://github.com/boogy777-lgtm/OPCLook/actions/runs/41177513808),
[package1](https://github.com/boogy777-lgtm/OPCLook/actions/runs/41177513730).
Это CI совместимости опубликованных research artifacts с неизменённым
production SDK/App, не повторный запуск SDK overlay стендов. Новый Gateway
fixture ниже требует своего следующего exact-head CI.

В test-only native режиме один arm связывает owner, Connect epoch, RequestId
и ItemKey с существующим SDK Async slot. Timeout даёт один Error/TimeoutRead;
последующий retire — cleanup, не повторный результат. Pending Disconnect и
закрытие канала завершают worker, сохраняя живой SDK lease в явном quarantine.
Другой Gateway в подготовленном live fixture читает, пока первая задача held;
следующая эпоха проверяется только после полного retirement прежней.
Allocated token не доказывает SDK/server admission; отдельный barrier подтверждает
вход SDK worker. Production Async остаётся Unsupported, нового owner нет.

Root actual-source/mock54/54, source/stub getter и Linux Rust typecheck нового
теста прошли. Agent no-default-features49/49 и три C++ source/stub guards прошли;
это не Win32 ABI/link/COM. Новый `sdk_async_gateway_live` precompile+nonzero-test
gate включён в оба COM inputs. Actual Windows outcome pending следующего head;
[scope](testing/native-sdk/async-gateway-correlation.md).

[Namespace cache audit](testing/ua-sdk-namespace-cache-audit.md) принят только
по закреплённым SDK исходникам: SDK cache write предшествует adapter post-check.
Потенциальный NewSession interleaving не воспроизведён, candidate не проверен и
не включён в PR. Никакого namespace fix/runtime acceptance не заявлено.
N4/remote/physical Windows/legal и остальные UA/DA capability gates открыты.

## 2026-10-31 — 6c5349c: 9/9; SDK readiness/drain reviewed research

Точный `6c5349c87334860c52a8317f8fdc5e34a9568dd1` прошёл все 9 jobs:
[SDK4](https://github.com/boogy777-lgtm/OPCLook/actions/runs/41176472728),
[COM2](https://github.com/boogy777-lgtm/OPCLook/actions/runs/41176472844),
[App2](https://github.com/boogy777-lgtm/OPCLook/actions/runs/41176472851),
[package1](https://github.com/boogy777-lgtm/OPCLook/actions/runs/41176472772).
Оба actual i686 COM inputs выполнили `sdk_async_values_live` (по 1/1): I4,
BOOL, Unicode BSTR, R8/BOOL/I8 и BSTR arrays, bounded oversize failure с raw
facts; strict fixture cleanup прошла. Остальные native gates также прошли.
Это test-only single-slot probe, не production Async, не все типы/версии/remote.
Unknown-admission exception проверен actual-source моделью, не SDK crash.

Принят отдельный исследовательский SDK readiness/racefix/drain packet:
[proof](testing/ua-sdk-readiness-drain-proof.md). Измерены SDK recovery16/16,
ACK Drop/incarnation2/2, scope predicate1/1, fresh Engine/Gateway15.45с и
instrumented SDK-observed Closed/Complete30.93с. Один SDK guard сохраняется
между фазами; metadata-ready ACK → bounded SDK replay/drain → Reconciled →
один snapshot53.5 → push54.5; старое52.5 встречает закрытый gate, history unknown
сохраняется. Это ReactivatedSession с одной subscription, disposable patches,
без production SDK pin/App change. Отдельный watch может увидеть failure, затем
Cancelled до terminal stream delivery; глобальная exactly-once гарантия не дана.

Открыты NewSession namespace-cache race, stale Connect/late ACK/loss-during-drain
wire cases, multi-subscription, mapped Transfer, secure interleavings, общий
порядок будущего Push/Read и per-ACK statuses. Production Async/default DA/N4,
remote/physical Windows/legal gates не закрыты. Следующая независимая пара:
SDK-owned namespace-cache scope и test-only Async Gateway correlation.

## 2026-10-30 — R2.ROUTE/R2.REUSE: источники OPC Foundation и вход LLM

## 2026-10-31 — 77d8a42: 7/9 jobs; portable fixture and prepared recovery repairs

SDK4 (41163828310), COM2 (41163828217) and Windows App (41163828213) passed.
Package stopped at the notice-test root alias comparison; collector source and
its profile hashes remain unchanged. The fixture now resolves the Cargo.toml
parent before controlled case-entry enumeration and asserts injection happened.
An alias/.. baseline reproduced the exact missing-ValueError failure; repaired
notice14/14, package8/8, inventory4/4 and profile19/19 passed locally.

Linux backend passed264 with one failure and one known ignored probe: metadata
restart did not see Reconnected. A controlled witness kept the replacement
offline until real SDK Stopped, proving that delayed startup can exhaust the
three-attempt retry budget; this does not prove the particular CI failure cause.
The test now builds the replacement server/RSA before the outage and binds the
original port only after observed Lost. Current-nonce/reconnect-revision and
Double/RW metadata assertions remain strict; bounded transition diagnostics are
retained. New full real-source backend265/265 passed, plus the one explicitly
ignored timing-sensitive notification probe (79.98s). Windows exact repair head
still pending. No production behavior or SDK pin changed by these test repairs.


## 2026-10-31 — 08a105d: App2/COM2/source2 success; test-stand repairs

Exact head 08a105da5794b15cd277af84b76d858abbebe6d5 completed all9jobs:6success,
3failure. App Linux/Windows41162662263 and both COM matrix41162662199 passed.
Actual SDK async I4 callback fixture PASS in both variants, followed by strict
cleanup; actual LargeInts SDK success plus bounded codec failure retained typed
HRESULT/quality/FILETIME/VT in both variants (1/1 each). Production Async stays
unsupported: submit/lifetime/full shapes are separate gates. Source probes2
passed. Both core jobs compiled/passed C++ before failing the isolated Rust ABI
stand: new test referenced backend-only model/scalar modules. Runtime FFI test
now asserts raw facts independently; mapper regression remains in scalar tests.
Locally reproduced two E0433 before; rust-stand5/5 and mapper1/1 passed after.

Package stopped before building at a test fixture that created LICENSE/license
as distinct physical files. Windows overwrites one. The repaired fixture uses
one real file and controlled duplicate-name enumeration; production collector
and ambiguity guard unchanged. Windows-like baseline failed; repaired collector
13/13, Windows-like2/2, packager8/8 passed, disabled guard still fails.
These are narrow test repairs; new exact-head CI pending. The prior adbd315
package/read/cleanup result is not a claim that 08 packaged the new notices.


## 2026-10-31 — опубликованный baseline9/9; следующий пакет R2/N2/N1

`adbd315` прошёл все9jobs, включая реальный запуск упакованного x86 CLI и
строгую COM cleanup. Следующий private packet добавляет scoped SDK recovery,
per-key UserAccessLevel/type metadata, sequence/Overflow facts, GUI/CLI requested
vs revised, DA failure facts и проверенные Cargo notice texts. Измерения и
ограничения: [packet evidence](testing/packet-2026-10-31.md). Native Async
production по-прежнему Unsupported; library patches остаются research.
N4/remote/physical Windows/legal gates не закрыты. Новый CI exact packet pending.


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

Полный Linux gate сначала остановился на E0463 для opclook_backend; после
очистки собственных backend/engine артефактов повтор обнаружил два падающих
reconnect UI теста, воспроизведённых и CI 74fa313. Test fixture при Connecting
ошибочно возвращал старые GUI rows в runtime.items. Fixture теперь сохраняет
пустую регистрацию при Connecting, не сбрасывая ledger или permit; тесты
по-прежнему проходят реальные reconnect request/reply и проверяют AddItems.
После исправления полный `scripts/check-linux.sh` прошёл; UI interactions
235 passed, 1 Windows-only ignored. Точную причину E0463 не определяли.

WriteReadback не подключён: UA write сводит Uncertain и Good к Ok(()), а DA
теряет положительные HRESULT, поэтому текущий результат не позволяет надёжно
распознать принятый Good. Общий конфиг, Refresh и корректное завершение каждого
ключа partial reply остаются открытыми, как и Browse fairness, полные таблицы,
native DA, timestamps/gap. Статусы не объявляются сохранёнными этим срезом.

## 2026-10-30 — R2.5/R2.1: Write outcome и наблюдение — до кода

Class: статус операции записи и последующее наблюдение. Invariant: сохранять
raw per-item статус, различать Good, Good_CompletesAsynchronously, Uncertain,
Rejected и неизвестный исход; read-back не доказывает физического действия.
N+1: использовать существующие SDK Write и Engine request/due/permit, без нового
протокольного scheduler или COM слоя. Defect: UA not-Bad преобразован в Ok(()),
DA HRESULT и отсутствие массива результата теряются; App и CLI интерпретируют
Ok как подтверждённое выполнение. Места: contract/event, UA io/worker, DA write,
Engine, App events и CLI. Проверки: O14–O16 и S01–S07, correlation/epoch,
конкретные native статусы и timeout через реальные entry points.


2026-10-30, результат: typed WriteOutcome передаёт raw UA/DA item или service статус и сохраняет Unknown на таймауте после отправки; preflight Err означает неотправленную запись. GUI и CLI используют общую нормализацию. Read-back не включён. Headless egui/wgpu отрисовал три синтетических сценария Rejected, NotSubmitted, Unknown; PNG визуально просмотрены. Первая попытка упала из-за отсутствующего CJK fallback; английские сцены проверены с установленным DejaVu Sans через штатный OPCLOOK_CJK_FONT.

Полный Linux gate завершился exit 0 после `cargo clean --target x86_64-unknown-linux-gnu` и прогона без debug symbols/incremental. Ранее E0463 для opclook_locale при последовательной сборке, причина не установлена; отдельный запуск без явного Linux target не нашёл неустановленный i686 core. Один повтор gate остановлен при заполнении диска. CLI checked deadline защищает от переполнения, post-dispatch ошибка ожидания даёт Unknown.

2026-10-30 R2 demand turn, до кода. Class: R02/R03/R08 product demand lifecycle; Inv: отдельные pending и покрытые Read цели, один permit на эпоху, push не покрывает post-write/explicit, ошибки терминальны, сохранение статусов, без изменения legacy policy. N+1: тот же scheduler/корреляция Engine; UA SDK 0.19 владеет протоколом, существующий DA worker остаётся открытым migration gap. Defect: `pending_initial` и `reserved_read` не представляют очереди WriteReadback/ExplicitRefresh; старый Read и поздняя потребность не связаны покрытием; CLI закрывается после Wrote. Места: crates/engine/src/lib.rs, src/app/runtime.rs/events.rs, crates/cli/src/main.rs, contract; проверка реальными send/accept/due и fake monotonic временем на E04–E16/S01–S07, включая старый ответ/ошибку/timeout и новый Write, push и reconnect. Запрещено включать read-after-write без явно выбранной политики; DA cache не объявлять FreshSource.


Реализован частичный R02/R03/R08: Engine ставит read-back только по matching принятому Wrote с opt-in на исходном Write; новый demand не покрывается старым Read или push. CLI boolean --read-after-write ожидает UA Read и сохраняет отдельно Write outcome и observation, включая ошибку сериализации; DA Cache-only отклонён до Write. Проверки/открытые строки перечислены в acquisition-truth-tables. Целевой запуск дал E0463 opclook_engine при последовательном cargo (причина не установлена); после ограниченного cargo clean и jobs=1 Engine 42+2 doctest и CLI 12+2 live прошли. Дополнительный тест усиленного reservation/Remove прошёл отдельно. Синтетический timeout не является проверкой UA quarantine.

CLI дополнительно проверяет ключ Wrote и покрытие конкретного permit: чужой ключ даёт Unknown, initial snapshot не может считаться read-back. Реальный CLI event-path regression прошёл; его первый compile потребовал исправления тестового импорта SubscriptionState.

Финальный полный Linux gate после wrong-key fix: exit 0; backend 223, contract 33+3 doctest, Engine 42+2, CLI 13+2 sample-server live, GUI library 233, UI interactions 235 и 1 Windows-only ignored; внешний stand 42+2. Профиль debug=0, incremental=0, jobs=1. Предыдущий gate остановлен для исправления correlation и не считается зелёным. Никакой Windows/физический DA live не заявлен.

2026-10-30, до R2.3 кода. Class R01-CONFIG/R02/R08; Inv: validated per-connection acquisition snapshot не меняется глобальными настройками, capability проверяется до Connect и никакая явная Subscription не заменяется Poll, Poll UA не создаёт monitored values, новый Write intent только по opted-in профилю; отсутствие профиля сохраняет legacy поведение. N+1: существующий Engine/UA SDK и backend capability, без новых протокольных циклов; DA Cache worker остаётся временным и не заявляет stream/device. Defect: режим и интервал сейчас глобальные/CLI force_poll фильтрует push после запуска stream, настройки не принадлежат соединению. Затронуты contract EndpointSpec, engine/adapter, CLI и при готовности GUI; тесты реальные entry points, C/U/M, fake time, живой UA и headless UI при изменении. Отдельный Class CI: bounded live fixture/диагностика, первый Browse timeout 15s в Linux run 41014492173, стадия неизвестна; Inv: сохранять проверку реального успешного Browse, deadline и error context, не retry и не считать предположение причиной.

Целевые результаты: contract 34+3 doctest, Engine 45+2, CLI 13, UA sample Poll live с отсутствием stream и реальным Read, App snapshot/dedup, Settings backup, три headless UI и DA double-click — зелёные. PNG UI 900px UA/DA и 720px UA просмотрены; на 720 действия достигаются прокруткой. Первый вариант 900px обрезал кнопки, исправлен адаптивной областью прокрутки; тест короткого окна также подтвердил Cancel. Linux CI 41014492173 на прежнем head: backend 223, CLI unit 13, ошибка Browse timeout 15s; локальный повтор того live случая прошёл. Test budget увеличен до 60s/120s без повторов и изменений assert; добавлен stage context connect/browse, первопричина не установлена.

Финальный Linux gate после viewport/null/IME изменений: backend 225, contract 34+3 doctest, Engine 45+2, CLI 13+2 sample live, GUI library 236, UI interactions 238 и 1 Windows-only ignored; внешний Engine stand 45+2, внешний backend stand UA Poll live — passed. Отдельные PNG 900 UA/DA и 720 UA после прокрутки просмотрены; проверено что Cancel достижим. Runtime frozen до gate; CI нового head ожидается.

## R2.3 — редактор профиля активного соединения: анализ до кода

- Class: изменение конфигурации действующей сессии и корреляция её эпохи (R01/R03).
- Inv: активный EndpointSpec/Engine snapshot неизменяем до принятия нового Connect существующим lifecycle (в DA — подготовкой маршрута); авто-reconnect использует активный snapshot; проверка, сохранение и проверка коллизии предшествуют остановке здоровой сессии; незавершённая Write становится Unknown и не повторяется.
- N+1: соседние пути ручного/автоматического reconnect, отсутствие session, смена эпохи в открытом диалоге, занятые RequestId, коллизия после сохранения, Cancel и ошибка сохранения; проверить через настоящие App/Engine события и GUI взаимодействия.
- Defect: профиль нового соединения уже существует, но редактор активного профиля отсутствует; глобальная перенастройка не должна обходить snapshot. Владелец протокола остаётся SDK; DA migration не входит в этот срез.

R2.3 восстановление: исходный локальный checkout был удалён средой; дерево
восстановлено из GitHub и проверено по tree SHA `763b4615ec2a0b1060489ae885c647d8c56edeb4`.
Измерено: App 5/5, GUI interaction 2/2, RU PNG 900/720 просмотрены; внешний
стенд настоящего Engine 45+2 doctest. Первый полный Linux gate остановился на
`fonts_cjk` из-за ошибочно заданного DejaVuSans как CJK override: он не содержит
китайских глифов. Это ошибка настройки среды, тест не изменён; последующий
gate запускается с реальным CJK шрифтом без override.

Повторный полный Linux gate с системным WQY CJK: exit 0, backend 225, CLI
13+2 sample live, contract 34+3 doctest, Engine 45+2 doctest, GUI lib 241,
UI interactions 240 и один Windows-only ignored. Font tests 2/2, PNG 900/720
перерендерены и просмотрены. Физический DA и Windows локально не проверялись;
Windows x86 остаётся за CI нового commit.

## N1 — проверка публичного API Softing до кода

Class: готовый native DA SDK и граница адаптера (R00/N0/N1).
Inv: SDK владеет COM/data-plane и callback lifecycle; значения, quality,
timestamps и raw per-item статусы не теряются, x86 и закрытие проверяются.
N+1: Browse, Sync/Async, Cache/Device, subscription, session isolation и
соседние callback/cleanup пути рассматриваются вместе; declarations и demo
не являются доказательством рабочего runtime.
Defect: текущий production DA остаётся собственным Sync/Cache worker; готовая
замена пока не доказана. Проверка обращается к публичным исходникам и сборке
SDK без изменения частных механизмов активации.

N1 Softing: публичные headers v4.47.1 проверены Linux g++ syntax-only.
Source-method harness: 14 случаев двух read режимов / семи шаблонов,
исходный SDK 6 read и 6 write-cache mismatches; подготовленный patch 0/0.
Исходный SDK body/fragment компилировались с fake dependencies, а не полным SDK.
Физические Windows/DA, транзитивные лицензии и lifecycle не проверены.
Финальный Win32 core probe подготовлен; его результат необходимо получить в CI.

## N1 CI setup: анализ до исправления

Class: перенос test harness между Windows и Linux, не OPC protocol.
Inv: оба SDK probe применяют один и тот же patch к точному blob; CMake
использует нормализованный путь и не ослабляет сборку или assertions.
N+1: CRLF/LF patch, спецсимволы и пробелы пути, source-method и core job.
Defect: CI run 41061743112 остановился до core-компиляции: Windows patch
не применился; CMake VerifyGlobs получил путь с обратными слешами.

Воспроизведено локально: CRLF patch к точному LF blob даёт git apply exit 1
на строке 982, как в Windows CI. Общий helper нормализует CRLF в LF, сверяет
blob SHA и выполняет `git apply --check` до применения. Тесты проверяют
идентичность результата LF/CRLF и отсутствие изменения при ошибках. CMake
нормализует SDK path до configure/glob; x86 SDK compilation по-прежнему
не выполнена локально.

## N1 Win32 core build: анализ до исправления

Class: изолированная конфигурация сборочного probe и использование API SDK.
Inv: тот же upstream DA/DCOM core и callback test компилируются без изменений
протокольных исходников или обхода SDK; отключение непроверенной capability
не выдаётся за её поддержку в production.
N+1: pointer API, зависимости Watch/XML/Expat и оба варианта SDK patch.
Defect: CI 41062539737 дошёл до компиляции и получил `isNull` C2039 и конфликт
Expat `ELEMENT_TYPE` с WinSDK; вторичные ошибки ещё не классифицированы.

Публичный SDK `SOCmnPointer` предоставляет `isNotNull`; probe переключён на
него. В рассмотренных core исходниках XML-DA ограждён `SOFEATURE_SOAP`,
Expat — `SOFEATURE_CMN_XML`. Изолированный native DA/DCOM probe отключает
только Watch/XML в test stdafx после SDK config. Pinned config preprocessor
check подтверждает DCOM/DA1 и отсутствие SOAP/Watch/XML; полноценная Win32
сборка и классификация остальных ошибок ещё ожидаются в CI.

## N1: SDK common XML и Expat — анализ до исправления

Class: test-only native build closure и совместимость старого Expat с WinSDK.
Inv: DA Browse сохраняет настоящий SDK `getVARTYPE`; Expat компилируется из
точного upstream source с согласованным SDK ABI, без подмены protocol кода.
N+1: символы и calling convention, char width, DTD/namespace, source patch и
оба read/write callback теста; никакое отключение функциональности не считать
доказательством работающего SDK.
Defect: 41063686408 показал зависимость DA Browse от common XML, поэтому
предыдущий DCOM-only config без CMN_XML неверен; Expat/WinSDK конфликт ещё
нужно проверить на реальной Windows сборке.

CI 41063686408 подтвердил зависимость DA Browse от CMN_XML (`getVARTYPE`):
предыдущее отключение common XML было слишком широким и отменено.
Точный SDK Expat собран на Linux тремя C translation units; narrow XML_Char,
namespace, DTD и C-to-C++ linkage прошли. SDK исходники не менялись. Первый
локальный запуск использовал усечённый header, после восстановления blob SHA
повтор прошёл. WinSDK compatibility и оставшиеся ошибки проверит новый CI.

## N1 Win32 host hook — анализ до исправления

Class: SDK sample host integration для форматирования сообщений ресурсов.
Inv: probe использует настоящий Win32 module handle, не подменяет SDK error,
DA protocol или COM; результат linked/runtime всё ещё проверяет CI.
N+1: resource lookup и отсутствие host message table не считаются доказанной
поддержкой локализованных ошибок; SDK source/patch остаются без изменений.
Defect: run 41065211900 скомпилировал native core и Expat, но link завершился
LNK2019: host не предоставил SDK `getInstanceHandle()`.

## N1: SDK lifecycle и диагностика Win32 probe

Class: host SDK lifecycle и диагностика исполняемого test probe.
Inv: `initSOCmn` предшествует созданию entry, SDK владеет COM, entry
освобождается до `exitSOCmn`; исходный код процесса и timeout видимы в CI.
N+1: ошибка initialize, callback fixture или terminate проходит освобождение;
ненулевой код и timeout не превращаются в успех. До нового CI причина падения
не доказана; synthetic callback не проверяет COM advise и физический сервер.
Defect: run 41066061516 скомпилировал и слинковал core, Win32 Expat smoke
прошёл, но core probe завершился до итогов callback; фактический код был
скрыт оболочкой CI. Пример SDK требует initSOCmn/exitSOCmn при SOFLAG_NO_DLLS,
а probe их не вызывал. Добавлены SDK lifecycle и flush фаз; без изменения SDK.

## N1: Win32 direct core — измеренный offline callback

[Run 41067236324](https://github.com/boogy777-lgtm/OPCLook/actions/runs/41067236324),
head `d375f15f4b57837baeec75d1cf3a7c9b843cb96e`: все три native jobs
успешны. Реальный direct SDK core собран и слинкован на Win32; Expat narrow
namespace/DTD smoke прошёл. Исходный SDK offline callback: read setup=1,
correct=0; write setup=1, correct=0; input unchanged=1. С предлагаемым SDK
patch все пять признаков равны 1. Каждый процесс exit 0 и выполнил common/entry
init и cleanup. Это не физический DA или доказательство COM advise/late teardown.

N2: разрабатывать тонкий адаптер на direct SODaC/SOClt/SOCmn; DaSession/TBC
имел только header probe, OTB не подключён. Открыты live/remote I/O, типы,
изоляция сессий, late callback/teardown и лицензия полного closure. Patch
остаётся предложением библиотеки, старый production DA не заменён. Результат
этого исторического head не переносится автоматически на новый docs-only head.


### 2026-10-30 — N2.1 callback facts

Решение: offline ABI возвращает не более 64 фактов последнего callback и
отдельный envelope (в том числе для нулевых элементов); доступ сериализует
хост. Это не доставка истории. Только VT_I4 становится DataValue, HRESULT
остаётся отдельным фактом, failed/unknown не выдаются за успешное значение.
Измерено локально: 3 теста настоящих Rust файлов на Linux. Win32 SDK/ABI
fixture и сквозная передача в Rust требуют проверки CI точного HEAD; живой
сервер, подключение к Engine и полная миграция остаются открыты.

### N2 SDK COM balance — source defect and regression (2026-10-30)

Class R00/R03/R09/R13, O30–O35: SDK владеет COM lifecycle, сессии изолированы,
старые регистрации не меняют новую эпоху. N+1: один зависший сервер не должен
задерживать другой; неподдерживаемое не превращается в успех. Defect: pinned
`SOCltEntry::initialize` сохраняет режим `COINIT_MULTITHREADED` (0), тогда как
`terminate` использует `m_coInit != 0` для решения о `CoUninitialize`, и один
успешный MTA init не балансируется. `softing-com-balance.patch` считает успешные
инициализации отдельно от режима; source blobs проверяются перед применением.
Windows probe ожидает обнаружить baseline leak и после patch отсутствие COM
context на той же thread; baseline leak он балансирует при выходе. Это не live
interop и не доказательство lifecycle произвольного host: Windows CI pending.
Локально измерено: 3 Python patch tests прошли на точных SDK blobs.
`SOCltServer::getWorkerThread` возвращает собственный `m_workThread` каждого
server, а не общий entry worker; live isolation и shutdown остаются открыты.

### N2 direct SDK runtime prototype (2026-10-30)

`native/softing-da/runtime` создаёт SDK host, server, group и item и вызывает
SODaC sync read/write, сохраняя service/item HRESULT, VARTYPE, quality и
FILETIME. `core-probe --live` требует fixture `OPCLook.NativeDaFixture.1`,
`Fixture.Value` начальное I4 42; проверяет device read, write 77, повторную
регистрацию и read. Это пока только I4 prototype, не Rust runtime и не
доказательство interop: Win32 compile и исполнение ожидаются. Для active group
выбран SDK drPolledRefresh: исходный `SODaCComGroup` не создаёт при этом
cyclic sync read (source inspection), Engine остаётся owner Poll. Production
остается legacy до полноценного adapter и live gate.

SDK syncRead/syncWrite непосредственно вызывают COM на вызывающей thread;
вызов с session thread без COM init был бы неверен. Prototype теперь запускает
I/O через SDK `SOCltTask` и `executeTask` на per-server SDK worker, который
сам инициализирует и балансирует MTA. Строгий fixture вызывает I/O на отдельной thread и проверяет, что её COM
context до и после не меняется; Windows может назначить implicit MTA. Win32 compile
и live выполнение этого изменения ещё не измерены.

CMake создаёт статическую SDK bridge library; `softing_runtime.rs` оборачивает
I4 ABI с thread-affine Host, явным close и уникальными регистрациями.
`rust-stand` компилирует настоящий файл и проверяет с mock ABI отклонение
удалённой и чужой регистрации; 4 stand tests прошли локально. Отдельный
`sdk-live` executable предназначен для cross-process fixture и проверяет две
сессии, повторную регистрацию, read/write и cleanup. Win32 link/run pending;
обёртка не подключена к Gateway. До N4 нужны реальные типы, Browse/Discover, Subscription и live proof.

Rust host владеется потоком инициализации, `SharedHost` удерживает entry в
независимых session threads. Entry branch list синхронизирован в SDK; каждая
Session имеет одного caller. При ещё живых ссылках `close` возвращает Busy и
не освобождает SDK под зависшей операцией: это намеренная утечка до выхода
процесса при timeout, а не подтверждённое ограниченное завершение SDK. Локальный
stand проверяет поток с удерживаемым host после Busy через mock ABI; Windows
live isolation/teardown остаются открыты.

SDK `executeTask` может вернуть S_FALSE, если task не поставлена; read output
до dispatch заполняется failed service/item sentinel, поэтому неисполненная
операция не превращается в успешный ноль. I4 write создаётся через SDK
SOCmnVariant::setI4. Проверка Windows compile/live ожидается.

### N2 Gateway SDK preview и process host (2026-10-30)

Feature `native-da-preview` выбирает один DA adapter за Gateway/router; direct
SDK C ABI используется для I4 и CLSID endpoint. Доступные HRESULT записи
сохраняются как per-item или service status, после dispatch отказ даёт Unknown,
повтора Write нет. Другие типы и пока не подключённые SDK browse/discovery
явно отклоняются. Default production не переключён.

Process-level owner thread только управляет SDK entry: новый Gateway повторно
получает тот же host, пока существует session reference; после исчезновения
ссылок owner выполняет terminate на thread и может создать host снова. SDK
per-server worker владеет OPC I/O. Возможное зависание terminate может задержать
новый Gateway, recovery после исключений в C ABI ещё не доказан. Восемь ABI
входов и две SDK task ловят C++ исключения; task устанавливает E_UNEXPECTED,
чтобы SDK отправил completion. Неисполненная Write не превращается в per-item
Rejected: при неизвестном dispatch остаётся Unknown с service HRESULT.

Linux runtime-stand компилирует реальные файлы router, Gateway adapter,
contract и Rust SDK owner с mock channel/ABI; 1 command/event test прошёл.
`sdk_gateway_live` запускается только с явно выбранным feature, без ignore или
условного успеха при отсутствии fixture. Win32 compile/link/live pending, типы,
Unicode, Browse/Discover и Subscription остаются открыты.

Process owner не выполняет idle termination: он переиспользует host между
Gateway до явного финального `finish_native_sdk` и никогда не блокирует новый
Gateway фоновым terminate. Финальный вызов возвращает Busy при живых сессиях,
ожидает terminate ограниченное caller время и завершает owner после успеха
или ошибки. `sdk_gateway_live` вызывает его после shutdown. Production app ещё
не подключает этот финальный API; strict Windows proof pending.

Live server run 41089436547: SDK direct client baseline прошёл cross-process
Read 42/Write 73/Read 73 и shutdown. Следующий `core-probe --live` завершился
exit 32 до Write; финальная печать затирала исходный HRESULT. Добавлены phase
logging connect, caller COM context, Add и Read с raw facts. Причина до
следующего Win32 прогона не установлена; не считать C ABI live зелёным.

`CoGetContextToken == CO_E_NOTINITIALIZED` на новой thread нельзя требовать
безусловно: в процессе может существовать implicit MTA. Строгий live fixture
сравнивает caller context до/после SDK операции, принимая implicit MTA; при
этом Read/Write выполняются на SDK worker. Причина run 41089436547 пока не
доказана; следующий run должен показать phase diagnostics.

N2 SDK scalar ABI переносит signed/unsigned 64-bit без f64 преобразования,
BOOL, I1/I2/I4/I8/INT, UI1/UI2/UI4/UI8/UINT, R4/R8, CY и DATE; запись создаёт
SOCmnVariant только SDK setters, диапазоны проверяются до dispatch. Raw type,
service/item HRESULT, quality/time сохраняются; ARRAY/BSTR и прочие типы дают
явный Unsupported. Linux stand: scalar boundary test и Gateway mock command
test по 1/1. Strict Gateway fixture расширен bool/i64/u64/f64 read/write,
SDK C++ Win32 compile и реальные значения ещё не измерены.

# 2026-10-31 — N2/N3 восстановление сборочных гейтов

Class: совместимость собственных мостов с платформенными заголовками и
сохранение существующего API при выносе общей конверсии. Inv: Win32 SDK
fixture компилируется с исходными заголовками, а legacy DA по-прежнему может
очистить каждый инициализированный VARIANT. N+1: любые WinSDK macro имена в
тестовых локальных переменных и все существующие вызовы общего cleanup.
Defect измерен в CI head `41a5f007`: C++ `small` раскрывается в `char`;
Rust потерял `clear_variant`; rustfmt также отклонил этот head. Изменено имя
тестовой переменной и восстановлена прежняя функция очистки. Существующий
Win32 core build/queue boundary fixture и DA Rust build/variant tests —
регрессионные проверки, которые на дефектном head не компилировались.

Локально: 37/37 Linux stand тестов с mock ABI и реальными адаптерными Rust
файлами; fmt и diff-check прошли. Это не компиляция Win32, не live COM и не
доказательство нового subscription/array/Unicode кода. Подробности и ссылки
на failed runs — в status; N2/N3/N4 остаются открыты.
Полный Linux gate после fmt остановлен отсутствующим в offline cache
`serde_json`; приложение этим прогоном не проверено.

2026-10-31 N2 DATE: Class = представление времени в floating-point DATE;
Inv = timestamp остаётся в пределах разрешения формата и тип не меняется;
N+1 = целые секунды и дробные секунды; Defect = прежний тест требует точного
nanosecond equality, Windows CI `9553bce` измерил +42 нс при roundtrip.
Два конкретных тестовых момента проверяются с допуском 1 мкс, отдельный тест
общего конвертера проверяет воспроизведённый момент. Это изменение теста,
не округление данных и не доказательство Windows до нового запуска.

2026-10-31 N3 crash: Class = наблюдаемость live native fixture; Inv = сбой
остаётся ошибкой и последняя отправленная фаза видна в CI; N+1 = любое место
dispatch/reply и ранний init/discovery; Defect = `9553bce` завершился с access
violation, а стандартный test capture скрыл пройденные этапы. Markers и
`--nocapture` не исправляют runtime; строгие assertions и cleanup сохранены.
Windows повторное воспроизведение требуется для определения причины.

2026-10-31 N3 fixture arrays: Class = семантика устройства для записи массива;
Inv = строгий тест получает объявленную замену или ошибку; N+1 = изменение
длины и nonzero lower bound; Defect = pinned SDK server default updateValue
делает индексный merge и возвращает FALSE для disjoint bounds, тогда как
complete игнорирует этот результат. На `bd6b9f74` измерено S_OK и прежнее
значение Ints. Fixture явно использует SDK setValue, проверяет BOOL и не
позволяет default merge повторно изменить результат; это не правка backend и
не утверждение о других серверах. Windows live regression ожидается.

2026-10-31 N3 reconnect: Class = диагностика intermittently crashing native
reconnect; Inv = каждый исходный assert и измерение отдельного процесса остаются
обязательными; N+1 = resolve ProgID и каждая стадия SDK open; Defect = 1-й
strict Gateway процесс на `4d22079` прошёл, 2-й завершился AV после Connect101,
3-й не запущен. Добавлены test-only фазовые markers и AV observer, который
возвращает EXCEPTION_CONTINUE_SEARCH; runtime поведение не менялось, дальнейшая
локализация требует Windows CI.

2026-10-31 N3 ephemeral worker: Class = SDK task/context lifetime; Inv = SDK
worker завершён до уничтожения context на caller; N+1 = resolve и discover,
успех и исключение/ошибка; Defect подтверждён pinned source: completion signal
предшествует последнему worker task release, а direct COM-server context не
инициализировал shutdown sink. Использован общий scoped SOCltServer без COM
server impl; SDK управляет своим потоком. Barrier fixture использует настоящий
SDK task, unguarded baseline в subprocess с watchdog и guarded destructor/thread
проверку. Пока это проект проверок, не измеренный Win32 результат и не доказанная
причина AV; полная subscription/teardown/remote проверка остаётся открытой.

2026-10-31 N3 measured acceptance: code `46339cc` прошёл все 6 CI jobs:
SDK run 41124600706 (3), COM 41124600770 (1), app 41124600679 (2).
Win32 baseline watchdog воспроизвёл self-wait; guarded SDK worker joined,
destructor on caller PASS; callback ABI mapping PASS, stand 4/4.
Gateway 3/3 независимых процесса: 0.89/0.89/0.85 с, strict assertions и
cleanup пройдены. Windows все шаги success, включая диагностический UA probe;
отдельные внешние live tests ignored. Production DA остаётся legacy.
Routing/status/audit обновлены по измерению точного code head; последующий
documentation-only commit не меняет код и не объявляется новым runtime proof.
Следующий gap: remote/другой сервер, late/failure/timeout teardown,
session isolation и license closure. Выборочная серия не доказывает отсутствие
всех races и единственную причинность прежнего AV.

2026-10-31 N2.TEST/N3.ISOLATION integration: Class = native status verdict и
независимость сессий при device I/O; Inv = один SDK owner и коррелированный
Test, B завершает read/write/read-back пока A подтверждённо ожидает release.
N+1 = общий D6 mapper и существующий fixture mode; Defect = native Test был
заглушкой, реальный isolation proof отсутствовал. Два агента подготовили
раздельные commits. Родитель добавил da_status в stand, полный stand и launcher
регрессии в Linux SDK CI, strict isolation gate после Gateway 3-run gate.
Event action assert-unset только проверяет отсутствие release, не сбрасывает его.
Measured локально агентами: 41 mock stand test, launcher success/failure,
fmt/diff; Windows/live пока pending. HRESULT failure mock, Running/Failed
должен проверить реальный SDK fixture; production DA остаётся legacy.

2026-10-31 N2.TEST fixture precondition: реальный a004b226 status вернул
NOCONFIG=3; API/mapping не объявили его успехом. Fixture получил явный SDK
Running после start до ready; новый CI pending. Владелец поручил далее
выполнять весь оставшийся план парами Terra High. Очередь remaining-work.md
объединяет прежние R2/N/P планы, различает выполненное, pending и внешние гейты.

2026-10-31 пары 01/02: `36fa668` прошёл все 6 jobs (SDK41137831955,
COM41137831873, app41137831806). Test RUNNING/FAILED/RUNNING в Gateway3/3;
isolation1/1 за0.31с, B42→77→77 до release A, cleanupPASS. Пара02: Class =
bounded shutdown при pending SDK I/O и provenance build inputs; Inv = SDK
не освобождается пока session running, reply не повторяется, drift не
проходит guard; N+1 = существующий barrier и pinned inventory checker;
Defect = missing live timeout acceptance и неполная source/license closure.
Добавлен strict test и CI prepatch guard. Локально measured41mock stand,
launcher, compile-only stubGateway,4checker tests; Win32/live нового test
pending. Legal clearance и release inventory не установлены. Следующая
пара03: UA cache/maxAge и MIT OPC inputs, изолированное владение.

2026-10-31 пары02/03: `70357117` SDK3/3 и COMPASS; pending-write shutdown
измерен5.003833с, Busy до release, одинAcceptedWrite и финальныйcleanup;
app41139818993 ещё выполняется. N1 guard проверил271 cleanSDKentry.
R2.3 Class=UA cache allowance, Inv=FreshSource доминирует в общей reservation,
N+1=mixed/readback/stale/direct/subscription, Defect=SDKmaxAge был0 и
CacheAllowed rejected. Engine-generated ReadCached ограничен permit, Poll
используетrequestedinterval; Subscription initial stronger0 доnegotiated.
4realEngine pathstand tests PASS; UA wiretest добавлен, compile/livepending.
N1 Class=officialMIT inputprovenance, Inv=no oldSDK OPC includes/IID opt-in,
N+1=pin/tamper/fallback/trace guards, Defect=oldgrantclosure unresolved.
Opt-in immutable8 inputs +MIDL CMake route,12Python tests PASS; rootдобавил
sdk/official-mit Win32matrix, retainedlogs/traceguard/COMsuite. Это кандидат,
не провереннаяABI/лицензионнаязакрытость. Root native stand41/41PASS.
Пара04 приступила к SDK negotiatedfacts и hostreuse во времяpendingwrite.

2026-10-31 пара04: Class=SDK negotiatedfacts и processhostreuse приpendingIO;
Inv=requested не revised, epochorigin и perkeygrace, fresh0доминирует;
SDKhostживдоreleaseA; N+1=mixedsub/partialmodify/stale/invalidratio/hugeinterval
и timedoutA→newB; Defect=applied=requested и отсутствующийN2L09liveproof.
SDKfacts берутсяиз publicstate0.19, servernormalizedrawсохранены; scheduling
acceptance1..3600000ms явнаяapppolicy,invalid→log/fallbackgrace/cache0.
Measured4negotiatedstand+2cacheповтора,41native/mockstandwithhooks иtestshim;
actualSDKsample1000→2000 и Win32hostreuse pending. Rootreviewдобавилstrict
rejectвсехunexpectedReply и nativeDa0. UIprojection/recoveryfacts/itemvalidity
ещёopen. CIпроверит03/04общимhead, old02testsнеотменялись.

2026-10-31 exact-head checkpoint: `70357117` завершил app41139818993
Linux/Windows2/2SUCCESS. Вместе сSDK41139819035(3) иCOM41139819114(1)
все6jobsSUCCESS. Только послеэтогопубликуютсяcodecommitsпар03/04 иих
новаяmatrix. Это не доказательство новыхнеисполненныхtestcases.

2026-10-31 measured690a830: SDK41141220207(4), COM41141220142(2),
app41141220265(2) всеSUCCESS. OfficialMIT8inputs staged/MIDL, traces14/11
requiredpaths nooldOPC; обаnativeварианта Gateway3/3+isolation+pending
shutdown+hostreuse. SampleUAwire1234/0 иrequested/revised1000/2000PASS.
Пара05review: Class=persisted legacyprofile и ambiguousWriteexception,
Inv=checkedatomicmigration/visiblewarning, oneUnknown/noRetry/cleanup;
N+1=mixedexplicit/invalid/savefailure/idempotence иone-shotSDKthrowaftercall;
Defect=missingmigration инеизмеренныйN2L10 (необъявленruntimebug).
Local3puremigration+localecompile,42mockABIstand+typedtestshim PASS;
Settings/App иновыеC++/livependingCI. ParentSDKsourcecheckpoint pinned0.19
иguardinstalledVCS, localtemporaryregressionPASS, actualidentitypending.
Пара06terminalAPI/P4вработе; realUI/SDKgatesнеподмененыstand.

2026-10-31 df2bf1d checkpoint: SDK41143031194(4), COM41143031277(2),
appLinuxPASS; Windows124361565035FAILEDтолькоintegration/test_isolation
privatePKIownнепоявилсяза30сautosearch. Productrootcauseнеустановлен;
Class=testprecondition, Inv=тотжеApp/SDKprivatePKIpath иsentinelassertions,
N+1=loopback query безmDNS/adapter, Defect=assertзависелотобщегоautosearch.
Тестdriverявныйloopback,30снеувеличено, новыйWinCIpending. Nativeexception
реальноUnknownDaService0x8000ffff/count1/readback77/noRetry,0.11сofficial;
installedUAcrateVCS9adpass. Пара06Class=readneedterminals/contextualpalette,
Inv=coverage+epoch/oneoutcome иstaleform/Yesnotdispatch; N+1=14Engineedges
иtypedUnknownBrowse/3locale/IME/repeat; Defect=missingperkeyAPI/P4context
иstaleWriteguard. Local14Engine/mock+3actions/mock+locale+2runnerchecksPASS;
fullGUI/counterfactual/PNGpendingCI. СредаудалилаoldsessionGitdb/cache;
recoveredexactpublishedtree89b4/362blobsвcurrentdirectory,06sources/07WIP
сохранены, remotehistoryнепереписана. FuturelocalSHAsновые,remoteheaddf2
осталсяparentдлянефорсирующейпубликации. Пара07frontendE02/packageвработе.

## 2026-10-31 — continuation парами07/08/09

- Class: read result consumers/time facts/native source intent/build closure.
  Inv: нет раннего Good до AddOK; ровно один terminal projection; unknown clocks
  не сравниваются; SDK владеет Device и COM; production legacy до N4.
  N+1: early push+AddErr, late older after idle/equal conflict, cached poll+refresh,
  inactive --check-cfg features, packaged executable vs test-hook library.
- Reviewed07/08 code: E02/bounded GUI+CLI consumers, ValueTimes/TimeSelection,
  SDK Device route и preferred MIT/nohook packager. Измеренные локальные результаты
  и непроверенный Windows/full-Cargo/GPU scope указаны в status; cache утрачен,
  direct real-source stands с mock boundary не объявлены actual SDK.
- cb3da7d: SDK4/COM2 success; App build/shared/library и252 GUI tests/platform
  success, два menu exact-label assertions fail. Upstream egui0.37 Button объединяет
  shortcut atoms с accessible label. Исправлен test query с сохранением action
  assertions и видимого Ctrl+P (47e6558); новый CI ещё ожидается.
- Пара09 выполняет native Fresh/readback capability/permit integration и отдельный
  actual nohook packaged CLI build/fixture read. External/legal/N4 gates открыты.

## 2026-10-31 — реальные gates после восстановления Cargo cache

Class G-R2/S11/E11/P2/P3/P4/P5: epoch admission, очередь и UI input.
Inv: pending Disconnect не даёт Read; due strong demand получает safe boundary
перед Browse2; Deferred не теряет initial tree item; Drop завершает needs один раз.
N+1: continuous Browse+sessionB, grace100s, Poll не блокирует Browse, held repeat,
early push требует AddOK, hover Time должен быть физически видимым.

Измеренные результаты и exact scope перечислены в начале status.md: Engine88,
App264, CLI18; full Linux GUI integration257/table21/theme19; P4 target19,
real guard-disabled/restored P4(2) иP3(1) прошли нужную последовательность.
Native44 — mock ABI, не Win32; Windows PKI47e1/1 за6.41с подтверждён actual log.
До этого новые тестовые drivers ошибочно ждали Read при pending root Browse,
считали synthetic repeat без held-key настоящим repeat и hover невидимой Time
ячейки. Исправлены реальные последовательности без ослабления guards; Source/
Server/Received вTime fixture различаются, pending first push применяется послеAddOK.

Подготовленный Windows batch включает Device+Engine readback и nohook packaged
CLI read. Отдельные UA recovery/namespace/metadata branches ещё review/private;
strict first notification реально failed, поздний sample и NewSession проходят.
Никаких production Async/native DA или release/legal clearance этим не включено.

Большие debug artifacts заполнили временный диск; root удалил только свой
воспроизводимый Cargo target, исходники и logs сохранены. Последующие локальные
сборки debug-info=0/incremental=0, без изменения debug assertions/opt policy.

## 2026-10-31 — Win32 gate be960ff и ремонт private preview

Class: SDK cache lifetime и reviewed patch newline. Inv: fixture-owned SDK refs
освобождаются до SDK common teardown; reverse verification проверяет исходные
blob bytes независимо от checkout CRLF. N+1: обе matrix и release nohook path,
точно те же reviewed hashes, extra tracked/untracked inputs запрещены.
Измерено: App2+SDK4 success;8PNG лично просмотрены root+UI owner. COM I/O PASS,
но cleanup AV; добавлен release device cache как у прежнего exception cache,
без подавления exit check. Packager reverse на CRLF падал на новом реальном
Git regression, после stdin normalization7/7; actual Windows повтор pending.
Root не смешивает этот repair с UA private recovery scopes; N4/remote/license
и полный Async/первая post-recovery notification остаются отдельными gates.


## 2026-10-31 — N1 inventory после CRLF repair

На c7ff4dc native jobs остановились до компиляции: verifier честно обнаружил
изменённый package-preview.py, чей profile hash не был обновлён вместе с
исправлением LF/CRLF. Повторно просмотрен точный diff: только нормализация
patch bytes и передача через stdin для reverse verification, без изменения
SDK inputs, dependencies или notices. Обновлён только этот SHA-256.
Все profile hashes локально совпадают; 4 inventory и 7 package tests проходят.
COM cache-release fix и полная упаковка пока не проверены новым Win32 runtime.


## 2026-10-31 — N1 absolute build-trace roots; 96fd624: 8/9 jobs

Exact head 96fd6249eeac06bd0e3bd039cd15e189f460f328 passed App Linux/Windows
(41158256649), SDK4 (41158256627) and COM2 (41158256863). Both native matrix
variants now pass strict fixture cleanup, Device/Fresh, Engine readback,
blocked-write isolation/shutdown/host-reuse and write-exception gates. These are
local fixture results, not remote/other-server or production-switch acceptance.

The remaining preview-package job 41158256861 compiled the no-hook bridge and
x86 release CLI and passed reverse SDK patch verification. It then rejected
required include paths: CLI arguments were relative while MSVC /showIncludes
reported absolute paths. The packager now resolves both verified roots before
the existing strict trace check; missing generated inputs and old SDK headers
still fail. Real temporary-path regression fails before and passes after this
change; package tests8/8 and inventory tests4/4 pass, with the reviewed profile
hash updated. The packaged executable has not yet passed its real CLI read.


## 2026-10-31 — SDK lifetime, arrays и replay boundary

Class: N2/N3 ownership/shape acceptance и R07 SDK replay evidence.
Inv: SDK владеет task/COM/recovery; timeout не освобождает accepted task;
retire не выдаёт второй outcome; unknown history не исчезает от свежего snapshot.
N+1: mutable Session escape, zero-item failed-master callback, >2^53/extrema,
readonly/invalid writes, поздний RepublishResponse и watch overwrite.
Defect: pending mutable accessor позволял заменить Session; empty failed-master
callback игнорировался; raw BOOL array принимал неканонические bytes. Для каждого
есть actual-source before-fail/after-pass regression. Общие primitive array codecs
уже существовали, повторная реализация не добавлена.

Native Linux runtime stand50/50 и две C++ source/stub regressions прошли;
оба live Rust gates подключены к обоим COM inputs, Windows результат pending.
Research Engine/Gateway replay:1/1 targeted12.06s и fresh runner1/1 18.24s;
SDK delayed response1/1 17.02s, cut1/1 9.08s, watch overwrite unit1/1.
Исторический exactb94303f прошёл все9 jobs; проверенный package manifest183/94/
295875bytes/missing0 и executable42+strictcleanup. Все ограничения в status.

## 2026-10-31 — SDK destruction order и pair22

Class N3/R07. Inv: retirement только после всех SDK task item/group refs;
независимая sessionB прогрессирует при blockedA; secure mode и certificate
verification устанавливаются на actual SDK, unknown остаётся после fresh read.
N+1: queued task removal, SDK base destructor release barrier; rejected-cert
negative case с последующим явно доверенным cert, NewSession вместо Reactivated.
Defect: ранний derived destructor notification; actual-source beforeFAIL/afterPASS
для двух branches. Root runtime stand51/51, secure live1/1 + fresh runner1/1.
Windows isolation/destruction-fix acceptance pending. SDK947 source/core4/4success;
остальные jobs ещё идут. Production Async/pins/default не переключены.

## 2026-10-31 — пары23/24 и точный94773fa green

Class A08/O23 и R07 secure Transfer/token. Inv: old callback не меняет новый
registration; активная subscription не создаёт hidden SyncPoll; SDK owns lifecycle.
N+1: callback88 held→remove/readd→99, distinct server cyclic vs client Sync read,
SDK public Transfer Good without destination item map, same-channel token renewal.
Новые native gates только подготовлены: actual-source C++ guards5/5, runtime51/51;
Win execution pending. UA Transfer real1/1+fresh1/1, stock secure token1/1+fresh1/1.
94773fa завершил все9 jobs, в обоих COM inputs actual array/manager lease1/1 иcleanup;
новый retirement fix/late callback/O23 не приписываются историческому run.
Пара25 A09 отрицательные facts и SDK readiness candidate в работе.

## 2026-10-31 — cec DA1 setup repair

Class N3/O24 test setup. Inv: принудительная версия выбирается до SDK connect,
а доказательство маршрута строится на real fixture request types, не getter.
N+1: production open(false) unchanged, explicit inactive open(true), bounded
phase diagnostics; independent live gates не скрываются за чужим failure.
SDK source force setter online allowed; server negotiation happens at connect.
CEC оба COM stop exit62/zeros, точная failing stage пока неизвестна; cleanuppass.
Shared-impl/source regression + runtime52/52 passed; новый Win результат pending.

## 2026-10-31 — 81f accepted; generic async N+1

81f exact all9 success; новые5 native gates passed в обоих inputs, cleanupstrict.
Class N2 single-slot SDK async conversion/admission. Inv: existing SDK owner,
I4/generic shared submit, only attested flags, no lease retirement on unknown
SDK acceptance. N+1: same bounded Sync codec, no new COM/transaction engine.
Defect: local throw after entering SDK call is not proof of rejection; attempted
flag keeps pending, caller error once/late cleanupACK. Modeled actual-source
before/after passes; runtime53/53, new Win generic execution pending. Source
SDK Sync normal return assigns item outputs; no unnecessary sentinel patch there.
Production Async stays Unsupported and callback-never-arrives quarantine remains.
