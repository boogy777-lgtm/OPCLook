# R2.TEST — таблицы истинности и переходов получения данных

Дата: 2026-10-30. База анализа: `main@62ad187c2294cff5df55fe40e82b05f5dda25617`.
Назначение: независимое задание агенту нового чата для написания тестов.
Статус: нормативные ожидаемые результаты; это не отчёт о пройденных тестах.
Связанные документы: [R2](../value-refresh-plan.md),
[конфиг соединения UA/DA](../connection-acquisition.md).
Вход для нового чата: [routing table](../llm-update-routing.md).
Сначала R2.0: [не дублировать готовые библиотеки](../backend-ownership.md).

## 1. Как использовать задание

Прочитать AGENTS.md, status, R2 и конфиг, затем сверить текущий HEAD.
Сначала реализовать тесты на настоящих contract/Engine/App/adapter entry points.
Mock backend фиксирует команды и инъецирует события; он не вычисляет решение
Engine вместо Engine. Не тестировать копию формулы как будто это продукт.

Каждое правило/строка имеет стабильный ID для имени теста и трассировки.
`1` — истина, `0` — ложь, `*` — оба значения. В сокращённых таблицах строки
применяются сверху вниз; первая подходящая строка задаёт результат.
Полные CSV не содержат `*`. Неприменимый параметр всё равно варьируется:
он не должен случайно влиять на результат. Булевы guard-таблицы не описывают
асинхронные переходы; для этого обязательны E/S/T.

В тестовой модели ключ имеет identity=(session, Connect epoch, registration,
ItemKey). Read хранит набор покрытых demand identities; HashSet одного ключа
без отделения pending/in-flight недостаточен. registration — логическая
идентичность добавления; не обязательно новый публичный тип. Не переиспользовать
удалённый ItemKey в одной эпохе; если reuse нужен, добавить generation guard.

Время — fake monotonic clock, не sleep. Считать команды и исходы; для blocked
ожидать отсутствие dispatch и сохранение ещё действительной потребности.
Локальные операции не являются обещанием физической свежести сервера.

## Измеренный частичный срез 2026-10-30

Текущий PR добавляет настоящие Engine тесты `old_read_reply_error_or_synthetic_timeout_does_not_cover_new_write` (покрытие старого Read, coalescing и push), `new_write_after_reservation_and_remove_are_distinct` (повторный demand после reservation и Remove), `e06_active_subscription_cannot_release_reservation`, `old_write_and_read_replies_do_not_cross_reconnect`, `write_opt_in_only_accepts_matching_good_replies` и `unsupported_da_readback_fails_before_write_and_uncertain_never_queues`. CLI `readback_failure_and_quality_preserve_accepted_write_record` проверяет сериализацию/quality; `wrong_key_write_reply_is_unknown_and_initial_read_is_not_readback` проверяет CLI event path и покрытие permit; `browse_read_write_push_forced_poll_and_idle_interrupt` также выполняет UA sample-server opt-in read-back. Они покрывают отдельные переходы E06/E08–E10/E13/E15–E16 и S01/S02/S04/S07/S17/S18, а не целиком соответствующие строки. Timeout тест — синтетический Engine timeout с работающим транспортом, не native UA IO quarantine S03. Общее per-key terminal API для многоключевого E11, fairness, полные G/D таблицы, физический DA/live и остальные случаи остаются открытыми.

### Частичный R2.3 профиль

`legacy_none_roundtrip_and_unknown_or_partial_profiles_fail`, `invalid_acquisition_interval_is_reported_with_backup`, `validates_known_adapter_capabilities_and_interval_boundaries`, `c14_c22_connection_profiles_are_snapshotted_and_poll_independently`, `c23_c24_profile_write_and_invalid_connect_guard` и `explicit_failed_subscription_has_no_hidden_periodic_poll` проверяют части M/C/U через реальные contract/backend/Engine entry points. `explicit_cli_poll_alias_uses_same_profile_and_rejects_invalid_options`, `explicit_poll_reads_without_monitored_value_stream`, GUI `acquisition_*`, `explicit_profiles_do_not_reconfigure_when_global_options_change` и `same_endpoint_with_different_profiles_creates_distinct_sessions` проверяют части C/U/GUI; это не полное покрытие таблиц. Полноценная миграция M03/M04, reported revised interval, UA CacheAllowed, DA Subscription/Device/native SDK и G/D/fairness остаются открыты; редактор активного профиля покрыт частично ниже.

### Редактор активного профиля — границы покрытия

`profile_save_auto_and_manual_reconnect_keep_distinct_snapshots` проверяет C16
через реальные App/Engine Connect и команды; `profile_rejects_invalid_save_collision_and_stale_epoch`,
`profile_late_collision_and_capacity_preserve_staged_choice` и
`profile_connect_dispatch_failure_keeps_saved_candidate` проверяют части C18,
включая отказ до остановки и сохранение отложенного выбора. Тест
`profile_reconnect_rejects_old_read_write_and_push_and_reregisters_rows`
покрывает часть C17 и корреляцию S13/S14 в обе стороны с реальными App/Engine событиями,
но не доказывает поведение физического сервера. GUI тесты
`active_profile_cancel_keeps_active_saved_and_staged_profile` и
`active_profile_enter_message_and_missing_target_are_safe` проверяют C15,
клавиатуру, приоритет сообщения и видимое отсутствие цели. Остальные требования
таблиц, включая native UA timeout quarantine S03, DA live и fairness, не закрыты.

### N1: проверка кандидата SDK

`reproduce-softing-status.py` использует точный исходник SDK и fake зависимости:
14 read-прогонов (два режима × семь наборов handle), те же семь write наборов
повторяются в двух режимах. Он обнаруживает несоответствие HRESULT до patch,
после patch проверяет identity/value/quality/time/status и неизменность входа.
[Run 41067236324](https://github.com/boogy777-lgtm/OPCLook/actions/runs/41067236324)
на head `d375f15f4b57837baeec75d1cf3a7c9b843cb96e` дополнительно вызывает
реальный слинкованный Win32 SDK callback offline до/после patch, проверяя
identity, raw status и неизменность input, а также common/entry lifecycle.
Это не закрывает O23–O26/A04–A10: COM advise, late teardown и физический DA
сервер отдельно обязательны. Подробности —
[Softing N1](native-sdk/softing-4.47.1.md).

## 2. G — полная таблица допуска Read

Файл: [acquisition-read-gate.csv](acquisition-read-gate.csv).
Все 128 комбинаций G000–G127. Порядок битов: C,E,K,I,B,D,A.

| Бит | Значение |
| --- | --- |
| C | Сессия Connected |
| E | Snapshot конфига и потребность принадлежат текущей действующей эпохе |
| K | Есть хотя бы один зарегистрированный непустой набор ключей для чтения |
| I | Уже зарезервирован/отправлен логический Read этой сессии |
| B | Есть выполняющийся Browse; только очередь следующего Browse не равна busy |
| D | Есть допустимая по времени потребность; вычисляется таблицей D |
| A | Адаптер умеет выполнить требуемый Read method/freshness |

`emit_read = C & E & K & !I & !B & D & A`.

| ID правила | C | E | K | I | B | D | A | emit_read | Проверка |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| G-R1 | 0 | * | * | * | * | * | * | 0 | Нет обращения к отключённой сессии |
| G-R2 | 1 | 0 | * | * | * | * | * | 0 | Старая эпоха не исполняется |
| G-R3 | 1 | 1 | 0 | * | * | * | * | 0 | Пустой Read не отправляется |
| G-R4 | 1 | 1 | 1 | 1 | * | * | * | 0 | Один read-slot, включая reservation до dispatch |
| G-R5 | 1 | 1 | 1 | 0 | 1 | * | * | 0 | Browse не перекрывается |
| G-R6 | 1 | 1 | 1 | 0 | 0 | 0 | * | 0 | Нет пустой фоновой активности |
| G-R7 | 1 | 1 | 1 | 0 | 0 | 1 | 0 | 0 | Видимый Unsupported; не бесконечное ожидание |
| G-R8 | 1 | 1 | 1 | 0 | 0 | 1 | 1 | 1 | Одна объединённая пачка, слот занят |

G проверяет чистый допуск. E отдельно проверяет отмену при Disconnect/epoch,
ошибку Unsupported и освобождение reservation при неуспешном dispatch.

## 3. D — полная таблица возникновения потребности

Файл: [acquisition-demand.csv](acquisition-demand.csv).
Все 128 комбинаций D000–D127. Порядок битов: P,T,N,H,W,R,F.

| Бит | Значение |
| --- | --- |
| P | Выбран Poll; 0 означает Subscription |
| T | Наступил срок периодического опроса |
| N | Требуется InitialSnapshot: регистрация подтверждена, конечного результата нет |
| H | Истёк один пересмотренный интервал ожидания первоначального stream-результата |
| W | Принят успешный Write, для которого ещё не выполнено последующее наблюдение |
| R | В snapshot конфига включён read_after_write |
| F | Есть явный пользовательский Refresh/одноразовый Read |

`demand_due = (P & T) | (N & (P | H)) | (W & R) | F`.

Полезные частные строки (полная комбинация — в CSV):

| ID | P | T | N | H | W | R | F | demand_due |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| D-R1 | 0 | 1 | 0 | * | 0 | * | 0 | 0 |
| D-R2 | 1 | 1 | 0 | * | 0 | * | 0 | 1 |
| D-R3 | 0 | * | 1 | 0 | 0 | * | 0 | 0 |
| D-R4 | 0 | * | 1 | 1 | 0 | * | 0 | 1 |
| D-R5 | 1 | * | 1 | * | 0 | * | 0 | 1 |
| D-R6 | * | 0 | 0 | * | 1 | 0 | 0 | 0 |
| D-R7 | * | 0 | 0 | * | 1 | 1 | 0 | 1 |
| D-R8 | * | * | * | * | * | * | 1 | 1 |

W означает принятый успешный Good-результат Write, а не нажатие кнопки,
Wrote Err или Uncertain. Good_CompletesAsynchronously сохраняет отдельную
степень подтверждения (O14); read-back не превращает её в доказанное действие.
Read, уже отправленный до Wrote Ok, не удовлетворяет W.
Конечный Bad-результат InitialSnapshot завершает попытку с ошибкой.
UA Bad_WaitingForInitialData не является конечным снимком.
Одноразовая попытка заканчивается успехом/ошибкой/отменой; таблица не разрешает
заново создавать N на каждом tick после терминальной ошибки.

## 4. U — постоянный канал и возможности

| ID | Mode | Stream capability | Read capability | Ожидаемый канал и результат |
| --- | --- | --- | --- | --- |
| U01 | Subscription | 1 | 1 | Stream; периодический Read=0; разовые Read разрешены через G |
| U02 | Subscription | 0 | * | Unsupported; не подменять Poll |
| U03 | Subscription | 1 | 0 | Конфиг с read_after_write=true не принимается без Read; при false stream возможен, недоступный Refresh имеет причину |
| U04 | Poll | * | 1 | Периодический Read; постоянная доставка stream=0 |
| U05 | Poll | * | 0 | Unsupported; не подменять Subscription |
| U06 | Subscription + Failed | ранее 1 | 1 | Failed остаётся видимым; разовый снимок не делает stream Ready |
| U07 | Subscription + partial monitor failure | 1 | 1 | Ошибочные ключи видны; poll_only не включает скрытый цикл |
| U08 | Poll + async Read | * | 1 | Callback завершения запроса разрешён; OnDataChange не применяется как штатный канал |
| U09 | Две сессии | разные | разные | Режим/ошибки/слоты одной сессии не меняют другую |

В U03 stream-only профиль без read_after_write допустим с явной диагностикой
недоступных initial fallback/Refresh. Нормативный исход: stream доступен,
одноразовый Read недоступен. Это разделение capability, а не скрытый fallback.

## 5. C — настройки соединения и границы

| ID | Вход/действие | Ожидаемый результат |
| --- | --- | --- |
| C01 | UA Subscription + capability | Конфиг в EndpointSpec; MonitoredItems; интервал означает publish request |
| C02 | UA Poll + capability | Read по интервалу; monitored values не создаются |
| C03 | DA Subscription без callback реализации | UI disabled с причиной; Engine/API отклоняет конфиг |
| C04 | DA Poll + Sync capability | Явный sync Poll; результат коррелирован с RequestId |
| C05 | DA Poll + Async capability | Async Read + transaction mapping; нет постоянного data-change канала |
| C06 | DA Subscription + Sync read method, обе capability есть | Комбинация разрешена; разовый Read не создаёт периодический Poll |
| C07 | DA Asynchronous выбран, доступен только Sync | Unsupported; никаких тихих подмен |
| C08 | interval_ms = 0 / 3600001 | Ошибка до side effects |
| C09 | interval_ms = 1 / 3600000 | Принят requested interval; adapter/server вправе revise |
| C10 | UA revised interval отсутствует/0/NaN/infinity | Ошибка канала; не нулевой timer loop |
| C11 | UA absolute deadband <0 / NaN / infinity | Ошибка конфигурации |
| C12 | DA percent deadband <0 / >100 / NaN / infinity | Ошибка конфигурации |
| C13 | DA percent deadband = 0 / 100 | Валидный конфиг; только Subscription применяет фильтр |
| C14 | Глобальный interval изменён при активных A/B | Активные snapshots A/B неизменны |
| C15 | Черновик A изменён, затем Cancel | Сохранённый и активный конфиги неизменны |
| C16 | Apply без reconnect | Сохранён профиль следующего ручного соединения; текущий snapshot и авто-reconnect неизменны |
| C17 | ApplyAndReconnect с валидным профилем | Старый канал закрыт, новая эпоха, новый snapshot |
| C18 | ApplyAndReconnect с невалидным профилем | Ошибка до остановки исправного старого соединения |
| C19 | Async DA completion пришёл | Reply только соответствующему Read; не Subscription event |
| C20 | UI/CLI передают одинаковый AcquisitionConfig | Одинаковые validation, scheduling и outcomes |
| C21 | --poll совместимый alias | Создаёт Poll config до connect, а не фильтрует готовую подписку |
| C22 | Две сессии одного URL/ProgID с разными config | Обе работают независимо; recent history не меняет snapshots |
| C23 | read_after_write=false и Wrote Ok | Write успешен, нового Read нет, текст не обещает read-back |
| C24 | read_after_write=true и Wrote Err | Ошибка Write, нового Read нет |
| C25 | Неизвестный update_mode из файла/API | Видимая ошибка; не Default/Auto/другой режим |
| C26 | read_after_write=true, FreshSource capability отсутствует | Конфиг отклонён; не Cache read-back под именем FreshSource |

## 6. E — очередь и терминальные результаты

| ID | Предусловие и событие | Ожидаемый результат |
| --- | --- | --- |
| E01 | AddItems: A Ok, B Err | Потребность только A; ошибка B видна |
| E02 | Initial A; early push A до AddItems reply | Начальный результат учтён, повторного snapshot Read нет |
| E03 | Initial A; UA WaitingForInitialData | Ожидание сохраняется; после grace одна попытка Read |
| E04 | Pending A + A + B, свободный слот | Один Read с уникальными A,B; covered demands сохранены |
| E05 | Poll due(A,B) + Refresh(B,C) | Один Read A,B,C; для B выбрать более строгую freshness |
| E06 | Due вернул Read, send ещё не выполнен, второй due | Второй Read=0 из-за reservation |
| E07 | Dispatch failed для reservation | Слот освобождён; покрытая попытка Failed/видима; нет потери новых demands |
| E08 | Reply R1 Values(A) полностью | Закрыть только R1; освободить слот; pending не очищать целиком |
| E09 | Reply R1 Error, transport usable | R1 Failed; слот свободен; новые pending сохранены; quarantine отдельно O29 |
| E10 | Timeout R1 | Один терминальный Failed; late R1 отклонён |
| E11 | R1 requested A,B; ответ содержит только A | A обработан; B MissingResult/Failed, не общий успех |
| E12 | R1 result A Bad/Uncertain | Сохранить статус и причину; Bad не считать пригодным значением; не retry-loop |
| E13 | Push Values во время R1 | Можно обновить строку; R1 и его слот остаются |
| E14 | Незапрошенный Read Error без R1 correlation | Диагностика; R1 не освобождается |
| E15 | Remove A при pending/in-flight A | Отмена A; поздний результат не создаёт строку и не меняет другую регистрацию |
| E16 | Disconnect при pending/R1 | Отмена работы эпохи; late Reply/stream старой эпохи отклонены |
| E17 | Unsupported Read freshness | Явный Failed; не другой источник и не зависшая очередь |
| E18 | Bad initial закончился; многократные tick | Новые одноразовые попытки не возникают сами |
| E19 | Частичный terminal R1 A Ok/B Bad | Исход отдельно по ключам; слот закрыт один раз |
| E20 | Max transport chunk меньше списка ключей | Ограниченные chunks; один логический слот; одна терминализация каждого covered demand |
| E21 | Повторный/чужой RequestId | Нет повторной терминализации и освобождения чужого слота |
| E22 | Удалённый ItemKey повторно добавлен | Новый ItemKey либо generation guard; старое событие не применяется |

## 7. S — обязательные последовательности гонок

R1/R2 — Read; W1/W2 — Write; d1/d2 — различные потребности.
После каждой стрелки проверять промежуточное состояние, а не только финальную ячейку.

| ID | Последовательность | Ожидаемый результат |
| --- | --- | --- |
| S01 | Read R1(A) -> W1 Ok(A)/d2 -> R1 Values | d2 остаётся; следующий свободный pass отправляет R2(A) |
| S02 | Read R1(A) -> W1 Ok(A)/d2 -> R1 Error, transport остаётся usable | d2 остаётся; R1 Failed; R2 разрешён |
| S03 | Read R1(A) -> W1 Ok(A)/d2 -> R1 Timeout с UA quarantine | Старой эпохе R2 не отправлять; d2 отменить при закрытии, новый snapshot после регистрации; late R1 отвергнуть |
| S04 | W1 Ok(A) -> W2 Ok(A) -> dispatch | Один Read A покрывает обе ещё не отправленные потребности |
| S05 | W1 Ok(A) -> R1 dispatch -> W2 Ok(A) -> R1 Values | Вторая потребность не покрыта R1; нужен R2 |
| S06 | W1 Err(A) -> несколько tick | Нет автоматического read-back, старое значение не заменено вводом |
| S07 | W1 Ok(A) -> push(A) -> R1 due | Push не закрывает read-back; R1 нужен |
| S08 | R1 -> Disconnect -> Connect(epoch2) -> R1 Values | epoch2 не меняется |
| S09 | Stream(epoch1) queued -> reconnect(epoch2) -> late stream1 | Новая строка/метки/статус не меняются |
| S10 | R1(A) -> Remove A -> Add same textual node as B -> R1 Values | B не меняется; имена узлов не заменяют identity |
| S11 | Browse1 -> Refresh pending -> Browse1 reply -> Browse2 queued | Refresh получает слот до Browse2; активные Browse/Read не перекрываются |
| S12 | Сессия A busy/failed -> due сессии B | B обслуживается; нет общего read-slot |
| S13 | Subscription -> ApplyAndReconnect Poll -> late old push | В Poll нет старого потока, события эпохи отвергнуты |
| S14 | Poll -> ApplyAndReconnect Subscription -> late old Read | Старый Read не меняет новую сессию; циклического Poll больше нет |
| S15 | Полный R1 reply -> duplicate R1 reply -> новый demand | Дубликат не завершает новую потребность |
| S16 | Reservation -> dispatch Exhausted/Closed -> tick | Нет вечного read_in_flight; видимая ошибка |
| S17 | W1 Ok -> R1 result value отличается от desired | Write success и read-back mismatch видны отдельно; не писать желаемое значение в строку |
| S18 | read_after_write=false -> W1 Ok -> штатный push/poll | Строка обновляется только полученными данными; автоматического read-back нет |

Для S11 арбитраж должен охватывать recursive Browse sender, а не только due.
Для DA реализация может сериализовать Read/Write внутри сессии: адаптерный тест
подтверждает реально достижимые interleavings; Engine всё равно проверяет все
допустимые последовательности принятия событий.

## 8. T — время, статус и происхождение

Обозначения: s=source timestamp; v=server timestamp; r=локальное received_at.
Comparable означает тот же достоверно сравнимый clock source без discontinuity.
Одинаковая эпоха Connect сама по себе не доказывает сравнимость разных часов.

| ID | Полученные данные | Ожидаемый результат |
| --- | --- | --- |
| T01 | s есть, v есть | Хранить оба; основная ячейка=s, origin времени=Source |
| T02 | s нет, v есть | Основная ячейка=v, origin=Server; не называть изменением источника |
| T03 | s есть, v нет | Основная ячейка=s; server field отсутствует |
| T04 | s нет, v нет | Основная ячейка пуста; r хранится отдельно |
| T05 | Значение равно предыдущему, метки новые | Метки обновлены; changed_at не обновлён |
| T06 | Значение изменилось | changed_at обновлён локально; s/v не заменяются r |
| T07 | Read(s=10) -> push(s=11), comparable | Показать push, обе метки принадлежат новому результату |
| T08 | push(s=11) -> поздний Read(s=10), comparable, Good | Строка не откатывается; операция Read завершается отдельно |
| T09 | Read(s=11) -> поздний push(s=10), comparable, Good | Строка не откатывается; поток не объявляется Failed только по старому sample |
| T10 | Поздний результат без сравнимых s | Порядок получения с явным origin; свежесть не считается доказанной |
| T11 | s одинаковые, значения разные | Конфликт виден; принять порядок получения без обещания новизны |
| T12 | Новый результат без timestamp после старого с timestamp | Не приклеивать старую метку к новому значению |
| T13 | Bad после Good, source time старее | Новая диагностическая ошибка не скрывается проверкой возраста значения |
| T14 | DA FILETIME | Время DA сохраняется как DA timestamp; не выдумывать UA SourceTimestamp/ServerTimestamp |
| T15 | Изменился только server timestamp на сервере, уведомления нет | Локальное поле само не обновляется; следующее Read/уведомление может обновить |
| T16 | От разных серверов s сравниваются | Глобального порядка не создавать |
| T17 | Часы источника сброшены/discontinuity | Comparable=false; показать неоднозначность, не блокировать все новые данные навечно |

T08/T09 применяются к выбору отображаемого экземпляра Good-данных, не к
подавлению свежих транспортных ошибок/Bad. Исход операции и текущее качество
соединения должны оставаться видимыми даже при сохранении последнего Good sample.

## 9. M — сохранение и миграция

| ID | Вход | Ожидаемый результат |
| --- | --- | --- |
| M01 | Новый UA профиль | Subscription, 1000 ms, read_after_write=true |
| M02 | Новый DA профиль, Device implemented | Poll/Sync/FreshSource, 1000 ms |
| M03 | Старый UA без acquisition, global seconds=5 | Subscription/5000 ms, явная запись профиля |
| M04 | Старый DA без acquisition, global seconds=5 | Poll/Sync/CacheAllowed/5000 ms/read_after_write=false; сохранить опубликованный baseline |
| M05 | seconds * 1000 overflow | Видимая ошибка; не wrap |
| M06 | Явный валидный acquisition при разных globals | Полный round-trip; globals не побеждают |
| M07 | requested interval=1000, revised=2000 | Оба видны; сохраняется requested=1000 |
| M08 | Профиль A сохранён, профиль B активен | B snapshot не меняется |
| M09 | Legacy UA с неподдержанными monitored items | Видимое предупреждение; не скрытый переход Poll |
| M10 | read_after_write отсутствует во впервые введённом явном config | Ошибка неполного профиля с backup/notice; legacy отсутствие всего acquisition отдельно |
| M11 | Новый DA профиль без Device capability | FreshSource disabled; CacheAllowed виден, read_after_write=false либо конфиг отклонён |
| M12 | Unknown enum/null/неверный тип | Для явно присутствующего невалидного acquisition ошибка; совместимость отсутствующего поля обрабатывается отдельно |

## 10. Адаптерные доказательства

| ID | Backend-сценарий | Что проверять настоящим entry point |
| --- | --- | --- |
| A01 | UA Subscription | Создание monitored values; поток; нет периодического Read |
| A02 | UA Poll | Read; не создаются monitored values |
| A03 | UA snapshot/read-back | maxAge согласно цели; Both timestamps; bad/missing per item |
| A04 | DA Poll Sync CacheAllowed | Реальный Sync Read с Cache source; не показывать как Device |
| A05 | DA Poll Sync FreshSource | Реальный поддержанный Device source, не константа OPC_DS_CACHE |
| A06 | DA Poll Async | Точный transaction->RequestId; OnReadComplete освобождает свой слот |
| A07 | DA Subscription | Реально установлен callback и активная группа; отдельные unsolicited data changes |
| A08 | DA callback после Unadvise/disconnect | Lifetime безопасен; stale epoch не меняет новое состояние |
| A09 | DA COM call partial failure/S_FALSE | Item errors и quality сохранены; один успешный COM HRESULT не означает все Good |
| A10 | DA Async выбран, callback недоступен | Unsupported; не фиктивный Async поверх sync loop |
| A11 | UA/DA shared contract | Одна схема demand/correlation; protocol extensions остаются в адаптере |
| A12 | Poll с активным серверным кэшем | Server sampling не объявляется вторым клиентским каналом; постоянные data changes не применяются |

Fake COM подтверждает обработку и владение, но не реальную DCOM-доставку.
Настоящие DA callbacks/Cache/Device проверяются Windows стендом отдельно.
Live UA/DA write-тесты используют разрешённые тестовые теги и восстанавливают
исходное значение guard-механизмом, включая ошибку/паническую раскрутку.

## 11. O — дополнения OPC Foundation и границы библиотек

Основание: [проверка FND](../opc-foundation-gap-review.md),
[карта владельцев](../backend-ownership.md) и
[обязательная native-миграция](../native-backend-migration.md).
Ниже — ожидаемые результаты, не отчёт о реализации. SDK-тест вызывает реальный
locked SDK API/loop либо доказанный adapter boundary; не копию recovery алгоритма.

| ID | Сценарий | Ожидаемый результат |
| --- | --- | --- |
| O01 | requested publish=1000, revised=2000 | Хранить и показывать оба; initial grace использует effective=2000 |
| O02 | Server пересмотрел sampling/queue/keepalive | Сохранить фактические значения отдельно; не подставлять requested |
| O03 | UA STATUS_VALUE, изменился только ServerTimestamp | Отсутствие DataChange допустимо; локальное время тега не тикает само |
| O04 | UA STATUS_VALUE_TIMESTAMP без deadband, SourceTimestamp изменился | Фильтр запрашивает timestamp notification; ServerTimestamp не критерий |
| O05 | Timestamp trigger с deadband / изменение StatusCode | Учесть семантику фильтра; quality change не скрывается deadband |
| O06 | Monitored queue=1, несколько sample до publish | Последний sample допустим; completeness истории не обещается |
| O07 | Queue>1, Overflow bit получен | Бит и gap сохраняются; Good severity не стирает признак потери |
| O08 | Revised lifetime <3*revised keepalive | Ошибка negotiated параметров; checked arithmetic, не wrap |
| O09 | SDK keepalive/Read служебного узла без data change | Liveness обновляется; user-tag value/received_at не меняются |
| O10 | Краткий transport разрыв, SDK восстанавливает Session | Нет второго Connect/recovery владельца; локальная epoch не меняется только из-за SecureChannel |
| O11 | Пропущенное сообщение доступно в retransmission queue | Recovery владелец использует готовый Republish; не второй replay в Engine |
| O12 | Gap не восстановить через Republish | Поддержанный ResendData либо один snapshot Read; history gap остаётся видимым |
| O13 | SDK создал новую remote Session/Subscription внутри локальной epoch | Старые registrations/aliases исключены; новые negotiated/stream facts обновлены |
| O14 | Write -> Good_CompletesAsynchronously | Сохранить raw status; «принято, источник не проверен», а не безусловное выполнение; read-back по конфигу |
| O15 | Write -> Uncertain status | Не превращать в полный success; raw status и неопределённость видны; W таблицы D не устанавливается |
| O16 | Timeout/Cancel Write без доказанного результата | Unknown/может быть выполнено; не автоматический повтор и не «точно не записано» |
| O17 | В одной bulk Write пачке повторён Node+Attribute | Не обещать порядок; для требуемого порядка отдельные последовательные вызовы |
| O18 | Server сообщает MaxNodesPerRead меньше локального chunk | Использовать меньший положительный предел и существующий bounded splitter |
| O19 | OperationLimits отсутствует/сервис TooManyOperations | Bounded fallback chunk/adaptive path, не бесконечное дробление и не нулевой loop |
| O20 | RegisterNodes вернул alias | Канонический NodeId/ItemKey не потерян; alias scoped к remote Session |
| O21 | RegisterNodes вернул исходный неизвестный NodeId | Не считать его valid/readable только по регистрации |
| O22 | CreateMonitoredItems Ok, первый sample Bad_NotReadable | Registration success и плохой data outcome различимы |
| O23 | DA stream capability + Sync Read capability | Subscription + одноразовый Sync Read разрешены; второй постоянный Poll не появляется |
| O24 | DA 1.0 Sync Cache при inactive group/item | Не выдавать invalid quality за Good; не переносить это правило вслепую на все DA методы/версии |
| O25 | DA 1.0 Sync Device при inactive group/item | Не отвергать только из-за Active=false; результат/quality определяет сервер |
| O26 | Общий DA callback получает stream и explicit result | Kind+transaction+epoch/registration маршрутизируют; имя OnDataChange не является достаточным признаком stream |
| O27 | Старый callback/alias после SDK recreation при том же Connect epoch | Adapter registration guard отвергает; одного локального epoch сравнения недостаточно |
| O28 | Готовый SDK/DA facade уже владеет recovery/callback lifecycle | Используется API/config/adapter mapping; не создаётся второй timer/worker/owner |
| O29 | UA IO timeout пометил сессию quarantined | Старой сессии новый Read не отправляется; её demands отменяются; после восстановления новая регистрация получает snapshot |
| O30 | Production UA и DA backend paths | Операции проходят через выбранные native client SDK, не windows-rs ABI-only обёртку |
| O31 | Native SDK владеет protocol recovery/Publish/COM callback | В OPCLook нет второй реализации того же lifecycle |
| O32 | Миграция DA завершена | Старые production IOPC calls/callback/memory owners удалены либо больше не являются runtime path; нет переименованной копии в «adapter» |
| O33 | Native кандидат не поддерживает обязательную capability | Новый кандидат/библиотечное исправление; не эмуляция в App/Engine |
| O34 | DA SDK заменён за общим contract | Те же GUI/CLI и контрактные тесты; нет library-name ветвлений в core |
| O35 | Native SDK возвращает raw status/value/time | Adapter сохраняет информацию; не скрывает ограничения и не превращает async facade в доказанный native stream |

O24/O25 опираются на официальный информационный обзор DA 1.0.
DA 2.05a/3.00 Refresh/SetEnable/Cancel и точные методы требуют полного source/spec
audit до реализации. В fixture O26 задавать operation kind явно; неизвестный
transaction не превращать в unsolicited value ради прохождения теста.
Факт наличия facade API не означает прохождение native Windows/DA interop.

## 12. Что передать из нового чата

После R2.0 агент начинает с выбранных маршрутом G/D/C/M и маркирует
отсутствующие возможности как open. SDK/adapter ownership проверяется до кода.
Не объявлять ignored/заглушенный тест пройденным доказательством.
Затем E/S/T/O и adapter A, после них UI/CLI locale/headless/live.
Для каждого теста: ID строки, файл/имя теста, воспроизведение до реализации,
результат после, ограничения среды. Не менять ожидаемые результаты под старый код.
Если противоречие требует изменения контракта, править R2 и эти таблицы вместе.

Проверка исходного документационного коммита была ограничена данными CSV и документацией.
128+128 — число строк данных, не число пройденных тестов продукта.

### N2.1 offline Softing fact boundary (partial; CI pending)

| ID | Case | Adapter expectation | Evidence / limit |
| --- | --- | --- | --- |
| N2F01 | Callback containing invalid SDK handle before valid item | Patched SDK supplies aligned per-item HRESULT; C ABI copies valid client handle, mode, transaction, master statuses, quality and FILETIME | Win32 fixture calls real SDK callback, C ABI, then Rust `adapt` via serialized test output; exact-head CI pending |
| N2F02 | Failed item HRESULT with Good quality | `Failed`, raw HRESULT/quality retained separately; never a usable value | Rust actual adapter test; not yet an Engine terminal event |
| N2F03 | Unknown VT including ARRAY/BYREF | `UnsupportedType`, raw VT retained; no Empty/Good value | Rust actual adapter test |
| N2F04 | Zero FILETIME, one tick, max | Raw ticks retained; zero maps to no timestamp, one tick converts with 100 ns precision, max is representable by chrono | Rust actual adapter and existing time tests |
| N2F05 | Empty callback with failed master status | Empty batch and envelope retained; previous facts are inaccessible | Win32 fixture calls group adapter directly for this edge; exact-head CI pending |
| N2F06 | 64 accepted; too many entries, null required arrays or invalid identity | 64 facts accepted; explicit overflow/invalid, no partial batch or stale facts; Rust handle/mode errors are typed | Win32 fixture / Rust actual adapter test; SDK callback inputs beyond SDK validation remain untrusted |
| N2F07 | Failed master error with positive item HRESULT and Good quality | `BatchFailed`, both raw statuses and quality retained; no usable value | Rust actual adapter test |

The 64-item limit applies after the SDK has received and prepared the callback; it does not bound SDK-side allocations. Buffer holds only the latest callback and requires external serialization; it is
not a queue or history, is not wired to the production backend, and cannot prove
live delivery, transaction correlation, teardown or two-session isolation.

### N2 SDK initialization owner (Win32 CI pending)

| ID | Условие | Ожидаемый результат | Проверка |
| --- | --- | --- | --- |
| N2L01 | Успешный MTA initialize и terminate на одной thread | COM initialization balanced; исходный SDK не балансирует | direct core probe проверяет CoGetContextToken для baseline и исправленного SDK; Windows CI pending |
| N2L02 | Несовпадение любого из двух SDK source blobs | Patch не изменяет ни один исходный файл | Python test, локально прошёл |
| N2L03 | Две независимые сессии, одна зависла | Вторая продолжает обслуживаться, teardown ограничен | source: per-server SDK worker; live interop pending |

| N2L04 | Fixture connect, add I4, device read, write, remove/re-add, read | SDK ABI returns raw service/item result and original value/quality/time | `core-probe --live`; Windows fixture/CI pending |

| N2L05 | Read/write с вызывающей thread без COM context | SDK выполняет I/O на своём per-server COM worker, caller context не создаётся | `core-probe --live`; Windows fixture/CI pending |

| N2L06 | Remove/re-add и чужая session registration в Rust ABI wrapper | Старый/чужой ключ отклоняется до native call | Реальный wrapper + mock ABI: Linux stand прошёл; `sdk-live` Win32 pending |

| N2L07 | Owner закрывает host при живой session thread | Busy; native host остаётся живым для завершения зависшей операции | Реальный Rust wrapper + mock ABI, Linux stand прошёл; Windows live pending |
