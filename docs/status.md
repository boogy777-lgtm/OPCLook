# Текущий статус

## 2026-10-31 — опубликованный baseline9/9; следующий пакет R2/N2/N1

`adbd315` прошёл все9jobs, включая реальный запуск упакованного x86 CLI и
строгую COM cleanup. Следующий private packet добавляет scoped SDK recovery,
per-key UserAccessLevel/type metadata, sequence/Overflow facts, GUI/CLI requested
vs revised, DA failure facts и проверенные Cargo notice texts. Измерения и
ограничения: [packet evidence](testing/packet-2026-10-31.md). Native Async
production по-прежнему Unsupported; library patches остаются research.
N4/remote/physical Windows/legal gates не закрыты. Новый CI exact packet pending.


Режим продолжения по поручению владельца: все оставшиеся задачи выполнять
парами Terra High; [очередь и зависимости](remaining-work.md).

## 2026-10-31 — be960ff: App2/SDK4 success; два native регресса исправляются

Измеренный [App CI](https://github.com/boogy777-lgtm/OPCLook/actions/runs/41155211535)
наbe960ff прошёл Linux и Windows, включая весь GUI suite, P3/P4 actual
counterfactual и8PNG. [SDK CI](https://github.com/boogy777-lgtm/OPCLook/actions/runs/41155211445)
прошёл4/4. Root и первоначальный UI исполнитель лично просмотрели все8 локальных
PNG того же code: P4RU/EN/ZH, Light confirmation, T08, P2, P3, P5; блокирующих
layout проблем не нашли. T08 Source/Server/Received различимы, порядок остаётся
Unknown без clock attestation. Это не physical Windows IME/layout proof.

[COM2](https://github.com/boogy777-lgtm/OPCLook/actions/runs/41155211450) выполнил
первый actual cross-process Read/Write с PASS, но server cleanup завершился
0xC0000005 в обоих variants. Новый device_read_count_cache оставался global SDK
reference после exitSOCmn, в отличие от exception_count_cache; добавлен release
после entry.stop до terminate/common-exit. Строгий launcher exit gate сохранён;
новый Win32 результат ожидается, единственная причина AV пока не объявлена.

[Preview](https://github.com/boogy777-lgtm/OPCLook/actions/runs/41155211495)
реально собрал clean nohook bridge и x86 release CLI, затем остановился на
reverse SDK patch verification. Новая LF/CRLF regression на disposable Git
fixture воспроизвела отказ исходного packager; нормализация patch bytes при
stdin reverse + eol=lf для *.patch дала7/7 passes. Patched SDK blob hashes и
untracked-file checks сохранены. Bundle/packaged executable read не приняты,
так как до них этот job не дошёл. Исправления отправляются отдельно от ещё
частной UA recovery/Async ветки; весь be head не green (6/9jobs success).

## 2026-10-31 — пары07–12: локальные реальные проверки и подготовка Windows

Текущий интегрированный code `77869bd` ещё не назван Windows-green.
Восстановлены Cargo sources; последующие проверки используют настоящий SDK0.19,
не прежние stubs. Измерено на Linux: весь App integration suite прошёл,
включая ui_interactions257/257 (один отдельный Windows-only ignored), table21/21,
theme19/19 и PKI isolation. P4 targeted19/19; реальные counterfactual runners
отключили только guards, получили2/2 ожидаемых assertion failures дляP4 и1/1
дляP3, восстановили точные файлы и получили2/2 и1/1 passes. PNG ещё требуют
рендеринга/личного просмотра; этим они не заменены.

Новые реальные Cargo проверки отдельных согласованных деревьев: Engine88/88,
App264/264, CLI18/18; CLI sample live2/2, backend239/239 ранее прошли на том же
неизменённом backend code. Native Rust adapter44/44 с mock ABI прошёл; это не
Windows COM. Уточнение прежней диагностики: на опубликованном47e6558 actual
Windows PKI isolation1/1 прошёл за6.41с ([App run](https://github.com/boogy777-lgtm/OPCLook/actions/runs/41148984434)).
SDK4/4 и COM2/2 также success ([SDK](https://github.com/boogy777-lgtm/OPCLook/actions/runs/41148984433),
[COM](https://github.com/boogy777-lgtm/OPCLook/actions/runs/41148984417)); App целиком
не green из-за трёхP4 тестов, исправленных и проверенных локально после этого.

Пара10: общий typed payload validator до dispatch (policy1MiB UTF-8/bytes,
depth32, nodes/children65536) сохраняет приоритет unavailable/epoch/registration.
GUI/CLI сообщают отказ без исходного payload. P3 защищает numeric/checkbox IME,
обычный ввод/commit/pointer сохранён; integer drag speed1 вместо0.25 — осознанная
UI policy для работающего keyboard step, float speed не менялся. Инвентарь P5:
[локальные компоненты](reports/p3-p5-followup.md). Physical Windows IME/layouts
и native accessibility отдельно открыты.

G-R2 regression исправлен: pending Disconnect с connection_id=None больше не
планирует Read;6 старых Connected fixtures получили действительную epoch.
Drop теперь завершает pending/reserved/in-flight read needs через прежний
cancel_reads; explicit shutdown→Drop не дублирует terminal (2 actual tests).
S11 fairness: ожидающий due strong Read имеет приоритет перед новым Browse;
существующая GUI AddChildren queue сохраняет исходный item/epoch и ждёт без
pop/loss/busy-spin. Typed Deferred отделён от failure, CLI получает явный отказ
до RequestId. Периодический Poll не запрещает Browse навсегда.

[Матрица](testing/read-matrix-results.md): измерены все128D (104 dispatch/24 no-read).
128G не объявлены128 одновременно достижимыми состояниями:8 actual combinations,
остальные120 — настоящие ранние guards/недоступная FreshSource. Есть реальные
old-epoch/Remove/coalescing/fairness/Drop проверки, не копия формулы CSV.

Следующая пара backend reviewed privately: UA SDK stream recovery/reconciliation,
namespace/type invalidation и shared bounded URI identity;3 sample recovery
сценария прошли на отдельном дереве. Строгий первый43.5 после ReactivatedSession
реально не доставлен, поздний44.5 доставлен; это известный незакрытый SDK/history
gap, не green за счёт ignored. Engine/UI/CLI consumers и защита Browse/metadata
после NewSession интегрируются отдельно. Native Async пока только fail-closed
selector и test-only SDK prototype, actual i686 proof pending; product capability
не включена. N1 nohook package, Windows Device/readback, source/license closure,
remote/other-server и N4 production gates остаются открыты до своих доказательств.

## 2026-10-31 — пары07/08 подготовлены, интеграция09

На `cb3da7d` SDK4/4 и COM2/2 прошли: [SDK](https://github.com/boogy777-lgtm/OPCLook/actions/runs/41146777682),
[COM](https://github.com/boogy777-lgtm/OPCLook/actions/runs/41146777679).
[App](https://github.com/boogy777-lgtm/OPCLook/actions/runs/41146777690) обеих платформ
прошёл build/shared/library (252 GUI library tests каждый), но остановился на
двух query-by-exact-label assertions в keyboard_header_footer. egui0.37 включает
shortcut_text в accessible label кнопки; проверки переведены на contains,
сохранены Enter→palette, Tab→close и проверка отображаемого Ctrl+P. Исправление
`47e6558` опубликовано отдельно, новый CI ожидается. Это не green всего head;
P4 actual counterfactual/PNG и PKI test пока не объявлены принятыми.

Пара07 code reviewed: E02 InitialPush отдаётся только после успешной Add,
GUI хранит один pending push без преждевременного Good; terminal metadata
потребляют GUI/CLI через bounded latest buffer без I/O в sink. CLI single JSON
дополняется read_observations после shutdown; старые data/readback поля остаются.
6 targeted real-Engine/mock tests и2 buffer tests прошли; GUI/CLI реальные tests
ожидают CI. Drop без explicit shutdown и отдельные coalesced predispatch Write IDs
не закрыты этим срезом.

N1 code reviewed: official MIT OPC inputs предпочтительны, probes выключены
по умолчанию, legacy OPC headers только явный test control. Packager проверяет
pinned patched SDK, x86 PE, no-test-hook symbols, Cargo active features и
reachable license inventory, clean include trace и CLI link map. Disabled
features в --check-cfg не считаются включёнными. 7 packaging и4 license guard
теста прошли локально. Новый Windows job должен собрать чистый bridge/CLI,
упаковать notices/receipts/evidence и прочитать42 именно упакованным executable.
Это инженерный private preview, не полная license/distribution clearance.

Пара08 code reviewed: ValueTimes разносит UA source/server, DA timestamp,
received_at и origin. SourceClockId даётся только при реальном attestation;
SDK его не предоставляет, поэтому production ordering остаётся Unknown.
TimeSelection сравнивает Good только для того же известного clock/type;
5с неоднозначности отсчитываются с первого отклонённого older, equal conflict
и Bad не скрываются. 5 policy tests и Engine propagation stand прошли;
новые UI tooltip, CLI mapping и sample-UA timestamps ещё ожидают полного CI.

Native DA ReadFresh использует готовый SDK maxAge0/Device, ReadCached — Cache;
fixture DeviceReadCount различает реальные device calls. 43 native mock tests,
legacy route и launcher checks прошли; actual Win32 Device/readback test
pending. Пара09 интегрирует capabilities/Engine permit/readback/time mapping
и готовит CI07/08. Production DA по-прежнему legacy; N4, remote/other-server,
реальные UAC/IME и legal gates остаются открытыми.

## 2026-10-31 — пара05:7/8jobs; пара06 подготовлена

Head `df2bf1d55d35a38d0156894099436b1e1e4449a4`:
[SDK4](https://github.com/boogy777-lgtm/OPCLook/actions/runs/41143031194) и
[COM2](https://github.com/boogy777-lgtm/OPCLook/actions/runs/41143031277) success.
[App41143031202](https://github.com/boogy777-lgtm/OPCLook/actions/runs/41143031202):
Linux success, Windows integration failure в test_isolation: private PKI own
не появился за30с после auto-search. Windows build, shared/CLI, library и
остальные предыдущие шаги прошли. Product PKI wrong-path/root cause этим не
установлены. Test driver теперь явно отправляет один loopback discovery через
App вместо автоматического mDNS/LDS/adapter поиска; isolation assertions и30с
сохранены. Новый Windows результат этой проверки pending.

N2L10 реально прошёл в обоих native variants: post-syncWrite exception дал
Unknown/DaService(0x8000ffff), fixture count1, read-back77, reconnectcount1,
no retry и cleanup. Official-MIT test duration0.11с. Installed Cargo client
VCS guard прошёл, поэтому audited9ad28fc подтверждён фактическим0.19 crate.
Migration file-I/O/App tests прошли Linux и Windows library; весь head не green.

Пара06 reviewed: Engine выдаёт per-key terminal result через PendingRequest и
opt-in синхронный sink, без очереди. Missing/Bad/Uncertain/service error/timeout/
cancellation/dispatch failure различаются, covered purposes отделены от новых
needs после dispatch. 14 новых real-Engine/mock-Gateway targeted tests прошли;
это не полный workspace suite. Initial push/E02, GUI/CLI sink integration и
Drop без explicit shutdown ещё не покрыты этим API; одинаковые needs до dispatch
coalesced, без отдельных Write IDs. Пара07 выполняет frontend/E02 integration.

P4: Mod+P, typed arbitrary Browse с anchor только после correlated success,
Write через существующую форму/confirmation, session+epoch recheck. Добавлены
2 stale-form/Yes regressions и counterfactual runner (guards off→2 assertion
failures, restore→2passes), плюс actual UI tests. Local3 real-actions/mock tests,
locale3-языка и2 runner selftests прошли; GUI/actual counterfactual pendingCI.
Подготовлен Linux wgpu render4PNG (DarkRU/EN/ZH Browse, LightRU Write confirm),
log/artifact capture; PNG ещё не созданы и не просмотрены, P4 пока не принят.

Во время работы среда очистила старый scratch Git/toolchain. Опубликованный
snapshot восстановлен в текущем каталоге с точным tree89b4ebaf и362 blob SHA;
private06code и изменения07 сохранены, remote/main не переписывались. Rust1.99.1
доступен в текущем runtime, утраченный dependency cache не выдаётся за сборку.
Пара07 также готовит preferred official-MIT/no-test-hooks private CLI bundle;
новый Windows build/package и release/license/N4 gates остаются открытыми.

## 2026-10-31 — пары03/04 приняты на `690a830`, пара05 подготовлена

Head `690a830761b2ce9b33d58ffa58edb44873606106` прошёл все8jobs:
[SDK4](https://github.com/boogy777-lgtm/OPCLook/actions/runs/41141220207),
[COM2](https://github.com/boogy777-lgtm/OPCLook/actions/runs/41141220142),
[приложение Linux/Windows2](https://github.com/boogy777-lgtm/OPCLook/actions/runs/41141220265).
Два native matrix варианта sdk/official-mit прошли Gateway3/3 каждый,
isolation, pending shutdown и N2L09 host reuse. MIT fixture trace содержал14,
bridge/core trace11 требуемых generated paths без старого SDK OPC каталога.
Pinned8 inputs реально staged, MIDL и SDK baseline/patched lifecycle и
callback→Rust regression прошли. Это измеренный build/local fixture proof,
не полный release/license closure и не remote DA.

Новые обычные sample-UA tests прошли: фактический wire maxAge1234/0,
requested publishing1000/revised2000 и SDK item facts. Все8jobs прошли;
это относится к данному head, не к следующим новым функциям.

Пара05 reviewed/integrated privately: migration persisted recent_endpoints
из legacy None в явныеUA Subscription/FreshSource/readback=false и
DA Poll/Sync/Cache/readback=false; checked interval, atomic persist/retry,
точные RU/EN/ZH warning/status и блокировка исходного файла при invalid.
3 pure real-file migration tests и production locale compile прошли;
3 file-I/O и2App tests ожидают CI. last_* строки не называются migrated.
N2L10 добавляет test-only исключение после фактического SDK syncWrite,
fixture write counter и strict Unknown/E_UNEXPECTED/no retry/cleanup test.
42 mock-ABI tests прошли, Win32 новых C++/live ещё не измерен.

Parent source audit уточнил locked UA SDK ownership:
[проверка0.19](testing/ua-sdk-0.19-recovery-audit.md). Installed-crate VCS guard
и его временный regression добавлены, реальная identity проверка pendingCI.
Тонкое наблюдение SDK event stream/reconciliation/replay остаётся открытым.
Пара06 выполняет per-key Read terminals и полный срез P4 palette/epoch guards.
Ниже сохранена история отдельных состояний.

## 2026-10-31 — пара 04 интегрирована, новый runtime CI ожидается

Два отдельных commits добавляют SDK negotiated snapshot и N2L09 host reuse.
UA backend получает publishing/lifetime/keepalive и per-item sampling/queue
из закреплённого SDK state, отдельно от accepted requested values, с Connect
origin. Engine хранит факты текущей эпохи, grace использует revised interval
ключа, CacheAllowed Subscription initial — минимум известных валидных
интервалов пачки. Refresh/readback доминируют maxAge0; unknown/invalid также
не разрешает cached read. Scheduling принимает1..3600000мс (политика OPCLook,
не протокольный максимум), lifetime ratio проверяется checked. Raw normalized
SDK facts сохраняются и при invalid; initial grace использует configured
fallback, не выдавая его за revised. Sampling -1 — SDK sentinel, не миллисекунды;
wire f64 до нормализации SDK недоступен. UI projection/полная item validity и
recovery-driven обновление facts ещё открыты. 4 negotiated/O08 stand tests и
2 cache-permit повтора прошли; sample1000→2000 compile/live ожидает CI.

N2L09 проверяет новый Gateway после bounded shutdown старого с pending Write:
тот же живой SharedCore, B I/O до release, Busy SDK, затем один Accepted reply
и cleanup. Feature native-da-test-hooks добавляет bounded наблюдение host
acquisitions; pointer наружу не передаётся. 41 mock-ABI stand tests с feature
и compile-only test shim прошли; новый strict Win32/live test ожидает CI.
CI следующего head объединяет принятые code commits пар03/04 без отмены
проверок предыдущего head. Пара05 выполняет migration старых профилей и
исключение Write task N2L10; production DA по-прежнему legacy.

## 2026-10-31 — пара 02 native gates прошли; пара 03 подготовлена

Head `70357117b8ba6ed128ee0c37a76bac9d71fac009`:
[SDK 41139819035](https://github.com/boogy777-lgtm/OPCLook/actions/runs/41139819035)
прошёл 3/3 jobs; [COM 41139819114](https://github.com/boogy777-lgtm/OPCLook/actions/runs/41139819114)
прошёл, включая новый sdk_pending_shutdown_live. Измерено: shutdown DA TimedOut,
UA Joined за 5.003833 с; до release SDK Busy, после release один Accepted Write
и final SDK cleanup. Test duration 5.21 с. Полный clean SDK inventory271 также
прошёл до patch. App Linux/Windows [41139818993](https://github.com/boogy777-lgtm/OPCLook/actions/runs/41139818993)
также прошёл2/2: все6 jobs этого exact head зелёные.

Пара 03: UA Poll CacheAllowed теперь передаёт SDK maxAge=requested poll interval.
Engine сам создаёт ReadCached лишь из действительного permit; direct Read,
Refresh и read-back остаются maxAge0, включая mixed batch. Subscription initial
сохраняет более строгий0, пока revised publishing interval недоступен; полная
семантика effective interval ещё открыта. Четыре теста реального Engine через
path-stand прошли; новый sample-UA wire1234/0 test ожидает compile/live CI.
Native real-file/mock-ABI stand на интегрированном коде прошёл41/41.

N1 MIT candidate: отдельный opt-in CMake path генерирует headers/IID/proxy
из8 immutable official MIT-labelled inputs. Старые SDK OPC includes/IID в этом
варианте исключены. Подготовлена CI matrix sdk/official-mit с retained include/
link logs, trace guard и live COM suite; старый путь остаётся default. Локально
прошли12 Python tests (3staging+2generation+3trace+4inventory). Реальные MIDL,
Win32 compile/trace/link/live этого пути ещё не измерены; полного license/release
closure нет. Пара04 выполняет negotiated UA facts и host reuse при pending I/O.

## 2026-10-31 — пара 01 принята на `36fa668`, пара 02 ожидает CI

На `36fa668a4b5777cbf751ee4eee866e9c1151518a` успешно завершились все
6 jobs: [SDK — 3](https://github.com/boogy777-lgtm/OPCLook/actions/runs/41137831955),
[COM — 1](https://github.com/boogy777-lgtm/OPCLook/actions/runs/41137831873),
[приложение Linux/Windows x86 — 2](https://github.com/boogy777-lgtm/OPCLook/actions/runs/41137831806).
Strict Gateway прошёл 3/3 отдельных процесса, каждый с RUNNING → FAILED →
RUNNING verdict. Двухсессионный live gate прошёл: B прочитал 42, записал 77 и
прочитал 77, пока device-write A ожидал release; затем A завершился и cleanup
прошёл. Измеренное время isolation test — 0.31 с. Это локальный SDK fixture.

Пара 02 добавляет отдельный strict pending-write shutdown test: ожидаются
ограниченный shutdown, Busy SDK до release, один terminal write reply и
финальное освобождение SDK после release. Он включён в Win32 COM CI;
реальный результат этого нового сценария пока не измерен. Локально прошли
41 mock-stand тест, launcher и compile-only проверка файла со stub Gateway.
N1.LICENSE добавляет pinned inventory, profile guard и notices: 4 локальных
теста checker прошли. Проверка полного SDK checkout включена в CI до patch.
Grant точных старых OPC Foundation copies, configured include/link trace и
release notices остаются открытыми; inventory не является legal clearance.

Следующая пара 03 выполняется изолированно: UA CacheAllowed/maxAge и
официальные MIT OPC Core build inputs. Production DA остаётся legacy;
N1/N3/N4 не объявляются полностью завершёнными. Ниже — история состояний.

## 2026-10-31 — начальное состояние fixture для N2.TEST (CI ожидается)

COM run [41136545085](https://github.com/boogy777-lgtm/OPCLook/actions/runs/41136545085)
на `a004b226` измерил NOCONFIG=3 в первом Test. Native verdict корректно дал
DaNotReady, strict Gateway остановился: 0/1 выполненных процессов, следующие
два и isolation не запускались. SDK client/C ABI/Rust owner прошли, cleanup
после ошибки прошёл. Fixture после успешного start теперь явно задаёт Running
через SDK API до ready-файла; backend/assertions не ослаблены. Новый Win32/live
результат пока не измерен. SDK jobs этого старого head прошли; app результат
фиксировать отдельно, не выводить его из COM failure.

## 2026-10-31 — N2.TEST и N3.ISOLATION (Win32/live CI ожидается)

Две отдельные задачи интегрированы в native preview. Test активной DA-сессии
использует public status API Softing на worker SDK и общий D6 mapper. Только
RUNNING даёт DaStateOk; прочие/неизвестные состояния и native failure дают
коррелированную Test-ошибку. Negative HRESULT сохраняется в detail как hex;
успешный raw HRESULT общий Event не хранит. Zero-initialized output превращает
отсутствующий SDK status в unknown 0. Windows live fixture должен проверить
RUNNING → FAILED → RUNNING через SDK ServerState; mock ABI этого не заменяет.

N3.ISOLATION добавляет test-only SDK device-write barrier: сервер подтверждает
приход A, B выполняет Read/Write/Read-back до освобождения A, затем обе сессии
и Gateway/SDK завершаются, launcher проверяет server/proxy cleanup. Barrier
ограничен 60 с, ошибки/assertions освобождают его; три проверки assert-unset
не сбрасывают событие. Новый live gate пока не измерен, это не remote proof.

Локально агенты измерили 41 mock-ABI stand test и две launcher test functions
с success/failure subcases, fmt и diff-check. Exact Windows SDK build, Test
сценарий и межсессионная изоляция требуют CI нового кода. Production DA
остаётся legacy, N2/N3/N4 не объявляются полностью закрытыми.

## 2026-10-31 — N3 локальная приёмка native preview на `46339cc`

Проверенный код: `46339ccf0bb46fb15ef4ab4fcc4cbfaa5baba0b7`. Все шесть
CI jobs завершены успешно: [SDK — 3 jobs](https://github.com/boogy777-lgtm/OPCLook/actions/runs/41124600706),
[COM — 1 job](https://github.com/boogy777-lgtm/OPCLook/actions/runs/41124600770),
[приложение Linux и Windows x86 — 2 jobs](https://github.com/boogy777-lgtm/OPCLook/actions/runs/41124600679).
Windows прошёл fmt, сборки, изолированные COM/VARIANT, shared/CLI, библиотечные
и интеграционные тесты, net helper и диагностический UA loopback. Отдельные
внешние live tests остаются ignored; успешный job не означает их выполнение.

Реальный Win32 SDK barrier probe воспроизвёл уничтожение временного context
на worker и self-wait старого порядка; общий EphemeralContext завершил worker
и уничтожил context на caller. SDK callback → C ABI → Rust mapping прошёл;
его stand завершил 4 теста. Строгий Gateway прошёл 3/3 отдельных процесса
(измерено: 0.89, 0.89, 0.85 с) с новым сервером и cleanup. Сценарий включает
Discover/Browse, Unicode ID и BSTR, scalar/array read-write-readback,
reconnect, subscription и registration guards.

Это измеренный локальный SDK-server preview, не завершение N2/N3/N4.
Три успешных процесса не доказывают отсутствие всех гонок или единственную
причину прежнего AV. Production DA остаётся legacy. Далее по R00/R03/R09:
проверить remote/другой сервер, late callback и failure/timeout teardown,
изоляцию сессий и license closure; затем решать N4. Настоящая запись
документирует указанный code head; последующий documentation-only commit
не является новым измерением runtime. Ниже сохранена история этапов.

## 2026-10-31 — N3 SDK временный worker ownership (Windows CI ожидается)

Pinned SDK source: `SOCltTask` держит ссылку на element до destructor, но
worker сигнализирует completion до release своей task-ссылки. Временный
resolve/discover context мог быть уничтожен на собственном worker и ждать
самого себя; прежний direct `SODaCComServerDefault` дополнительно не получал
обязательный `initClt`, оставляя COM shutdown sink неинициализированным.
Оба вызова теперь используют общий scoped `SOCltServer` как готовый SDK task
worker; caller удерживает его до штатного SDK shutdown/join. Он не добавляет
OPC или COM data-plane. Реальный SDK barrier probe сравнивает unguarded
interleaving в отдельном watchdog-процессе с guarded cleanup и проверяет поток
destructor; Win32 build и запуск пока не измерены. Ни этот source defect, ни
исправление ещё не доказаны причиной прошлых access violation.

## 2026-10-31 — N3 intermittent reconnect crash

На `4d22079` [COM fixture](https://github.com/boogy777-lgtm/OPCLook/actions/runs/41122495828)
первый Gateway процесс прошёл весь strict сценарий и cleanup, включая массивы,
Unicode, reconnect и subscription. Второй независимый процесс прошёл тот же
путь до Disconnect, но получил `0xc0000005` после отправки следующего Connect;
третий не запускался. Это 1/2 выполненных процесса, не зелёный gate. Добавлены
test-only SDK markers для resolve, открытия и закрытия сессии, а также
наблюдатель AV с адресом/модулем без перехвата исключения; причина и исправление AV
ещё не установлены. Известный из source lifetime риск временных discovery/
resolve worker отдельно требует регрессии и не считается причиной без доказательств.

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
