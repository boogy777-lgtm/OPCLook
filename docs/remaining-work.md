# OPCLook — оставшаяся работа и выполнение парами

Дата: 2026-10-31. Поручение владельца: продолжать все оставшиеся задачи плана
парами исполнителей GPT-6.1 Terra High. Это рабочая очередь существующих
планов, не новый функциональный scope и не утверждение о завершении миграции.

## Последний подтверждённый срез

`f787f43` прошёл все9 jobs (SDK4/COM2/App2/package1). Новый test-only
Gateway Async fixture прошёл1/1 в обоих i686 COM inputs: SDK0.32с,
official-MIT0.33с, обе strict cleanup PASS. Generic single-slot value fixture
и прежние native gates также прошли. Production Async остаётся Unsupported.
Это заменяет pending verdict исторических строк только для вошедшего кода;
accepted test-only correlation не означает production transaction/cancel policy.

SDK readiness/racefix/drain принят как воспроизводимый исследовательский
артефакт: [точные измерения и границы](testing/ua-sdk-readiness-drain-proof.md).
Fresh Engine/Gateway15.45с, SDK-observed Closed/Complete30.93с, recovery16/16,
ACK Drop/incarnation2/2. Одна ReactivatedSession/subscription, closed gate →
metadata-ready ACK → SDK replay/drain → snapshot53.5 → push54.5; history unknown.
Production SDK pin/App не меняются; общий порядок Push/Read, NewSession,
несколько subscriptions и secure recovery этим кандидатом не приняты.

## Правило исполнения

1. Выбирать две независимые доступные задачи с учётом зависимостей и фактического
   кода. Каждому исполнителю — отдельный worktree, branch и CARGO_TARGET_DIR.
2. До кода фиксировать Class/Inv/N+1/Defect и владельцев файлов. Общие contract,
   workflow, status, audit и routing интегрирует родитель после review.
3. Использовать API выбранных SDK. Не создавать второй OPC transport, scheduler,
   recovery или COM owner для библиотечной способности.
4. Интегрировать пару одним пакетом проверки. Различать code head, результаты
   CI, mock/stand и live. Не отменять нужные проверки новыми несвязанными правками.
5. После проверки переходить к следующей паре. Внешний блокер одной задачи
   фиксировать и продолжать независимые задачи; не выдавать mock за стенд.
6. Не запускать новые функции вместо исправления подтверждённого regression
   текущего пакета. Production переключать только после его собственных гейтов.

## База

- A–H и B1–B20 базового рефакторинга: выполнены согласно refactoring-status.md.
- P1, темы: выполнена. Не повторять эти изменения из старых чекбоксов.
- Native preview code `46339cc`: 6 CI jobs и 3 отдельных Gateway процесса прошли.
  Это локальная приёмка, не закрытие всех N2/N3/N4 и R2.

## Пары и измерения

| Пара | Задачи | Результат |
| --- | --- | --- |
| 01 | N2.TEST, N3.ISOLATION | `36fa668`: все 6 CI jobs success, Gateway 3/3 и live two-session gate 1/1; см. status |
| 02 | N3.PENDING-IO-SHUTDOWN, N1.LICENSE inventory/guard | `70357117`: SDK3/3 и COM success, shutdown5.003833с/Busy/release/cleanup; app2/2, все6 jobs success |
| 03 | R2 UA CacheAllowed/maxAge, N1 official MIT OPC Core build inputs | `690a830`: все8 jobs success, wire1234/0 и обе Win32/COM matrix прошли |
| 04 | R2 negotiated subscription facts, N2L09 SDK host reuse | `690a830`: sample1000→2000 и host reuse в обоих native matrix прошли; UI/recovery ещё open |
| 05 | R2 legacy profile migration M03/M04, N2L10 Write-task exception | df2bf1d: SDK4+COM2+Linux success; Windows PKI isolation-test failure, bounded driver fix pending |
| 06 | R2 per-key Read terminal API, P4 typed palette/epoch guards | cb3: SDK4/COM2 success; App252 library tests/platform прошли, menu-label assertions fixed47e6558; actual counterfactual/4PNG pendingCI |
| 07 | E02+GUI/CLI read-outcome consumers, preferred MIT/no-hook preview package | Code reviewed;6 Engine+2 buffer tests и7 packaging+4 license tests локально; actual app/package pendingCI |
| 08 | R2.T distinct time facts/ordering, N2 Device/Fresh read | Code reviewed;5 time policy+1 propagation и43 native mock tests; integration/Win32/GPU pending |
| 09 | Engine native Fresh/readback integration, CI/nohook packaged CLI proof | Code reviewed; actual Engine/CLI/Linux tests прошли, новый Win32/package proof pending |
| 10 | P2 payload limits, P3/P5 embedded/numeric controls and inventory | Реальные Engine/App/CLI и GUI tests прошли; PNG/native Windows input отдельно |
| 11 | SDK UA recovery/identity, SDK DA Async source/prototype | UA3 live scenarios pass privately; strict-first notification failed; Async test-only/no production cap, i686 pending |
| 12 | G/D/fairness/Drop, UA Engine/App/CLI consumers | be960ff: App2/SDK4 success,8PNG viewed; COM cleanup иpackage reverse repair pending; UA branch ещё private |
| 13 | SDK async prototype review, UA Browse/metadata identity | Typed raw HRESULT review и SDK pre/post scope tests локально; actual Asynci686 pending |
| 14 | Late Add quarantine, UI context generation | SDK3 cut/timeout tests pass; Engine91/App267 private context tests; следующий integration/CI pending |
| 15 | UA per-key metadata/negotiated UI, SDK notification facts | Linux Engine93/App268/backend264+1knownignored; real SDK access/restart/gap/overflow and panel render passed; new WinCI pending |
| 16 | SDK first-notification and Republish evidence, DA failure facts | Server defect and client gap isolated; DA mock47/48 and typed facts; i686 strict test pending |
| 17 | N1 verified notice texts, SDK-only replay prototypes | Inventory183/94/295854bytes/missing0; library before/after stand; production pins unchanged |
| 18 | Initial Connect/recovery scope, SDK checked Async lifetime | Initial scope accepted in b94303f; checked lease remains test-only, own Win gates pending |
| 19 | Native session/manager lease, SDK fault probes | Linux runtime50/50; Deadline/cut/watch evidence recorded; new i686 lease execution pending |
| 20 | Primitive arrays coverage/BOOL validation, Engine SDK replay boundary | Existing codecs reused; before/after BOOL regression; Engine1/1 + fresh runner1/1; Win arrays pending |
| 21 | Actual native CI wiring, SDK failure-visibility gate | Both COM inputs wired; actual SDK window guard PASS + guard-only counterfactual FAIL, fresh runner reproduced |
| 22 | Native async two-session isolation, secured UA reconnect | Mock51/51; real secure1/1 + fresh runner1/1 NewSession; found/fixed SDK base-destruction retirement order, new Win CI pending |
| 23 | A08 late SDK callback, TransferSubscriptions acceptance | Held actual-callback gate prepared, Win pending; public secure Transfer Good raw observer1/1+fresh1/1, destination mapped state still absent |
| 24 | O23 active subscription + Sync one-shot, secure token renewal | Engine/SDK request-counter gate prepared, Win pending; original SDK verified secure token1→2, same Session/sub/item,1/1+fresh1/1 |
| 25 | A09 actual item/quality failures, SDK bounded readiness handoff | A09 passed both81f/6c COM; readiness fixes reviewed as research, no production pin/Async switch |
| 26 | Forced SDK DA1 source route, generic single-slot Async values | DA1 passed both81f COM; generic fixture passed both6c COM with cleanup; no physical DA1/productionAsync claim |
| 27 | Native generic CI acceptance, SDK shared recovery guard and scoped drain | 6c all9green; SDK16+ACK2 and fresh Engine15.45с/Closed30.93с research accepted; one-subscription ReactivatedSession only |
| 28 | SDK namespace-cache identity audit, test-only native Async Gateway correlation | Namespace source-only risk, no measured fix; Gateway mock54/49-nohooks, Linux typecheck and both f787 COM live gates passed; no production Async claim |

Пара 01 исправила начальное состояние fixture после честного отрицательного
NOCONFIG verdict на `a004b226`. Проверка на `36fa668` подтвердила
RUNNING → FAILED → RUNNING во всех трёх процессах и read/write/read-back B
до освобождения device-write A. Это не remote/другой DA server proof.

## Очередь по зависимостям

Точный состав следующих пар уточняется по результатам предыдущей. Пункты одной
строки не означают обещание общего коммита или одновременную правку одного файла.

| ID / план | Оставшаяся способность или проверка | Условие закрытия |
| --- | --- | --- |
| N3.LIFETIME | Late callback, remove/re-add/epoch, failure/timeout teardown и независимость сессий | Реальные entry points, содержательные negative cases, bounded cleanup; локальный proof не назван remote |
| N1.LICENSE | Полный состав зависимостей собираемого native SDK и notices | Проверенный source/build closure, версии/лицензии/распространяемые notices; новых прав использования не предполагать |
| R2.3 / N5 | UA CacheAllowed/maxAge и negotiated requested/revised параметры | Ready SDK API/config, общий capability и наблюдаемые фактические параметры |
| R2.3 / M03–M04 | Миграция старых профилей и прежнего fallback | Существующее поведение не меняется молча, профиль и уведомление сохраняются и тестируются |
| R2.1–R2.2 / R2.4 | Per-key terminal outcomes, coalescing и fairness с Browse | Каждая действительная потребность имеет исход; занятый слот не теряет новые цели, другая работа не голодает |
| R2.5 | Source/server/received времена и происхождение результата | Метки раздельны от mapping до UI; старый Read/Push не выдаётся за новое состояние, операция завершается отдельно от отображения |
| R00 / R04–R12 / N5 | Оставшиеся Foundation cases: SDK recovery, queues/filter, limits, aliases и initial result | Сверены locked SDK и O-таблицы; библиотечные способности не реализованы повторно, gaps видны |
| P2 | Общие пределы размера/глубины payload и ранняя форма действия | Один типизированный механизм для GUI/CLI, сохранён порядок ошибок и подтверждений |
| P3 | Оставшиеся контекстные клавиши, фокус, встроенные виджеты и IME | Headless регрессии и честно отделённые ручные платформенные проверки |
| P4 | Ввод параметров Browse/Write в палитре и клавиатурное открытие | Тот же каталог/Engine/подтверждение, что у остальных способов вызова |
| P5 | Обследование и выделение повторяющихся компонентов UI | Небольшие show(model, style) компоненты с тестами; без собственного backend/state owner |
| N4 | Production native DA и удаление прежнего OPC COM data-plane | Native SDK capability, interop/lifetime/license gates закрыты; системный WinAPI/helper не удалён как OPC code |
| R2.6 / N6 | Полная матрица G/D/E/S/T/C/U/M/A/O, упаковка и согласование планов | Доказательства по стабильным ID, точный code head, итоговые настройки/документы и CI; число строк CSV не число пройденных тестов |

Пара28 приняла test-only DA Async Gateway RequestId/epoch/registration
fixture на обоих f787 Win32 inputs. Production facade/cancel/version/cleanup
policy и late callback через новый Connect остаются отдельными гейтами. Namespace-cache
[source audit](testing/ua-sdk-namespace-cache-audit.md) выявил риск по SDK source,
но не дал измеренного defect/fix: этот gate остаётся открытым. Непроверенный
SDK prototype не включать в production или в accepted patch packet.
Остальные DA method/capability и UA recovery gates выбирать по source audit. P2 и автоматизируемые P3/P4 gates
уже выполнены; physical Windows и N4 нельзя закрыть локальными mock/fixtures. Подтверждённые regressions исправлять перед новым
функциональным пакетом. Дальнейшие пары выбирать по фактическим зависимостям.

## Внешние гейты

Remote/другой DA server, реальные UAC/маршруты и ручные Windows IME/раскладки
не доказываются текущим локальным SDK fixture или синтетическими событиями.
Если нужного стенда нет, сохранять эти гейты открытыми и выполнять остальные
задачи. Не ослаблять критерии production переключения без решения владельца.

## Источники

- [Текущий статус](status.md) и [routing](llm-update-routing.md).
- [R2](value-refresh-plan.md), [native миграция](native-backend-migration.md).
- [P1–P5](next-ui-command-plan.md), [матрица рефакторинга](refactoring-status.md).
- [Таблицы приёмки](testing/acquisition-truth-tables.md).
