# OPCLook: ревью идиом Rust и архитектуры

Коммит `d85c744` (main, 2026-10-27). Код разбит на 10 зон, каждую читал отдельный агент на haiku. Дополнительно прогнан `cargo clippy -W pedantic -W nursery` под x86_64-linux. Windows-код (COM/WinAPI) под Linux собирается как заглушки, поэтому clippy покрыл его частично, а находки по нему проверены только чтением.

Метки: **✔** — оркестратор лично перечитал код по ссылке и подтвердил. **C** — подтверждено агентом по коду. **P** — правдоподобно, нужна проверка.

---

## 0. Сначала баги: найдены попутно, чинить раньше рефакторинга

| # | Где | Что | Метка |
|---|-----|-----|-------|
| B1 | `ui/dialogs/confirm.rs:26,38` + `write.rs:107` + `dialogs/mod.rs:30,35` | Enter в диалоге записи открывает подтверждение. `confirm_dialog` рисуется позже **в том же кадре**, видит тот же `key_pressed(Enter)` и сразу ставит `yes = true`. В итоге запись в ПЛК уходит без показа подтверждения | ✔ |
| B2 | `bff/da_variant.rs:309-314` | Неверные размеры элементов SAFEARRAY: VT_BOOL (11) → 4 байта, а `VARIANT_BOOL` это i16 (2 байта); VT_R4 (4) → 8 байт вместо 4. `copy_nonoverlapping` (стр. 359) пишет за конец буфера (heap overflow). Чтение массивов при этом берёт размер у ОС | ✔ |
| B3 | `app/search.rs:569` | `rest[..10]` режет по байту. Если ввести в поле LDS-хоста `aaaaaaaaaé`, будет паника, а с `panic = "abort"` в release приложение закроется | ✔ |
| B4 | `bff/ua/values.rs:407, 427, 550`; `bff/da_variant.rs:217, 272, 410` | Запись молча портит значение: 3.7 → 3 в Int, 70000 → 32767 в VT_I2 (`clamp`), 300 → 44 в VT_I1 (`as i8`), `u64::MAX` → -1. Для OPC-клиента это опасно | ✔ |
| B5 | `nethelper/src/main.rs:350-357` | `ConnectNamedPipe` выполняется в порождённом потоке, а `GetLastError()` читается в главном. Код ошибки берётся чужой, поэтому `ERROR_PIPE_CONNECTED` превращается в ложный отказ. Код надо брать из самого `windows::core::Error` | ✔ |
| B6 | `nethelper/src/main.rs:412, 423` | `&*(Vec<u8>.as_ptr() as *const TOKEN_USER)` — невыровненная ссылка, это UB. Нужен буфер `Vec<usize>` | ✔ |
| B7 | `bff/ua/trust.rs:57-60` | `let _ = remove_file(...)`, после чего маркер всё равно стирается. Если удаление не удалось, сертификат остаётся доверенным навсегда | ✔ |
| B8 | `da_route.rs:698-712` | Имя пайпа и токен привилегированного helper берутся из xorshift, засеянного временем и pid, то есть их можно предсказать. Нужен `BCryptGenRandom`. Токен к тому же передаётся в командной строке | ✔ |
| B9 | `ui/table.rs:172-174` + `settings.rs:396-399` | DnD колонки передаёт индекс в **видимом** списке, а `move_to_insert` ждёт индекс в **полном** порядке, где есть скрытые колонки. Со скрытой колонкой вставка попадает не туда | ✔ |
| B10 | `app/runtime.rs:14`, `app/mod.rs:1090` | `update_interval_secs.max(1) * 1000` вычисляется на значении из settings.json без clamp при загрузке. Переполнение u64: в debug паника, в release wrap | ✔ |
| B11 | `bff/da_variant.rs:343` | `?` на вложенном элементе VT_VARIANT-массива выходит, не вызвав Unaccess/Unlock/Destroy, и SAFEARRAY утекает | ✔ |
| B12 | `bff/da.rs:490-494` | Если после `AddGroup` падает `cast()`, `?` выходит без `RemoveGroup`, и группа остаётся на сервере | C |
| B13 | `bff/da.rs:835, 1011` | `PWSTR::to_string()` (wcslen) вызывается до проверки на null, а в одном месте проверки нет совсем. Сервер, вернувший S_OK с null, даёт UB | C |
| B14 | `bff/ua/worker.rs:243-246`, `ua/scanner.rs:120-126` | `lock().ok()` при отравленном mutex молча теряет отмену или регистрацию скана. Это нарушение правила `unwrap_or_else(PoisonError::into_inner)` | ✔ |
| B15 | `bff/ua/worker.rs:742` | Когда `load_struct_types` падает по таймауту, ставится `timed_out`, но актор всё равно отправляет первую команду в сеть. Загрузка типов (до 30 с) блокирует и Disconnect | C |
| B16 | `bff/mdns.rs:405, 561` | `ResolveDone` не несёт id запроса, поэтому неудачный resolve не засчитывается и поиск всегда ждёт полный дедлайн 5 с | C |
| B17 | `da_route.rs:601-612` | `self.launch = None` выполняется до успешного connect. Если helper не успел создать пайп за 250 мс, повтора не будет и сессия получит «no helper client» | C |
| B18 | `app/sessions.rs:295-301` | Индекс закрытой вкладки считается до `retain`, поэтому активируется не ближайшая соседка | C (симуляция) |
| B19 | `bff/da_variant.rs:37,126` | `chrono::Duration::days` паникует на враждебном VT_DATE, `ub - lb + 1` переполняет i32 | C |
| B20 | `nethelper/src/main.rs:145-146` | `let _security = security;` ничего не освобождает, SD из `ConvertStringSecurityDescriptor…` утекает. Ключ созданных маршрутов не включает `next_hop`, поэтому один маршрут может остаться в системе | C |

Нарушения AGENTS.md вне тестов: `unreachable!` (`app/sessions.rs:532`), `.unwrap()`/`.expect("checked")` (`ui/dialogs/csv_import.rs:17,27`), `transmute` в `i18n.rs:710` вне COM-файлов, 18 `expect` в `src/bin/probe_*` (они собираются вместе с приложением). `// SAFETY:` нет у ~87 unsafe-строк в net/mdns/nethelper и у ~80 в da_variant.rs.

---

## 1. Сквозные темы: где идиомы Rust дают больше всего

### A. Newtype и enum вместо «голых» u64 и сентинелов
- `SessionId`, `RequestId`, `ItemKey`, `scan_id` сейчас — `type = u64/u32` (`bff/model.rs:4-6`). Сигнатуры вида `connect(request_id, session, …)` и `disconnect(gw, session, request)` компилируются и с перепутанными аргументами. Нужны `struct SessionId(u64)` и т.д. с `#[derive(Copy, Eq, Hash, Ord)]`.
- Значение `request_id == 0` означает «unsolicited» (`model.rs:1429`). Замена: `enum Origin { Reply(RequestId), Unsolicited }`.
- `session == 0` означает «discovery/DA-маршрут» (9 литералов в `app/mod.rs`, `app/search.rs`). Замена: `enum Owner { Discovery, Session(SessionId) }`. Минимальный шаг — одна `const`.
- Плейсхолдеры качества `0xFFFF_FFFE/FFFD` лежат в одном пространстве с кодами сервера (`model.rs:973`). Сервер, приславший такой код, станет «плейсхолдером». Замена: `enum RowQuality { Server(Quality), NoData, Waiting }`.
- `"\u{1}views"` как синтетический node_id встречается в двух файлах. Нужны константа и метод `is_views_root()`.
- Магические VT-номера в трёх местах (`app/table.rs:497`, `state.rs:247`, `bff/vt.rs`). Замена: `struct VarType(u16)` с ассоциированными константами.
- `Option<NonZeroU32>` для индексов интерфейсов вместо `Option<u32>` + `.filter(|i| *i != 0)` (`bff/net.rs:35-57`).

### B. Сделать невалидные состояния непредставимыми: enum вместо bool и Option-комбинаций
| Сейчас | Предлагается |
|---|---|
| `ConnectDialog { protocol, network_tab: bool }` | `enum ConnectPane { Da, Ua, Network }` |
| `ua_identity_index: usize` (0/1/2, `_ =>` = «сертификат») | `enum UaIdentity { Anonymous, UserName, Certificate }` |
| `ua_searching, ua_network_searching, ua_subnet_searching, ua_auto_pending, ua_scan_id…` | `enum UaSearch { Idle, Single, Auto{pending}, Subnet{scan_id, confirm} }`; `ua_subnet_searching` просто удалить, оно ≡ `ua_scan_id.is_some()` |
| `ConfirmDialog { open: bool, action: Option<…> }` | `confirm: Option<ConfirmRequest>` + `take()` в `confirm_yes` |
| `da_servers` + `da_server_hosts` (параллельные Vec, дедуп через `zip`) | `Vec<DaHit { server, host }>` |
| `UaScanFinished { cancelled: bool, error: Option<String> }` | `end: ScanEnd { Completed, Cancelled, Failed(..) }` |
| `CsvImportJob { open, cancelled, done }` (`open`/`done` никто не читает) | `state: CsvImportState` |
| `SessionView` с UA-полями у DA-сессий | `kind: SessionKind { Da, Ua(UaSession) }`. Тогда проверки `protocol == OpcUa && …` исчезают |
| `Option<UaSecurityPolicy>`, где `None` ≡ `Auto` (dedup и reconnect сравнивают по-разному: `sessions.rs:34` vs `:64`) | поле без Option + `skip_serializing_if = "is_auto"` |
| `Browse.filter: Option<u8>` (1, 2, остальное = «все») | `enum NodeClassFilter { All, Objects, Variables }` |
| `DaRouteManager { status, helper, launch }` | `enum HelperState { Idle, Launching, Launched{..}, Connected, Failed }`. Заодно чинит B17 |
| `Journal Entry { text, l10n: Option<..> }` | `enum Message { Plain(String), Localized{ key, args } }` |
| nethelper `PipeMessage { op: String, target: Option<..> … }` | `#[serde(tag = "op")] enum Request { Hello{..}, Add(RouteRequest), … }` |

### C. «Parse, don't validate»: разбирать вход один раз, на границе
- `settings.column_order: Option<Vec<String>>` разбирается через `serde_json::from_str` + `format!` **каждый кадр** (`settings.rs:363`, вызов из `ui/table.rs:50`). Замена: `Vec<ColumnId>` с `deserialize_with` для старого формата. Это заодно упрощает исправление B9.
- `Settings.lang: String` → `Lang` (lenient-десериализатор); `preferred_adapter: Option<String>` → `Option<AdapterId>` (serde уже есть); `selected_local_ip` → `Option<IpAddr>`.
- `UaEndpointInfo { security_policy: String, security_mode: String }` собирается из `format!("{:?}")` и потом разбирается обратно (`discovery.rs:214`, `security.rs:38`). Замена: `Option<Advertised<UaSecurityPolicy>>`, где `enum Advertised<T> { Known(T), Unknown(String) }`.
- `Options::clamped()` вызывать в `Settings::load`, а не только в диалоге (закрывает B10). Вместе с этим `#[serde(default)]` на всей `Settings`, иначе файл без одного поля целиком уходит в `.bad`.
- `opc.tcp://` URL разбирается пятью разными способами (`discovery.rs`, `scanner.rs`). Нужен один `struct UaUrl` с `FromStr`, сравнение без аллокаций.
- Константы времени → `Duration`; инварианты (`lifetime >= 3*keepalive`, `refresh < lifetime`) → `const _: () = assert!(…)`.

### D. RAII для COM/WinAPI: unsafe собрать в одном месте
- Модуль `bff/com/` с типами `CoString` (PWSTR + CoTaskMemFree в Drop, null-check в конструкторе), `ComArray<T>`, `OwnedVariant`, `SafeArrayBuilder` (Unaccess/Unlock/Destroy в Drop, `into_variant` + `forget`), `ServerGroup` (RemoveGroup в Drop, пока не `disarm`). Это закрывает B2, B11, B12, B13 и 17 ручных `CoTaskMemFree` в `da.rs`.
- `winutil`: `OwnedHandle`, `LocalAllocBox`, общий `wide_to_string` (сейчас одинаковые копии лежат в `net.rs:1280` и `mdns.rs:349`), загрузчик символов `symbol::<F>(module, c"Name")` вместо четырёх `transmute`. `LoadLibraryExW(..., LOAD_LIBRARY_SEARCH_SYSTEM32)`.
- Обход WinAPI-массивов через `slice::from_raw_parts` + `zip` вместо `.add(index)` (`mdns.rs:430`).
- `#![deny(clippy::undocumented_unsafe_blocks)]` в COM/WinAPI-модулях, чтобы компилятор сам проверял правило из AGENTS.md про `// SAFETY:`.
- `Tr::all()`: вместо `transmute` + `COUNT = последний вариант + 1` сделать макрос, генерирующий `enum Tr` и `const ALL: &[Tr]`. Сейчас вариант, добавленный после `DaRouteHelperIdle`, молча выпадет из `all()` и из теста.

### E. Типизированные ошибки (thiserror уже в зависимостях)
- `Gateway::send -> Result<(), String>` с английским текстом, который попадает прямо в журнал UI. Замена: `enum SendError { GatewayClosed, WorkerClosed }` + i18n.
- UA: ожидание с таймаутом возвращает `(Option<T>, bool)`, отсюда шесть `expect("…unless it timed out")` (`worker.rs:1151`, `io.rs:62…`). Одна функция `bounded(fut, dur) -> Result<T, Timeout>` убирает все `expect`.
- `BackendMsg::Raw(format!("browse: {e}"))` — английские префиксы в обход i18n (`io.rs:68,144,175,255,443`).
- `HelperClient` возвращает `Result<_, String>`, а `HelperUnavailable(detail)` при показе теряет `detail`. Замена: `enum HelperIpcError { Io(#[from]), Protocol(#[from] serde_json::Error), Rejected, Closed }`.
- `parse_lds_host -> Result<_, ()>`, `next_hex -> Result<Vec<u8>, ()>` → нормальные error-типы или `Option`.
- `WorkerHandle::shutdown` проглатывает панику воркера (`let _ = join.join()`). Нужен `ShutdownOutcome { Joined, Panicked, TimedOut }` и запись в журнал.
- CSV-импорт молча пропускает битые строки, номеров строк нет. Нужен `Vec<CsvIssue { line, kind }>` → журнал.

### F. Immediate mode: убрать аллокации на каждый кадр
Главный приём: **кэш производных данных с ревизиями в App** (`rows_rev`, `tree_rev`, `expanded_rev`, `settings_rev`), а UI только читает. Сейчас каждый кадр пересчитываются:
- `visible_row_positions()` + `table_lines()` по всем строкам (`ui/table.rs:65`), `visible_tree_indices()` по всему дереву;
- `Value::children()` собирает `Vec<(String, &Value)>` **по всему массиву** до проверки бюджета (массив из 100k элементов даёт 100k `format!` на кадр). Замена: `child_count()` + `child(i) -> (Cow<str>, &Value)` или `ChildName<'_>: Display`;
- для каждой строки таблицы `value.to_string()`, `timestamp_text()` (chrono + format), `write_type_text()`. Последнее → `Cow<'_, str>`, потому что `vt_name` уже `&'static str`;
- журнал: все ≤5000 записей рендерятся без `show_rows`, у каждой `render()` → `String`; `Vec::drain(0..1)` → `VecDeque::pop_front`; при `add_l` текст формируется по-русски впустую;
- `Quality::from_da` аллоцирует строку на каждое значение. Замена: `text: Cow<'static, str>`;
- `fmt(lang, …, &[&x.to_string()])`: `fmt` и так принимает `&dyn Display`, `to_string` не нужен (и в `i18n::fmt` `write!` вместо `a.to_string()`);
- DnD-payload с `vec![item.id]` на каждую строку дерева. Строить его только при `drag_started()`;
- `on_hover_text(format!(…))` → `on_hover_ui(|ui| …)`, строится только при наведении;
- «О программе» читает и разбирает сертификат с диска каждый кадр (`about.rs:66`);
- дерево UA-результатов в диалоге подключения пересобирается каждый кадр (`ua_tree.rs:234`).

### G. Итераторы и коллекции
- O(N·K): `keys.retain(|k| rows.iter().any(..))`, `selected_rows.retain(..)`. Замена: один `HashSet<ItemKey>` (`app/table.rs:137,149`, `sessions.rs:286`).
- ~46 повторов `self.sessions.iter_mut().find(|s| s.id == id)` → `session()` / `session_mut()`; `.map(|s| s.protocol() == OpcUa).unwrap_or(false)` → `self.session_protocol(id) == Some(OpcUa)`.
- `tick()`: `for index in 0..len { self.sessions[index]… }` ×15 → `for view in &mut self.sessions`.
- `RowIndex` инвалидируется сравнением длины: «удалить k, добавить k» оставляет индекс устаревшим (P). `rows` сделать приватным, `push_row` начинать с `ensure_row_index()`.
- `SubnetHosts` (самописный итератор) → `(first..=last).map(..)`; маска через `checked_shl` без ветки `prefix == 0`.
- `next_hex`: Vec на каждую группу → `[u8; N]` + `chunks_exact(2)`.
- Монтирование тегов UA: квадратичный поиск узла по ключу (`session.rs:867,902`); NodeId разбирается из строки на каждый опрос (`io.rs:347`). Хранить разобранный `NodeId` в items.

### H. Крупная структура: что резать
- **`App`** (40+ pub-полей, 6 обязанностей) → выделять по одной структуре: `PendingRequests`, `DaRouteHolds`, `WriteBatch`, `network`. `App` остаётся координатором.
- **`on_event`** (620 строк, 25 веток) → `on_discovery / on_session / on_data / on_operation_error`. Переход «связь потеряна» сейчас повторён 4 раза с **разным** набором сбрасываемых полей. Нужна одна функция `mark_link_lost`.
- **`connect_dialog`** (800 строк), **`table_panel`** (560 строк, 4 уровня замыканий), `navigation` → подмодули. Общий каркас диалогов повторён 9 раз; можно сделать `fn modal(ctx, title, |ui| -> Option<R>) -> Option<R>`.
- **`bff/model.rs`** (2000 строк, `BackendMsg` ≈161 вариант) → `model/{command,event,endpoint,security,value,quality}.rs` с `pub use`. `BackendMsg` можно разбить на `Da(DaMsg) / Ua(UaMsg) / Discovery(..)`, i18n-match тогда тоже делится по доменам.
- **`src/net.rs`** (2036) → `net/{id,adapter,cache,subnet,lease,windows}.rs`; `NetworkBinding` перенести из `bff::net` в доменный модуль, чтобы убрать инверсию зависимостей.
- `UaSession` (5 областей) и циклическая зависимость `io ↔ session ↔ worker`. Таблица операций дублирована (`CommandKind::operation()` vs `reply_no_connection`), и копии уже расходятся.
- DA: поток обслуживает одну сессию, но хранит `HashMap<SessionId, DaSession>`, отсюда ~70 строк недостижимых веток. Замена: `Option<DaSession>`.
- Списки «молчаливых/неподдержанных» команд повторены в 4 местах (`da_router.rs`, `da.rs`, `da_unsupported.rs`). Брать их из одного `route_of()`.
- `Settings` — тоже god-object (вид, UA, сеть, кэш, колонки, плюс `Arc<Mutex<notices>>` внутри клонируемой модели). Предлагается `ViewPrefs/UaPrefs/NetworkPrefs` и `load() -> Loaded { settings, notices }`.
- Блокирующие операции в пути подключения: `ensure_helper` (до 2 с `sleep`), DNS-резолв. Вероятно, они выполняются в UI-потоке (P). Перенести в поток сессии с событием.

**Чего делать не надо:** трейт `Protocol` для DA/UA в слое app. Бэкенд уже отделён через `Command/Event`, а оставшиеся проверки протокола закрывает sum-тип `SessionKind`. Агенты зон 6 и 1 пришли к этому независимо.

---

## 2. Быстрые механические правки по clippy (pedantic, x86_64)
| Линт | Кол-во | Что даёт |
|---|---|---|
| `collapsible_if` → let-chains (edition 2024) | 39 | `if let Some(x) = a && cond { … }` |
| `map().unwrap_or(false)` → `is_some_and` | 10 | `app/events.rs:287, 706` |
| `map().unwrap_or(_else)` / `option_if_let_else` → `map_or(_else)` | ~35 | `app/tree.rs:370`, `security.rs:72` |
| `single_match` → `if let` | 10 | |
| `assigning_clones` → `clone_from` | 24 | переиспользует буфер String/Vec |
| `redundant_clone` | 6 | `app/table.rs:212, 302` |
| `needless_pass_by_value` / `needless_pass_by_ref_mut` | 9 / 14 | `&[u64]` вместо `Vec<u64>`, `&` вместо `&mut` |
| `large_enum_variant` | 2 | `ua/worker.rs:664` (≥208 байт) → `Box` |
| `significant_drop_tightening` | 2 | `ua/trust.rs:45, 89`: lock держится дольше нужного |
| `derivable_impls` | 4 | `app/dialogs.rs:139, 162` |
| `cast_*` | 29 | `sessions.rs:343` usize↔isize, `columns.rs:167`, `values.rs:407/550` (= B4) |
| `match_same_arms` | 65 | объединить `A \| B =>` |

Шум, не трогать: `missing_const_for_fn` (75), `use_self` (146), `doc_markdown` (69), `suboptimal_flops` (28). Сырой вывод: `clippy_raw.txt`.

---

## 3. Предлагаемый порядок (по коммиту на пункт, по правилам AGENTS.md)
1. **Безопасность и корректность:** B1–B8, B10, B14 (каждый с регрессионным тестом).
2. **RAII-слой COM/WinAPI** (тема D) + `deny(undocumented_unsafe_blocks)`. Закрывает B2, B11–B13, B20 системно.
3. **Типы на границах** (тема C): `column_order`, `lang`, адаптер/IP, `Options::clamped`, `#[serde(default)]`.
4. **Newtype ID + Origin/Owner** (тема A). Механика, компилятор найдёт все места.
5. **Enum-состояния диалогов и сессий** (тема B): `ConfirmRequest`, `UaIdentity`, `ConnectPane`, `UaSearch`, `SessionKind`.
6. **Кэш производных данных + Cow** (тема F): сначала замерить FPS и аллокации на 10k тегов.
7. **Разрезание модулей** (тема H) — после 4–5, когда типы устоятся.
8. Clippy quick wins — фоном, отдельным коммитом на группу линтов.

## Ограничения
- Windows-часть не компилировалась. Всё по DA/COM/WinAPI проверено чтением.
- Цифры производительности оценены по коду и не замерены.
- Пункты с меткой P требуют проверки перед правкой. Номера строк даны для `d85c744`.
