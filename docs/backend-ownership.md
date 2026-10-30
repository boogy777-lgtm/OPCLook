# R2.REUSE — границы готовых библиотек и кода OPCLook

Дата: 2026-10-30. База: `main@4738cbd9ace14bdd3f59808f40ffa7f8c635a2f0`.
Вход: [routing table](llm-update-routing.md).
Связаны [проверка OPC Foundation](opc-foundation-gap-review.md),
[AcquisitionConfig](connection-acquisition.md) и [таблицы тестов](testing/acquisition-truth-tables.md).

Обязательное уточнение владельца: [native SDK для обоих протоколов](native-backend-migration.md).
Собственный OPC data-plane в приложении не является допустимой альтернативой.

## 1. Ответ на риск дублирования

Да, протокольные требования нельзя автоматически превращать в новые компоненты
Engine. В UA уже используется async-opcua, в DA — собственный worker поверх
COM bindings. Общий scheduler остаётся оправданным только для продуктовых
потребностей: объединить начальный снимок/read-back/Refresh/Poll, ограничить
нагрузку и одинаково обслужить GUI/CLI. Он не реализует протокол повторно.

Перед реализацией R2 выполнить R2.0: проверить установленную библиотеку и её
готовый API. Наличие API ещё не доказывает конкретный автоматический сценарий.
И отсутствие имени Republish в App ещё не доказывает, что SDK его не выполняет.

## 2. Карта владельцев

| Поведение | Основной владелец | Что остаётся OPCLook | Чего не добавлять |
| --- | --- | --- | --- |
| UA SecureChannel, encoding, service request/response | async-opcua | Конфиг соединения и отображение результата | Собственный сетевой стек/общую transport correlation |
| UA Publish pipeline и acknowledgements | SDK event loop | Запуск/завершение владельца, видимость состояния | Второй Publish timer в Engine |
| UA reconnect/Activate/Transfer/replay | Сначала SDK; точный охват подтвердить по закреплённым исходникам | Нормализованные события Recovering/Ready/Gap, terminal restart | Параллельный recovery loop поверх живого SDK loop |
| UA request timeout/cancel | Сначала SDK request lifecycle | Продуктовый deadline и unknown outcome; текущий quarantine не убирать без proof | Второй wire-timeout/retry механизм без доказанного пробела |
| UA revised intervals/filter/queue | SDK service result/cache + тонкий адаптер | Requested/effective контракт и UI | Угаданные revised значения |
| UA Republish/ResendData/Read snapshot | SDK service/API и recovery-владелец | Решение о снимке только после реального recovery outcome | Read, который объявляет восстановленной утраченную историю |
| DA COM apartment, memory, callback lifetime | Выбранная native DA library; текущая собственная реализация подлежит замене | Проверка границ и результат для contract | Второй apartment/worker/callback owner вокруг готового владельца |
| DA Async transaction/callback routing | DA library/adapter | Mapping к RequestId и Connect/registration identity | OnDataChange=stream без проверки transaction/kind |
| Нужен snapshot/read-back/Refresh | Существующий Engine | Общая очередь целей и терминальных исходов | Собственная очередь в каждом экране GUI |
| Coalescing и один логический read-slot | Существующий Engine scheduler | Продуктовое ограничение нагрузки | Копия transport queue/response IDs библиотеки |
| Ограничения размера протокольной пачки | Адаптер/готовая library capability | Ограниченный общий payload и справедливость сессий | Независимые splitters на каждом слое |
| Freshness и raw value/status/time | Библиотека получает; adapter нормализует | Сохранение фактов, видимость best effort/unknown | Обещание физической свежести по одному Ok |
| Выбор режима и редактирование профиля | Общий contract + Engine validation; UI thin client | Сохранённый профиль и snapshot эпохи | Протокольная логика в UI |

Два ID не всегда дублирование: RequestId продукта связывает логическую команду,
а wire requestHandle/DA transaction связывает транспорт. Mapping принадлежит
адаптеру. Две независимые таблицы одного и того же transport lifecycle —
дублирование, которое требует устранения.

## 3. Что установлено и что ещё не доказано

### UA

`crates/backend/Cargo.toml` содержит async-opcua 0.19.
`ua/session/connection.rs` запускает SDK event loop.
Публичные страницы [Session](https://docs.rs/async-opcua-client/latest/opcua_client/struct.Session.html)
и [SessionEventLoop](https://docs.rs/async-opcua-client/latest/opcua_client/struct.SessionEventLoop.html)
описывают service APIs и loop, включая Republish API.
Проверка по moving latest URL дала смешанные версии страниц:
поисковый/API результат показывал 0.19, открытый connect source — 0.16.
Поэтому точный auto-recovery/replay охват 0.19 не объявлен проверенным.
Cargo.lock этой базы проверен: async-opcua и async-opcua-client = 0.19.0.
В новом чате открыть source именно locked версии.
Не обновлять SDK автоматически и не писать обход, пока этот пробел не выяснен.

### DA

Нынешний backend — IOPCSyncIO::Read(Cache), active group/items, свой COM worker.
Generated IOPCAsyncIO/IOPCDataCallback не равны готовому Async/stream механизму.

Найдены первичные API-кандидаты для замены собственного COM data-plane:

| Кандидат | Что видно в публичном API | Что необходимо доказать до выбора |
| --- | --- | --- |
| [opc_da 0.3.1](https://docs.rs/opc_da/latest/opc_da/client/index.html) | v1/v2/v3, unified facade, AsyncIo traits, DataCallback | Рабочий callback stream, transaction/lifetime, типы, x86 Windows, raw statuses и cleanup |
| [opc-da-client 0.2.0](https://docs.rs/opc-da-client/latest/opc_da_client/) | Асинхронный Rust facade с COM worker, read/write/browse | await не доказывает native async I/O; subscription/read-source/raw results проверить отдельно |
| [bytehound-opc-da-client 0.3.1](https://docs.rs/bytehound-opc-da-client/latest/opc_da_client/) | Provider/worker и bounded browsing/inventory | Не путать inventory stream с DA value subscription; проверить нужную capability |

Это кандидаты для feasibility-проверки, не выбранные зависимости и не утверждение
промышленной надёжности. Документация API не заменяет Windows/DA стенд.
В этом коммите зависимости не добавляются, лицензии проекта не меняются.

## 4. Решение use/configure/adapt/extend

| Найденное положение | Следующий шаг |
| --- | --- |
| Готовая библиотека уже реализует инвариант | Использовать её API |
| Поведение управляется её настройкой | Передать конфиг и показать effective значение |
| Результат есть, но не попадает в общий contract | Тонкий adapter mapping; не новый runtime service |
| Нужная продуктовая политика не является функцией SDK | Расширить существующий Engine scheduler/intent |
| В SDK воспроизводимый дефект | Минимальный reproducer; исправление самой библиотеки или смена кандидата, без обхода в App/Engine |
| В SDK нет capability | Сменить native backend за тем же contract; до этого явный Unsupported, без самостоятельного OPC слоя |
| Непонятно, кто выполняет recovery | Сначала source/trace audit; не запускать второго владельца наугад |

## 5. Обязательный выход R2.0 для следующего агента

Для каждой строки маршрута записать:
`Requirement -> library/version/API -> actual trace/test -> missing part ->
owner -> selected use/configure/adapt/extend -> removal of duplicate path`.

Текущий quarantine после UA IO timeout сохранить до проверки locked SDK
request-timeout/late-response/cancel поведения. Это защитное поведение baseline,
не обязательный запрет OPC. Если библиотека уже безопасно изолирует timed-out
request, устранение избыточного quarantine требует reproducer/регрессии и
согласованной правки O29/E/S, а не ещё одного слоя обхода.

Проверки: ровно один session recovery owner; нет параллельного повторного
создания subscriptions; один источник фактических revised параметров;
нет потери исходных StatusCode/quality/time; old callbacks отвергаются;
изолированные сессии не делят изменяемый протокольный контекст.
Подмена backend проверяется теми же контрактными таблицами, а не новой логикой UI.

R2.3 описывает wiring/config выбранного native backend и удаление собственного
OPC COM data-plane. Если кандидат не покрывает обязательную способность, кандидат
меняется. Список мест изменения не разрешает писать свой аналог библиотеки.
Разрешено расширение продуктовой политики Engine, когда это не функция SDK.

### N1: дополнительный Softing SDK

Public SODaC/SOClt/SOCmn остаются владельцем DA protocol lifecycle.
Проверка исходников v4.47.1 выявила неправильное сопоставление HRESULT
в SDK callback; детали, source-method reproducer и проект upstream patch —
[здесь](testing/native-sdk/softing-4.47.1.md). Win32 direct core link, common/entry init/cleanup и offline callback до/после
patch проверены в [run 41067236324](https://github.com/boogy777-lgtm/OPCLook/actions/runs/41067236324)
на head `d375f15f4b57837baeec75d1cf3a7c9b843cb96e`. N2 должен использовать
этот direct core; DaSession/TBC имеет только header probe, OTB не подключён.
Real COM advise, late teardown, live I/O, типы, isolation и полная лицензия
closure ещё не доказаны; production owner не изменён.

### N2.1 offline boundary

SDK по-прежнему владеет callback/VARIANT; `native/softing-da` копирует только
ограниченный последний callback. Rust адаптер отображает VT_I4 и сохраняет
raw факты отдельно от DataValue. Это не очередь, runtime binding или второй
backend; serialization вызывает хост. Identity session/registration и lifecycle
требуют N3, production ownership не изменён.
