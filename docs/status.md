# Текущий статус

## 2026-10-31 — N3 fixture array semantics (Windows CI ожидается)

На head `bd6b9f74` Gateway прошёл Discover, Browse, scalar и UI1 array
read/write, но Ints write с HRESULT S_OK не изменил старый массив. Причина
подтверждена исходниками pinned SDK сервера: `SODaSRequest::complete` вызывает
`cache->updateValue`, игнорируя его BOOL; `doUpdateValue` сливает по индексам,
а disjoint bounds [0..1] и [5..7] не пересекаются. Test server теперь явно
моделирует замену массива через SDK `setValue` и возвращает ошибку при неудаче;
strict changed-length и nonzero-bound read assertions сохранены. Это изменение
fixture, не product workaround; новая Win32 проверка ожидается. `Value::Array`
не хранит lower bound; accepted write не доказывает применение значения.
Предыдущий `0xc0000005` на `9553bce` и возможная гонка уничтожения временного
SDK worker остаются открытыми; их причина не установлена этим тестом.

## 2026-10-31 — N3 Gateway crash диагностика

На `9553bce` [COM interop](https://github.com/boogy777-lgtm/OPCLook/actions/runs/41119942026)
прошёл independent SDK client, C ABI и Rust owner, но Gateway live test упал
`0xc0000005` до появления результатов; cleanup сервера/proxy прошёл. В строгий
тест добавлены phase/request/reply markers и `--nocapture`, без изменения
assertions и runtime. Локализация причины и исправление ожидают Windows CI;
этот диагностический шаг не объявляет Gateway или production DA исправным.

## 2026-10-31 — N2 DATE precision, CI candidate

На head `9553bce` Windows VARIANT regression измерил roundtrip DATE с ошибкой
42 нс: ожидаемое целое значение секунды не представляется точно в binary64
DATE. Тест для двух конкретных дат теперь допускает до 1 мкс, сохраняя проверку
типа и не меняя конверсию. Общий Linux stand проверяет этот случай отдельно;
Windows VARIANT повторно проверяет CI. Другие типы по-прежнему сравниваются точно.
На том же head Gateway live упал с `STATUS_ACCESS_VIOLATION`; см. последующую
диагностику, это не доказательство исправности Gateway.

## 2026-10-31 — N2/N3 восстановление CI после 41a5f007 (проверка Win32 ожидается)

Candidate `41a5f007` добавил preview Browse/Discover, Unicode, подписку и
массивы, но это ещё не подтверждённые capabilities. Измерено на этом head:
SDK source-probe Linux и Windows прошли; Win32 core и local COM bridge не
собрались на `PushState small` (коллизия с Windows `small` macro); Linux и
Windows fmt не прошли; Windows Rust также не собрался из-за удалённой
`clear_variant`. Live шаги этих запусков не выполнялись. Runs:
[SDK](https://github.com/boogy777-lgtm/OPCLook/actions/runs/41103146266),
[application](https://github.com/boogy777-lgtm/OPCLook/actions/runs/41103146409),
[local COM](https://github.com/boogy777-lgtm/OPCLook/actions/runs/41103146352).

Имя переменной изменено, существующая WinAPI очистка VARIANT восстановлена,
Rust отформатирован. Локально измерено: `cargo fmt --all -- --check`,
`git diff --check` и 37 тестов Linux stand с реальными Rust-файлами и mock
ABI прошли. Win32 C++/COM и Windows Rust здесь не скомпилированы; точный новый
head проверит CI. Старые результаты live не распространяются на этот код.
Полный `scripts/check-linux.sh` остановился после fmt до компиляции: в offline
Cargo cache отсутствует `serde_json`; это не результат проверки приложения.
N2/N3 остаются открытыми, production DA остаётся legacy; Unicode, browse,
подписка, массивы, поздний callback/teardown и remote требуют live proof.

## 2026-10-30 — N2 SDK Gateway preview (не production)

Опциональный `native-da-preview` включает существующий Gateway/router с SDK
host и scalar Connect/Add/Remove/Read/Write; сейчас Connect только по CLSID.
Добавлены numeric/BOOL/DATE/CY scalar types через SDK variant setters; строки,
массивы и остальные unsupported. Discover/Browse/Test, Subscription и Unicode
ID явно недоступны. Process-level owner повторно использует entry, пока живы сессии,
до явного финального shutdown процесса; зависшая сессия не приводит к
освобождению под вызовом. Финальный SDK terminate ограничен для caller по времени;
последствия timeout и recovery после исключения на C ABI ещё не доказаны. Production по умолчанию остаётся
legacy; N2/N3/N4 открыты.

Локально измерено: Linux stand подключает настоящие файлы Gateway router и
SDK adapter, command/event test с mock ABI прошёл 1/1. Строгий Windows
`sdk_gateway_live` добавлен без skip; compile/link и живое выполнение пока
не измерены. SDK init/reuse и exception guards требуют Win32 interop.
Отдельный C ABI live run 41089436547 завершился exit 32 до Write; добавлена
диагностика исходных connect/add/read HRESULT и фактов для локализации причины.

## 2026-10-30 — N2 SDK C ABI runtime prototype (не production)

Добавлены SDK host/session и I4 sync I/O C ABI и строгий `--live` fixture
сценарий (connect, add, device read, write, remove/re-add и read). SDK владеет
COM/VARIANT; sync I/O отправляется на SDK per-server worker, host destroy должен выполняться на той же thread и только после
завершения сессий. Для Poll group активна с SDK drPolledRefresh, без SDK cyclic
sync-read; остальные типы, Browse, Discover, подписка и production wiring ещё не
реализованы. Прототип имеет Rust-владельческую обёртку с уникальными регистрациями,
разделяемым host для session threads и отдельный live binary, но не имеет
production callers. Локально
измерено 4 stand tests, включая stale/foreign registration с mock ABI; Windows сборка и live run
этого HEAD не измерены. N2/N3/N4 открыты.

## 2026-10-30 — N2 SDK MTA lifecycle defect (проверка Win32 ожидается)

В pinned Softing SOCltEntry успешный `CoInitializeEx(COINIT_MULTITHREADED)`
оставляет `m_coInit == 0`, а `terminate` проверяет это поле как признак
инициализации и не вызывает `CoUninitialize`. Библиотечный patch отдельно считает
успешные вызовы; инициализация и завершение должны выполняться на одной thread.
Win32 probe теперь проверяет COM context после terminate как на оригинальном,
так и на исправленном SDK. Локально измерено: patch применяется к двум точным
blob, 3 Python tests прошли; Win32 компиляция и regression этого HEAD ещё не
измерены. Runtime adapter, live interop и N2/N3/N4 остаются открытыми; production
DA не переключён. Подробности — в devlog.

## 2026-10-30 — N2.1 offline callback fact adapter (частично)

Добавлен ограниченный adapter direct Softing core: callback → C ABI snapshot →
Rust mapping в существующий `DataValue` для VT_I4. Envelope сохраняет mode,
transaction и оба master HRESULT даже для пустого callback; per-item HRESULT,
quality, FILETIME и VT сохраняются отдельно. Failed master/item HRESULT и неизвестный тип
не становятся значением; неверная identity даёт типизированную ошибку. Последний
callback перезаписывается; snapshot требует внешней сериализации и не является
очередью. Результаты не подключены к Engine/production DA; read HRESULT пока
не передаётся в общий Event. Session/registration identity, live I/O, callbacks
при teardown, полное покрытие типов и license closure остаются открыты.

Локально измерено: Linux stand компилирует реальные Rust файлы и проходит 3 теста;
`cargo fmt --all -- --check` проверяется перед коммитом. Windows C++/SDK и
сквозной fixture не скомпилированы локально, строгий Win32 CI должен проверить
точный новый HEAD. Успехи N1 CI к нему не переносятся. N3 live proof обязателен
до N4; capability production не менялась.

## 2026-10-30 — N1 Softing native DA feasibility (не миграция)

**Измеренный Win32 SDK gate:** на head `d375f15f4b57837baeec75d1cf3a7c9b843cb96e` [run 41067236324](https://github.com/boogy777-lgtm/OPCLook/actions/runs/41067236324) прошёл все три native jobs. Реальный direct SODaC/SOClt/SOCmn core слинкован; offline callback до patch: `read_setup=1 read_correct=0 write_setup=1 write_correct=0 unchanged=1`; после patch: `1/1/1/1/1`. Оба процесса exit 0 с ожидаемыми результатами и завершили common/entry lifecycle. Win32 Expat namespace/DTD smoke также прошёл. Это не COM advise, late teardown или live DA.

Для N2 предпочтителен проверенный direct core, не DaSession/TBC facade: последний имеет только header probe, OTB не подключён. Реальные серверные I/O, удалённые серверы, типы, изоляция сессий, teardown и полная лицензия build closure остаются открытыми. Production DA по-прежнему legacy; patch — предложение на уровне SDK, не обход приложения. Результаты относятся к указанному head, CI этого документационного head проверяется отдельно.

Исследован Softing OPC Classic SDK v4.47.1, commit `5130e763998189c822882434c43e6be2eb0174cf`.
Публичные C++ API предоставляют DA browse, sync/async, maxAge,
subscription и raw per-item результаты; Linux compile-only probe заголовков
прошёл. В SDK обнаружен риск неверного сопоставления HRESULT после неизвестного
item handle. Пinned source-method harness показал 6 read + 6 write-cache
несовпадений на 14 read режим/шаблон прогонах; предлагаемый библиотечный patch
устранил их в том же изолированном harness. Это не linked SDK или live proof.
См. [полный N1 audit](testing/native-sdk/softing-4.47.1.md).

CI run 41061743112: Linux source-method probe прошёл; Windows x86 public
headers и baseline source-method прошли, но fixed source-method остановился
на применении CRLF patch. Core configure прошёл, build остановился в CMake
VerifyGlobs из-за обратных слешей пути. Это ошибки нашего setup до SDK
компиляции, не результат SDK. Общий helper теперь нормализует patch и проверяет
исходный blob; CMake нормализует путь до генерации и проверяется отдельно.
Следующий run 41062539737 прошёл оба source-probe и Win32 configure/ZeroCheck;
в core C++ получены `isNull` C2039 и Expat/WinSDK `ELEMENT_TYPE` C2371,
а также ещё не классифицированные ошибки компилятора Expat. Следующий run
41063686408 подтвердил, что предыдущий выбор без common XML неверен: DA Browse
использует `SOCmnVariant::getVARTYPE`, доступный при CMN_XML. Common XML
возвращён; Watch и XML-DA остаются вне test profile. Точные Expat C файлы
собраны раздельно на Linux: namespace/DTD и C-to-C++ linkage прошли;
компиляция SDK core и Expat на Win32 прошла в 41065211900, но core link остановился:
SDK host hook `getInstanceHandle` не был предоставлен. Probe теперь использует
реальный Win32 module handle; в нём нет таблицы сообщений SDK. Run 41066061516 собрал и слинковал Win32 core, а Expat namespace/DTD smoke
прошёл. Core probe завершился до итогов callback, исходный exit code не был
виден. SDK common lifecycle и диагностический runner добавлены для следующего
CI; последующий run 41067236324 проверил init/cleanup и offline callback, как указано выше. Системный Windows/DA
сервер здесь недоступен. UA SDK не менялся; старый production DA worker
сохранён, native DA migration и полная проверка лицензий SDK closure открыты.
Предыдущий published head 2335b8e прошёл Linux/Windows x86 CI run 41051563765;
результат нового head нельзя выводить из предыдущего.

## 2026-10-30 — редактирование профиля активной сессии (частичный R2.3)

Активный профиль остаётся snapshot текущего Connect; сохранение черновика
применяется при следующем ручном переподключении, авто-reconnect продолжает
использовать текущий профиль. Сохранение с reconnect проверяет профиль,
коллизию, RequestId и запись settings до остановки здоровой сессии. Диалог
связан с Connect epoch; поздние события старой эпохи не изменяют новую. При
потерянном ответе Write журнал показывает Unknown, повторной Write нет. Для DA
принятие Connect означает подготовку существующего маршрута, не подключение
worker. Pending хранится в памяти сессии; recent endpoint сохраняется на диск.

Целевые проверки Linux: 5 App тестов и 2 headless UI теста прошли; RU PNG
900/720 просмотрены. Это синтетические события, не физический DA/live proof.
Предыдущий опубликованный head 6d7ab38 имеет успешный Linux/Windows x86 CI
run 41021101530; результаты этого head не относятся к новой работе. Native DA
SDK, revised параметры, UA CacheAllowed, per-item terminal outcomes, полные G/D
и fairness остаются открытыми. Финальный `scripts/check-linux.sh` (Linux x86_64, jobs=1, debug=0,
incremental=0, системный WQY CJK) завершился exit 0: backend 225, CLI
13+2 live, contract 34+3 doctest, Engine 45+2 doctest, GUI lib 241,
UI interactions 240 и 1 Windows-only ignored; внешний Engine stand 45+2.
Первый полный gate остановился на CJK font test из-за ошибочного DejaVu
override, затем тест и весь gate прошли с настоящим CJK шрифтом. CI нового
commit пока не измерен; физический DA и Windows локально не проверялись.

## 2026-10-30 — явный профиль соединения (частичный R2.3)

EndpointSpec хранит явный AcquisitionConfig (mode, requested interval,
read_after_write, freshness); Engine валидирует его до Connect и использует
неизменяемый snapshot для расписания и ordinary Write. GUI предлагает отдельные
черновики UA/DA и показывает requested mode/interval; CLI использует тот же
контракт и `--poll` как alias Poll. UA Poll не создаёт MonitoredItems и не
принимает поток значений, Subscription не включает скрытый периодический Poll.
Старые endpoints без профиля пока остаются в legacy режиме с прежним fallback;
автоматическая миграция глобального интервала и предупреждение ещё не сделаны.

DA реализован только Poll/Cache/Sync, без read-back и без native SDK миграции.
UA CacheAllowed отклонён как текущий пробел реализации OPCLook, а не ограничение
SDK; заявлен только FreshSource. CLI default read_after_write=false для
совместимости, GUI новый UA профиль — true. Revised interval и effective source
пока не показаны; полный per-item terminal API/fairness и DA native SDK открыты.
Некорректный явный интервал или неполный профиль отклоняются при загрузке
настроек через стандартный backup/notice, а не исправляются UI.
Ранние целевые проверки: contract 34+3, Engine 45+2, CLI unit 13;
UA sample Poll live, App snapshot/dedup, Settings backup, GUI interaction и
PNG 900/720 прошли. Профиль ранее не существовал, поэтому все его поля
обязательны; отсутствие всего acquisition остаётся legacy, явный null — ошибка.
CI 41014492173 на прежнем head: Windows success, Linux первый Browse timeout
15s; первопричина не доказана. Тестовый timeout увеличен до 60s с общим 120s
budget, без retries или ослабления проверки Browse.

Финальный Linux gate на текущем code tree: backend 225, contract 34+3
doctest, Engine 45+2, CLI 13+2 UA live, GUI library 236, UI interactions
238 и 1 Windows-only ignored. Внешний Engine stand 45+2, backend stand UA
Poll live прошёл. UI PNG 900/720 просмотрены; short viewport и Cancel проверены
после прокрутки. Exact-head Windows/Linux CI нового коммита ещё не измерен.

## 2026-10-30 — явные Read intents (частичный R2)

Engine резервирует ExplicitRefresh и opt-in WriteReadback вместе с текущими
снимками/periodic Reads, переносит покрытые цели в PendingRequest и сохраняет
новые цели отдельно при занятом слоте. Только коррелированный принятый Wrote
создаёт read-back, push его не закрывает; reconnect отменяет старую эпоху.
CLI `--read-after-write` для UA дожидается коррелированного Read и возвращает
отдельно outcome записи и данные/quality наблюдения. Это временный булев флаг;
общий per-connection профиль и UI выбора пока не готовы. Без флага привычная
политика сохранена. DA Cache-only отказывает в FreshSource до отправки Write.
GUI пока не включает новую опцию; единый end-to-end GUI/CLI workflow не заявлен.
Проверки текущего code tree: полный scripts/check-linux.sh exit 0 (debug=0,
incremental=0, jobs=1): backend 223, contract 33+3 doctest, Engine 42+2,
CLI 13+2 UA sample live, GUI library 233, UI interactions 235 и один
Windows-only ignored; внешний Engine stand 42+2. Wrong-key CLI regression и
ограничение JSON проверены отдельно. Предшествующий прогон остановлен до конца
для исправления ошибочной корреляции; прежний E0463 без установленной причины
зафиксирован в devlog. CI нового head ещё не измерен.
Отдельные terminal outcome API для всех сообъединённых потребителей, полный
G/D и E набор, fairness и native DA SDK остаются незавершёнными.

## 2026-10-30 — результат записи (частичный R2)

UA adapter сохраняет native status записи, включая Good_CompletesAsynchronously,
Uncertain и Bad; DA legacy adapter сохраняет доступный per-item HRESULT. Err в
Wrote означает установленный отказ до native Write; post-dispatch timeout или
service failure дают Unknown с причиной и доступным raw service status. Общая
нормализация используется GUI и CLI; принятая сервером запись не доказывает
физического действия. GUI сохраняет прежний error dialog для Rejected; CLI
возвращает исход и raw status в JSON и ненулевой код для неоднозначного или
отклонённого результата. Текущее значение не заменяется оптимистически.

Проверено: полный `scripts/check-linux.sh` завершился с кодом 0 на Linux
x86_64 (CARGO_INCREMENTAL=0, debug symbols отключены); headless egui/wgpu
сценарии Rejected, NotSubmitted и Unknown выполнены и PNG просмотрены. Это
синтетические UI данные. Первый запуск без Linux target завершился E0463 из-за
неустановленного default i686 target; другой запуск после check дал E0463 для
opclook_locale, причина не установлена. Один повтор прерван из-за заполнения
диска; последующий чистый gate прошёл. Windows x86 и физический DA ожидают CI
и отдельной live проверки соответственно.

Это не реализация WriteReadback/ExplicitRefresh: единая очередь этих целей,
настройка AcquisitionConfig на соединение и миграция DA на полноценный native
client SDK остаются открытыми. Существующий DA COM worker сохранён. Наличие
native DA сервера и Windows/live не проверено локальным Linux прогоном.

## 2026-10-30 — R2.1: начальные снимки через Engine (частично)

Подтверждённые AddItems создают в Engine одноразовую потребность снимка;
GUI больше не отправляет собственный Read после Add. Engine объединяет готовые
ключи с периодическим чтением, резервирует один Read и выдаёт одноразовый
permit; старый permit после нового Connect не допускается. Пустой Read,
повторная регистрация ItemKey в той же эпохе и параллельная отправка отклоняются.
Таблица identity ограничена 100 000 ключей на соединение с явной ошибкой.
Ранний terminal push закрывает ожидание, WaitingForInitialData — нет.
Постоянный legacy fallback Inactive/Failed/poll_only сохраняется. Для UA
начальная задержка использует текущий interval вызывающего frontend, поскольку
revised interval пока не передаётся backend; это не полная реализация R2.

Измерено: Linux Engine — 36 unit и 2 doctest; contract — 31 unit и 3 doctest;
GUI library — 230 тестов, no-hardcoded — 5; CLI — 9 unit и 2 live UA теста; внешний стенд с
реальным Engine файлом — 36 unit и 2 doctest. Формат проверен отдельно.
В одном промежуточном параллельном запуске CLI browse истёк timeout; при
изолированном повторе после сборки CLI тесты прошли. Поздний push и уже
отправленный snapshot могут оба доставить одно значение; live тест сохраняет
оба события и проверяет последующее изменение.

Полный Linux gate на дереве с исправленным test fixture прошёл; UI interaction:
235 passed, 1 Windows-only ignored. Первый полный запуск остановился с E0463
для `opclook_backend`; следующий обнаружил два падения reconnect fixture, те же
два падения были в Linux CI для 74fa313. Fixture исправлен без сброса ledger
или permit; после этого полный gate прошёл. Причина E0463 не установлена.

Остаются открытыми общая очередь WriteReadback/Refresh, raw Write status,
политики AcquisitionConfig и явный Poll/Subscription в GUI/CLI, частичные
Read outcomes, fairness для Browse и все ещё не исполненные строки G/D/C/U/M/E/S/T/A/O.
UA Write сегодня теряет различие Uncertain и Good: автоматический read-back
на такой Ok не включён. Native DA миграция, timestamps и gap не выполнены;
прежний DA backend и UA timeout quarantine сохранены. Windows/DA стенд и
физический сервер не проверены; CI для нового HEAD следует смотреть отдельно.

## 2026-10-30 — R2.1: частичное исправление состояния Read

Если уже отправлено несколько Read одной сессии, ответ или ошибка одного больше
не разрешают периодический Read, пока другой запрос остаётся в Engine. Это
устраняет преждевременное освобождение флага, но не запрещает параллельную
отправку и не реализует очередь snapshot/read-back или остальные требования R2.
UA timeout quarantine сохранён. Полная native DA миграция и AcquisitionConfig
остаются открытыми; старый DA путь не удалён. Подробности аудита SDK — в devlog.

Измерено: регрессия через реальные send/accept/due воспроизвела ошибку до
исправления; после исправления Linux Engine — 28 unit и 2 doctest прошли,
внешний Linux-стенд с реальным исходником — 28 unit и 2 doctest прошли;
`cargo fmt --all -- --check` прошёл.
Windows x86 и физический DA на этом стенде не выполнялись; CI точного HEAD
нужен отдельно. Полные таблицы G/D и C/U/M/E/S/T/A/O не заявляются пройденными.

## 2026-10-30 — R2.ROUTE: Foundation audit и границы готовых библиотек

Добавлен [входной routing document для LLM](llm-update-routing.md), связанный из
AGENTS.md. [Проверка Foundation](opc-foundation-gap-review.md) уточняет revised
параметры, trigger/queue, recovery/gaps, raw Write statuses и DA semantics.
[Карта владельцев](backend-ownership.md) требует сначала R2.0 use/configure/adapt:
не дублировать Publish/recovery/COM lifecycle готовой библиотеки.

Исправлен слишком широкий запрет DA Subscription + Sync Read из прошлого плана:
при реальных capability разовое чтение разрешено, второй постоянный цикл — нет.
Добавлены O01–O35 как ожидаемые проверки; уточнены UA quarantine и snapshot/gap.
Cargo.lock проверен: async-opcua/client 0.19.0. Точный SDK source/recovery audit,
полные DA 2.05a/3.00 и Windows/live остаются открытыми.

Владелец уточнил обязательный результат: [native SDK UA/DA](native-backend-migration.md).
Смена кандидата разрешена, собственный OPC data-plane DA подлежит замене;
реализация миграции в этом коммите ещё не выполнена.

Коммит только документационный. Проверены ссылки/пути/ID/согласованность и diff
whitespace; формулы CSV G/D не менялись. Cargo/rustfmt недоступны, fmt, сборки
и продуктовые тесты не запускались. Результаты исследований не являются
доказательством runtime соответствия.

## 2026-10-30 — R2.TEST: тестовые таблицы и общий конфиг UA/DA

Добавлены [таблицы R2.TEST](testing/acquisition-truth-tables.md) и два полных
CSV по 128 комбинаций: допуск Read и возникновение потребности.
Таблицы задают ожидаемые результаты для нового агента; тесты продукта ещё
не написаны. [Настройки соединения](connection-acquisition.md) определены как
AcquisitionConfig в EndpointSpec: Subscription/Poll, интервал, read-after-write,
freshness; Sync/Async DA отделены от UpdateMode. Исследованы официальные PTC DA
Group/Communications/Quick Client. Текущий DA код читает Cache через SyncIO;
generated Async/callback bindings не означают готовую подписку.

В этом коммите только документация/CSV. Проверены полнота и уникальность CSV,
формулы/сокращённые строки, ссылки и whitespace. Cargo/rustfmt отсутствуют;
fmt, сборки и runtime-тесты не запускались. Настройки в Rust/UI ещё не реализованы.

## 2026-10-30 — R2: утверждён план обновления значений

Утверждён общий механизм Engine для #13/#16/#17 и сохранение происхождения
времени для #14. [План R2](value-refresh-plan.md) фиксирует pending/in-flight,
корреляцию, ошибки, отмену и проверки гонок. Исследование PTC уточнило политику:
один постоянный режим UA (Subscription или Poll), отдельно разовые чтения
снимка/восстановления/после Write/Refresh; скрытая подмена режима исключается
планом. Реализация, миграция настроек и проверка живых серверов ещё не выполнены.
#16 в проверенном main@c4d073b отсутствует; локальная работа из issue недоступна.

Этот коммит меняет только документацию. Проверены whitespace diff, локальные
ссылки и Markdown-ограждения. Cargo/rustfmt недоступны; fmt, сборки и runtime-тесты
не запускались. Исторические результаты ниже не относятся к этому коммиту.

## 2026-10-29 — ширина столбцов таблицы

Ширину столбцов в таблице тегов теперь можно менять перетаскиванием разделителя в заголовке: у `TableBuilder` включён ресайз (раньше он был выключен, и ручки не существовало). Перестановка столбцов перетаскиванием заголовка сохранена; широкая таблица по-прежнему прокручивается горизонтально. Измерено: fmt, GUI library 230, ui_polish_table 20, interaction 236; регрессия `column_separator_drag_resizes_the_column` воспроизводит реальные pointer-события. Запись в issue #15; проверка на физической Windows-мыши не выполнялась.

## 2026-10-29 — значения после рекурсивного добавления

После «Добавить дочерние» значения больше не зависят от того, присылает ли сервер начальное `DataChange`-уведомление: после ответа `AddItems` приложение один раз читает подтверждённые ключи (при активной UA-подписке периодический опрос подавлен). Для колонки «Время изменения» используется `SourceTimestamp`, а при его отсутствии — `ServerTimestamp`. Измерено: fmt, GUI library 229, interaction 236, backend 270; живой CODESYS — 1265 строк рекурсивного добавления, 1241 со значением за ~3 с, время заполнено у всех строк со значением. Записи в issue #13 и #14; физический OPC DA и сервер владельца не проверены.

## Состояние после PR #10

В `main` слиты исправления P2, первые срезы P3–P5 и клавиатурное управление
четырьмя областями. `Tab` / `Shift+Tab` перемещают фокус между шапкой,
таблицей тегов, серверами и подвалом. Стрелки и `Enter` управляют текущей
областью; клавиатурное изменение открывает один текущий тег, а пакетная
запись вызывается отдельной командой с указанием числа целей. Подтверждение,
когда оно включено, начинается с фокуса «Нет». Подробности и ограничения
фокуса, диапазонов выделения и IME — в [журнале](devlog.md).

**Измерено:** Linux и native Windows x86 задания [CI PR #10](https://github.com/boogy777-lgtm/OPCLook/pull/10/checks)
успешны для HEAD `9d05dd2965b6c2081ef33a11cc73aa2edc9188b7`; PR слит merge-коммитом
`0e4d4c310cbc20cba24345657b7e3d340c7861dc`. Проверки будущих коммитов должны
выполняться отдельно. Windows CI собирает и запускает автоматические тесты,
но не заменяет ручную проверку реальной клавиатуры, IME и средств доступности.
Физические OPC DA, UAC, helper и реальные маршруты по-прежнему требуют стенда.

P2–P5 остаются частичными: общий предел/валидация типизированных payload,
часть сочетаний и встроенных виджетов, ввод параметров Browse/Write в палитре
и дальнейшее обследование компонентов не завершены. Палитра пока открывается
из меню. Текущие задачи перечислены в [плане](next-ui-command-plan.md).

## История реализации и локальных проверок

Сведения ниже относятся к отдельным историческим состояниям и не заменяют
результат CI PR #10 выше. Фразы о необходимости CI соответствуют тому
состоянию, на котором они были записаны.

Базовая версия всех крейтов — Rust 1.99; `rust-toolchain.toml` и оба задания CI закрепляют 1.99.1. На базовом коммите до P1 Windows i686 workspace all-targets typecheck прошёл на 1.99.1 с test-only пропуском ресурсов и manifest; Windows runtime не проверялся. Проверки базового PR #8 приведены в [PR #8 checks](https://github.com/boogy777-lgtm/OPCLook/pull/8/checks); они не относятся к текущему HEAD.

Рефакторинг по темам A–H и исправления B1–B20 реализованы. GUI и headless CLI используют общий Engine; контракт, backend и локализация выделены в отдельные крейты. Текущая матрица решений, проверок и внешних ограничений — в [refactoring-status.md](refactoring-status.md); исходные замечания сохранены в [refactoring-plan.md](refactoring-plan.md) и [аудите](audit-2026-10-27.md) как история, а не текущий список задач.

Локальный `scripts/check-linux.sh` завершился успешно для кода `c5762583f96635b37bc8924295703fde9bdccc8b` (до миграции Rust последующие изменения касались только документации): GUI, backend, Engine, contract, locale и CLI, включая живой sample UA и два B15 proxy-теста. После очистки кэша GUI повторно прошли all-target check, обычная сборка GUI/CLI и все GUI test targets. Rust 1.95.0 выполнил Windows i686 workspace all-targets typecheck с `--locked --offline` и test-only пропуском GUI-ресурсов и helper manifest; это не выполнение Windows. Подробные результаты — в [матрице](refactoring-status.md).

Windows CI содержит native unit tests. Результаты [PR #8 checks](https://github.com/boogy777-lgtm/OPCLook/pull/8/checks) относятся к базовому PR; merge текущего стека требует зелёных проверок его точного HEAD. Физические DA, UAC, привилегированный helper и маршруты требуют внешнего стенда.

Пять сцен до F, после F и после H отрендерены и просмотрены; детали и результаты измерения CPU/аллокаций — в [отчёте](reports/refactor-frame-baseline.md). Живой физический OPC DA, Windows UAC, helper pipe и реальные маршруты требуют внешнего стенда. Идентификатор Connect отделяет unsolicited события старого подключения от текущего; детерминированная регрессия и живой UA sample проверяют фильтр и маркировку. Результаты платформенных проверок и ограничения описаны в матрице.

P1: единые палитры, выбор темы в заголовке и проверка контраста реализованы; измерено: `scripts/check-linux.sh` прошёл, GUI — 220 unit, 149 interaction (один Windows-only пропущен), 20 table и 19 theme тестов; детали, измерения и ограничения — в [отчёте](reports/p1-theme-validation.md). P2 выполнен частично, P3 реализован частично в текущей ветке, P4 и P5 начаты частично; актуальный порядок — в [плане](next-ui-command-plan.md). Windows i686 для P1 локально не скомпилирован и не исполнялся: необходим CI точного HEAD; физические проверки Windows остаются открытыми.

P2 в работе: каталог содержит `ui.connect.open`, `session.browse`, `session.write` с явными ID и кодами; GUI и CLI направляют Browse/Write через Engine, а открытие диалога — через локальное действие. Реализованы проверки сессии и протокола, защита подтверждения при смене соединения и фильтр автоповтора Enter. Измерено: внешний Linux-стенд выполнил девять целевых интерактивных тестов; PNG `docs/screenshots/p2-disabled-action-ru.png` просмотрен. P2 полностью не закрыта: требуется определить совместимые границы размеров payload и правила их проверки до построения больших значений; проверка формы UA сейчас выполняется backend после отправки, а не в запросе доступности действия; отмена исключает отправку до dispatch, но не отзывает уже отправленную OPC-запись. Измерено: полный `scripts/check-linux.sh` прошёл; GUI library 220, interaction 158 (один Windows-only пропущен), navigation 16, table 20, theme 19; остальные результаты — в журнале. Windows i686 изменённого кода локально не скомпилирован и не исполнялся, проверит CI; физические DA/UAC/маршруты не проверены.

Уточнение P2: backend UA проверяет форму массива и типы inferred элементов; данные неизвестного типа со смешанными или пустыми элементами массива теперь отклоняются существующей ошибкой, вместо создания некорректного UA массива. Пустой массив остаётся допустимым. Проверка UA выполняется после проверки подписки и возвращает коррелированный `Wrote`. Известные имя пользователя и пароль маскируются в диагностике сессии; обнаружение любых иных секретов в произвольном ответе сервера не обещается. Валидация ввода и выбор совместимого предела размеров остаются открытыми; подробности — в журнале. DA COM на Linux не проверялся.

Измерено после уточнения UA Write: полный `scripts/check-linux.sh` прошёл (backend 219, GUI library 220, interaction 158, один Windows-only пропущен); регрессия также прошла во внешнем Linux-стенде. Windows i686 текущего HEAD не скомпилирован; проверит CI.

Интеграция R1 с P2 подготовлена: запись после Disconnect блокируется при отсутствии эпохи, старое событие не влияет на подтверждение нового подключения. Измерено: целевые Engine, GUI и CLI и live UA прошли; полный `scripts/check-linux.sh` прошёл: backend 220, GUI library 222, GUI interaction 158 (один Windows-only пропущен), CLI 9 unit и 2 live; внешний Linux Engine-стенд выполнил 27 unit и 2 doctest. Предыдущий gate использовал устаревший GUI test binary и не был зелёной проверкой текущего состояния; итоговый повтор выполнен после пересборки. Windows i686 не скомпилирован и не исполнялся, физический DA не проверен.

P2: исправлен общий диалог подтверждения: Enter на кнопке «Нет» отменяет Write, Disconnect и RemoveSession. До исправления headless-тест воспроизводил отправку Write. Проверки границ payload и ранней формы остаются открытыми; приоритет ошибок подписки должен сохраняться. Windows i686 и Windows runtime не проверены локально, необходим CI точного HEAD.

Ручной ввод Write: добавлены предел 1 МиБ UTF-8 на значение и 16 МиБ на расчётную пачку до разбора и копирования запросов. Это новая граница GUI; ввод в виджет уже выделен, прямые типизированные payload не ограничены, проверка формы и общий лимит остаются открыты. Серверные ошибки для отправленных запросов сохраняют прежний порядок.

Оценка 16 МиБ учитывает размер структуры WriteRequest на текущей архитектуре и не является жёстким лимитом потребления памяти. При прямом apply_write ошибка размера предшествует недоступной сессии; в UI кнопка записи при недоступной сессии отключена. CLI имеет собственный предел аргумента 4096 байт.

Измерено для этого дополнения P2: Linux all-target check, сборка GUI/CLI, CLI boundary, shared/CLI тесты с живым UA, GUI library 223 и все GUI integration (interaction 163, один Windows-only пропущен; navigation 16, table 20, theme 19) прошли отдельными запусками. Внешний Linux-стенд подключил реальный файл GUI-тестов через `#[path]` и выполнил 14 целевых тестов; PNG подтверждения с фокусом «Нет» отрендерен и просмотрен. Единый полный запуск прервался из-за ошибки архива зависимости; отдельные GUI тесты прошли с временным `codegen-units=1` для async-opcua-core-namespace. Windows i686 текущих изменений не скомпилирован и не исполнялся; проверит CI точного HEAD.

P3 подготовлен частично: Mod+N для открытия подключения берётся из каталога, контекстные клавиши из локального реестра; сравнение кандидатов и ограничения — в [отчёте](reports/p3-keymap-comparison.md). Защищены проверенные пути Enter и IME, но не доказана защита всех встроенных контролов. Измерено: `scripts/check-linux.sh` прошёл на Linux; backend 220, GUI library 225, GUI interaction 188 (один Windows-only пропущен), navigation 16, table 20, theme 19. Внешний стенд с реальным `ui_interactions.rs` через `#[path]` выполнил 25 P3 тестов. Windows i686 не скомпилирован и не исполнялся, необходим CI точного HEAD; реальные Windows IME, физические раскладки и физический OPC DA не проверены. P2 остаётся частичным; P4 и P5 развиваются отдельно.

P3, уточнение: подписи F5, Enter/F2, Delete и переключения панелей формируются из локального реестра; конфликтный тест проверяет все каталоговые привязки против локальных. Из подсказки пункта «Добавить выбранный» удалено ложное утверждение об эквивалентности Enter в дереве; проверка до исправления воспроизвела неверную подсказку. Краткие обозначения стрелок и Shift+Page ещё статичны, `session.write` не привязан напрямую к Enter/F2. Доступность и полноценное поведение всех платформенных IME остаются открытыми; Windows CI для точного HEAD обязателен.

P4 в работе: создан первый срез палитры из меню. Поиск использует локализованное название, семантический ID и код; `ui.connect.open` вызывает существующее UI-действие после закрытия палитры и повторной проверки. Доменные `session.browse` и `session.write` пока показаны недоступными с причиной отсутствия контекста: их типизированные payload не создаются автоматически. Открытие по клавише и реальный ввод параметров доменных действий остаются открытыми.

P4: при появлении другого диалога/импорта палитра закрывается, а клавиатурные события текущего кадра не передаются фоновым обработчикам. При незавершённой IME-композиции и отсутствии `Commit`/пустого `Preedit` приложение продолжает блокировать клавиши до завершения композиции или потери фокуса окна: это fail-closed ограничение, доступно управление указателем; поведение реального Windows IME не проверено.

Измерено для первого среза P4: Linux GUI interaction — 204 теста прошли, 1 Windows-only пропущен; no-hardcoded — 5 тестов прошли; внешний Linux-стенд с реальным тестовым файлом через `#[path]` — 14 целевых тестов прошли. PNG `docs/screenshots/p4-palette-ru.png` создан и просмотрен. `cargo fmt --all -- --check` и `git diff --check` прошли. Единый полный `scripts/check-linux.sh`, Windows i686 typecheck и Windows runtime для P4 не выполнены; точный HEAD требует CI. Физические OPC DA и реальный Windows IME не проверены.

P5 начат: вынесена локальная кнопка с необязательной причиной недоступности. Она использует текущие стили egui и возвращает Response; App/backend и вычисление доступности остаются у вызывающего кода. Применена к адаптеру локального действия и кнопке записи; прежняя причина сохраняется только там, где уже существовала (для не writable и открытого подтверждения новые причины не добавлены). Полное обследование компонентов остаётся открытым.

Измерено для первого среза P5: полный Linux `scripts/check-linux.sh` прошёл, включая live UA; backend 220, GUI library 228, interaction 204 (один Windows-only пропущен), navigation 16, table 20, theme 19. Внешний Linux-стенд подключил действительный файл компонента через `#[path]` и выполнил две регрессии: обычный/двойной клик и disabled с доступной причиной. PNG активной и недоступной кнопки просмотрены. Windows i686 текущего кода не скомпилирован и не исполнялся, требуется CI точного HEAD; P5 остаётся частичным.

Клавиатурная навигация: четыре глобальные области Header → Tags → Servers → Footer (Tab/F6, Shift для обратного направления), включая пустые области. В Tags стрелки перемещают фокус и прокручивают нужный тег; Enter/F2 редактируют только фокусированный ItemKey, поле открывается с выделенным значением. Подтверждение открывается с фокусом No, Left/Right выбирают ответ, Enter исполняет сфокусированный ответ. По No/Escape запись не отправляется. Servers имеет отдельный курсор списка и подфокус дерева; Header использует действующие меню, Footer — действующие вкладки, кнопки и Script. Верхний диалог владеет клавиатурой и фоновая область отключена; системное сообщение приоритетнее и временно приостанавливает CSV. IME, авто-повторы, перестановка/удаление строки и смешанные события проверены синтетически; настоящий Windows IME и физические раскладки не проверены. Детали и область охвата — в журнале. Windows i686 локально не скомпилирован и не исполнялся, требуется CI точного HEAD; физический OPC DA не проверен.

Совместимость темы P1 сохранена: при открытом Options доступен только фоновый селектор темы, синхронизирующий черновик; остальные фоновые действия отключены. Клавиши открытого popup темы не передаются Options, синтетическая IME-композиция блокирует активацию и Escape в popup.

Множественное выделение: Ctrl+стрелки двигают только курсор, Ctrl+Space переключает выбранность, Shift выделяет диапазон по anchor ItemKey, удержанный Ctrl+Shift дополняет исходный набор и сокращает диапазон при обратном движении. Контекст/двойной клик меняют один тег, для пачки предусмотрена явно названная команда меню и отображение количества целей. Измерено: полный Linux gate зелёный (GUI interaction 235, один Windows-only пропущен; Header/Footer 7); внешний Linux-стенд выполнил 36 целевых тестов. Фокус, выделенный текст и No по умолчанию просмотрены на PNG. Native Windows UI/IME и физическое OPC оборудование не проверялись.
