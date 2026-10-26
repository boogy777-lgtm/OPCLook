# Аудит UI/UX: «Сетевые настройки» и OPC UA

**Дата:** 2026-10-26  
**Область:** `src/ui/dialogs/connect.rs`, `src/ui/dialogs/network.rs`,
`src/app/search.rs`, `src/app/mod.rs`, `src/i18n.rs`, `tests/ui_interactions.rs`.

## 1. Итог

Найдены и исправлены ошибки, влиявшие не только на внешний вид, но и на
поведение подключения. Главные:

- выбор «Анонимно» всё равно отправлял сохранённые имя/пароль;
- двойной клик по результату поиска игнорировал security/trust/timeout и
  идентичность;
- открытие вкладки OPC UA запускало поиск само;
- «Отмена» в сетевых настройках закрывала окно, хотя изменения уже сохранены;
- можно было одновременно запустить host-search, LDS/mDNS и сканер;
- пустой порт давал URL `opc.tcp://host:`.

### Обновление 2026-10-26: выбор адаптера без Auto

По новому плану `docs/plan-ua-search-network.md`:

- `AdapterSelection::Auto` и fallback удалены;
- «Сетевые настройки» используют один ComboBox и показывают детали только
  выбранного адаптера;
- пустой выбор означает «адаптер не выбран»: loopback разрешён, удалённые
  операции/подсеть дают локализованную ошибку;
- добавлен счётчик `UA-сессии: N/100`;
- добавлена строка статуса DA route helper и локализованные ошибки UAC/
  конфликта маршрута.

### Обновление 2026-10-26: кэш адаптеров, stale GUID и минимальная форма

По `docs/network-settings-ontology.md`:

- список адаптеров загружается из `settings.cached_adapters` при старте, без
  автоматического опроса Windows; «Обновить» полностью заменяет кэш;
- из формы убраны заголовок-дубль, описание, состояние, полный список адресов,
  шлюз, метрика, «Используется», «Маршрут DA» и сырая ошибка сети — они пишутся
  в журнал;
- dropdown строится по уникальным GUID, выбранная строка вычисляется явно и
  помечается маркером «●»; stale GUID даёт ноль выбранных строк и «Забыть»;
- локальный IP всегда берётся из выбранного адаптера: один адрес — read-only,
  несколько — ComboBox; чужой сохранённый IP не используется;
- открытие вкладки, `current_network_binding`, `discover_da_all`,
  `prepare_da_route_for_host` и старт subnet scan больше не вызывают refresh.

## 2. Исправленные дефекты

| # | Severity | Дефект | Исправление |
|---|---|---|---|
| 1 | P0 | Identity=Anonymous + непустые user/password → worker уходил в UserName | endpoint заполняет user/password только для UserName; Anonymous/X509 не наследуют поля |
| 2 | P0 | Двойной клик по найденному endpoint не передавал policy/mode/trust/timeout/identity | double-click собирает endpoint из текущих настроек диалога |
| 3 | P1 | Открытие вкладки OPC UA автоматически запускало combined search | автозапуск удалён; поиск только по кнопке |
| 4 | P1 | «Отмена» в сетевых настройках при уже применённых изменениях | кнопка «Закрыть» + явная подсказка «Изменения применяются сразу» |
| 5 | P1 | Три поиска можно было запустить одновременно, результаты смешивались | общий `ua_busy` блокирует host/LDS/scan, пока любой из них активен |
| 6 | P1 | Порт пустой/невалидный → `opc.tcp://host:` в поле URL | URL строится с `DEFAULT_UA_PORT`; bare IPv6 берётся в скобки |
| 7 | P1 | Клик по уже активной вкладке OPC UA отменял идущий скан | отмена только при реальном уходе с UA/переключении |
| 8 | P1 | UserName identity с пустым именем молча подключался как Anonymous | локализованная ошибка, диалог остаётся открытым |
| 9 | P1 | Поля user/password показывались всегда, даже для Anonymous/X509 | поля видны только для UserName; для DA сохранены |
| 10 | P1 | После закрытия/повторного открытия могли примениться старые результаты поиска | `close_connect_dialog` очищает pending session 0 и флаги поиска |
| 11 | P2 | Ряд «адрес + порт + три кнопки» не помещался в окно 560 px | LDS/mDNS и сканер вынесены на отдельную строку |
| 12 | P2 | Вкладка «Сетевые настройки» могла неявно обновлять адаптеры при входе | скрытый refresh удалён; `GetAdaptersAddresses` вызывается только кнопкой «Обновить» |
| 13 | P2 | В сетевых настройках не было явного пояснения, что выбор сохраняется | добавлена локализованная подсказка RU/EN/ZH |

## 3. Изменённые файлы

- `src/ui/dialogs/connect.rs` — identity/credentials, endpoint builder,
  URL helper, busy-state, layout, close semantics, tab/scan behavior; сетевой
  tab больше не запускает refresh.
- `src/ui/dialogs/network.rs` — минимальная форма, уникальные строки, явный
  маркер выбора, stale GUID/«Забыть», read-only/ComboBox IP.
- `src/app/search.rs` — `validate_ua_identity`; удалены неявные refresh.
- `src/app/mod.rs` — `close_connect_dialog`, очистка pending session 0,
  загрузка `cached_adapters`, явный refresh/apply, журналирование, удалён
  refresh on entry.
- `src/settings.rs` — `cached_adapters` + tolerant deserialization.
- `src/net.rs` — `CachedAdapter`, `build_adapter_cache`, `reconcile_selection`,
  uniqueness/stale/no-fallback, очистка ошибки после resolve.
- `src/i18n.rs` — `NetworkStaleAdapter`, `NetworkEmptyList`,
  `NetworkRefreshFailed`, `NetworkNoIp`, `NetworkForget` (RU/EN/ZH).
- `tests/ui_interactions.rs` — сетевые UI-тесты (startup cache, stale,
  one/multiple IP, no implicit refresh, cache replacement, sessions).
- `src/ui/dialogs/connect.rs` (unit tests) — URL/port/IPv6.

## 4. Проверки

- `cargo test --lib` — **241 passed**.
- `cargo test --test ui_interactions` — **109 passed**.
- `cargo test --test no_hardcoded_ui_strings` — **2 passed**.
- `cargo test --test gateway_shutdown` — **2 passed**.
- `cargo check --all-targets` — чисто.
- `cargo fmt --check` — чисто.

Добавленные UI-тесты сетевого прохода:

- `network_tab_startup_uses_cached_adapters_without_windows_query`
- `network_tab_stale_guid_has_zero_selection_and_forget_keeps_cache`
- `network_tab_one_ip_read_only_and_multiple_ip_combo`
- `implicit_network_paths_do_not_refresh_inventory`
- `network_refresh_replaces_cache_and_filters_down_adapters`
- `network_tab_refresh_button_queries_windows_inventory`
- `changing_adapter_does_not_rebind_active_sessions`

## 5. Остаточные UX-замечания (не блокирующие)

1. В результатах поиска нет явной метки источника (`Loopback`, `LDS`, `mDNS`,
   `Подсеть`). Пользователь видит URL, но не понимает, откуда он.
2. Кнопка подтверждения большого скана называется «Да»; лучше
   «Сканировать только этот компьютер».
3. Прогресс скана до первого события показывает `0 из 0`.
4. Список адаптеров обновляется только по кнопке «Обновить», без live-
   уведомлений Windows; при смене VPN/кабеля состояние кэша остаётся прежним до
   явного refresh (осознанный компромисс модели).
5. ~~Для DA пока нет строки статуса route helper~~ — ✅ закрыто: статус
   маршрута/helper и ошибки UAC/конфликта пишутся в журнал; в минимальной форме
   «Сетевые настройки» отдельной строки DA route больше нет.
6. Двойной клик по результату сразу подключается (теперь с корректными
   настройками); некоторым пользователям привычнее «двойной клик = выбрать URL».
7. Нет tooltip на кнопках LDS/mDNS и сканера.

Эти пункты не ломают сценарий подключения и могут быть закрыты отдельным
UX-проходом.

## Повторный аудит popup (2026-10-26)

- `Auto/Auto` сохраняет ранее принятое поведение: выбирает `None/None`
  первым. Явный выбор endpoint в списке переносит его policy/mode в настройки
  подключения.
- OPC DA: результаты различаются по паре `(host, ProgID)`; вводимые ранее
  имя и пароль удалены из интерфейса, поскольку DCOM работал от Windows-токена.
- Standalone LDS/mDNS показывает найденные серверы; UA endpoint дедуплицируется
  по `(URL, security policy, security mode)`.
- Проверка UA policy/mode ограничена выбранным URL. Новая конфигурация UA
  существующего URL инициирует переподключение, а отказ не закрывает popup.
- Выбор сетевого адаптера можно сбросить в `Не выбран`.
- Завершение DA discovery с ошибкой сбрасывает индикатор поиска.
- Enter из полей host/port запускает поиск, а не скрытое подключение.

Сохраняемые компромиссы: содержимое кэша адаптеров обновляется только
явной кнопкой, реальные DCOM/UA соединения требуют Windows и сервера для
сквозной проверки.

### Discovery UX and settings transaction

- The UA result list groups endpoints under a server (expanded by default).
- The UA network-search button is context-sensitive: populated host/port
  searches that host; blank host starts subnet discovery with a selected NIC
  or a loopback-only search without one.
- Streaming subnet results become selectable immediately.
- UA security policy/mode form a draft. They are saved only when a connection
  is accepted, so Cancel cannot silently change preferences.
