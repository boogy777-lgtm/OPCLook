# R2.TEST — таблицы истинности и переходов получения данных

Дата: 2026-10-30. База анализа: `main@62ad187c2294cff5df55fe40e82b05f5dda25617`.
Назначение: независимое задание агенту нового чата для написания тестов.
Статус: нормативные ожидаемые результаты; это не отчёт о пройденных тестах.
Связанные документы: [R2](../value-refresh-plan.md),
[конфиг соединения UA/DA](../connection-acquisition.md).

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

W означает подтверждённый Write, а не нажатие кнопки и не Wrote Err.
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
| C06 | DA Subscription + Sync read method | Комбинация отклоняется правилом продукта; режим не меняется |
| C07 | DA Asynchronous выбран, доступен только Sync | Unsupported; никаких тихих подмен |
| C08 | interval_ms = 0 / 3600001 | Ошибка до side effects |
| C09 | interval_ms = 1 / 3600000 | Принят requested interval; adapter/server вправе revise |
| C10 | UA revised interval отсутствует/0/NaN/infinity | Ошибка канала; не нулевой timer loop |
| C11 | UA absolute deadband <0 / NaN / infinity | Ошибка конфигурации |
| C12 | DA percent deadband <0 / >100 / NaN / infinity | Ошибка конфигурации |
| C13 | DA percent deadband = 0 / 100 | Валидный конфиг; только Subscription применяет фильтр |
| C14 | Глобальный interval изменён при активных A/B | Активные snapshots A/B неизменны |
| C15 | Черновик A изменён, затем Cancel | Сохранённый и активный конфиги неизменны |
| C16 | Apply без reconnect | Сохранён профиль следующего соединения; текущий snapshot неизменен |
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
| E09 | Reply R1 Error | R1 Failed; слот свободен; новые pending сохранены |
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
| S02 | Read R1(A) -> W1 Ok(A)/d2 -> R1 Error | d2 остаётся; R1 Failed; R2 разрешён |
| S03 | Read R1(A) -> W1 Ok(A)/d2 -> R1 Timeout | То же, поздний R1 не закрывает R2 |
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
| M10 | read_after_write отсутствует в новом совместимом config | true |
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

## 11. Что передать из нового чата

Агент начинает с G/D/C/M и маркирует отсутствующие возможности как open.
Не объявлять ignored/заглушенный тест пройденным доказательством.
Затем E/S/T и adapter A, после них UI/CLI locale/headless/live.
Для каждого теста: ID строки, файл/имя теста, воспроизведение до реализации,
результат после, ограничения среды. Не менять ожидаемые результаты под старый код.
Если противоречие требует изменения контракта, править R2 и эти таблицы вместе.

Проверка текущего коммита ограничена данными CSV и документацией.
128+128 — число строк данных, не число пройденных тестов продукта.
