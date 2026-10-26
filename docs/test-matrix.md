# Матрица покрытия «функция → тест»

Таблица ведётся постоянно: при добавлении фичи или теста обновите статус.
Обозначения:

- ✅ — покрыто (имя теста / файл)
- ⏳ — запланировано (агент/фаза)
- ⛔ — заблокировано (причина; часто «код живёт во `src/ui.rs`/`src/app.rs`, которые вне зоны изменений»)

Оговорка о процентах: без инструмента замеров покрытия (например `cargo llvm-cov`)
проценты не указываем. Статусы ниже отражают фактическое наличие тестов по одной функции.

Юнит-тесты: `src/{settings,columns,journal}.rs` (модули `#[cfg(test)]`)
и существующие тесты в `src/bff/model.rs`.

---

## Сеть, привязка UA и DA route steering (2026-10-26)

| Функция | Статус | Тест / файл |
|---|---|---|
| `AdapterSelection::{None,Adapter}`, нет Auto/fallback | ✅ | `none_selection_is_an_explicit_error_without_fallback`, `missing_or_down_adapter_is_an_error_not_a_fallback` — `src/net.rs`; `connect_dialog_network_tab_renders_single_adapter_combo` — `tests/ui_interactions.rs` |
| `cached_adapters` round-trip / missing field / deleted config | ✅ | `cached_adapters_roundtrip_and_load`, `missing_cached_adapters_loads_as_empty`, `deleted_config_loads_with_empty_adapter_cache`, `malformed_cached_adapter_record_is_dropped_and_reported` — `src/settings.rs` |
| Startup читает кэш и не опрашивает Windows | ✅ | `adapter_cache_loads_without_windows_and_keeps_ipv6_scope` — `src/net.rs`; `network_tab_startup_uses_cached_adapters_without_windows_query` — `tests/ui_interactions.rs` |
| «Обновить» полностью заменяет кэш; down/no-address/loopback/duplicate отфильтрованы | ✅ | `build_adapter_cache_replaces_and_filters_inventory`, `load_cached_adapters_rejects_duplicates_unspecified_down_and_no_address`, `windows_inventory_is_non_panicking_without_a_selection` — `src/net.rs`; `network_tab_refresh_button_queries_windows_inventory`, `network_refresh_replaces_cache_and_filters_down_adapters` — `tests/ui_interactions.rs` |
| Ровно одна выбранная строка; stale GUID — ноль; дубли GUID | ✅ | `exactly_one_row_is_selected_even_with_duplicate_guids`, `stale_guid_has_zero_selected_rows_and_no_fallback` — `src/net.rs`; `network_tab_stale_guid_has_zero_selection_and_forget_keeps_cache` — `tests/ui_interactions.rs` |
| IP принадлежит выбранному адаптеру; успешный resolve очищает ошибку | ✅ | `ip_must_belong_to_the_selected_adapter`, `successful_resolve_clears_a_previous_network_error`, `multiple_addresses_select_ip_and_matching_subnet` — `src/net.rs` |
| Один IP read-only; несколько IP — ComboBox | ✅ | `network_tab_one_ip_read_only_and_multiple_ip_combo` — `tests/ui_interactions.rs` |
| Нет неявного refresh (вкладка, binding, DA, subnet) | ✅ | `implicit_network_paths_do_not_refresh_inventory` — `tests/ui_interactions.rs` |
| Смена адаптера не перепривязывает активные сессии | ✅ | `changing_adapter_does_not_rebind_active_sessions` — `tests/ui_interactions.rs` |
| Миграция старых settings без GUID → «не выбран» | ✅ | `old_settings_missing_network_fields_load_as_unselected`, `invalid_persisted_network_values_degrade_to_unselected` — `src/settings.rs` |
| CIDR `/23`, `/24`, `/31`, `/32`, IPv6, большие диапазоны | ✅ | `subnet_*` — `src/net.rs` |
| Все UA-пути через bound connector/lease | ✅ | `loopback_connect_ignores_a_physical_binding`, binding tests — `src/bff/net.rs`; scanner/session/discovery используют `BoundConnectorBuilder`/`BoundConnectionSource` |
| Loopback без адаптера; remote без адаптера блокируется | ✅ | `remote_ua_search_without_adapter_is_blocked`, `remote_ua_connect_without_adapter_is_blocked_but_loopback_is_allowed` — `tests/ui_interactions.rs` |
| Сканер: loopback первым, IPv6, union endpoints по `ApplicationUri` | ✅ | `scan_targets_*`, `scanner_unions_endpoints_for_the_same_application_uri`, `scan_server_keys_*` — `src/bff/ua/tests.rs` |
| Лимит 100 UA-сессий: 99→100, 101 отклонён | ✅ | `ua_session_cap_allows_100_and_rejects_the_101st` — `tests/ui_interactions.rs` |
| Реконнект на лимите разрешён; DA не считается | ✅ | `ua_session_cap_allows_reconnect_at_the_limit`, `ua_session_cap_does_not_block_da_sessions` — `tests/ui_interactions.rs` |
| Скрытые/отключённые считаются; удаление освобождает слот | ✅ | `ua_session_cap_counts_hidden_disconnected_and_frees_on_remove` — `tests/ui_interactions.rs` |
| Счётчик `UA-сессии: N/100` | ✅ | `connect_dialog_ua_security_controls_render` — `tests/ui_interactions.rs` |
| Route planning: on-link, gateway, IPv4/IPv6, reject default/multicast/broadcast | ✅ | `on_link_*`, `off_link_*`, `ipv6_*`, `loopback_multicast_and_broadcast_targets_are_rejected` — `src/da_route.rs` |
| Route conflict и refcount `(IP, adapter)` | ✅ | `route_decision_*`, `refcount_releases_only_after_the_last_owner` — `src/da_route.rs` |
| Local/loopback DA не требует адаптера/helper | ✅ | `local_targets_do_not_require_an_adapter` — `src/da_route.rs` |
| Helper: named pipe, ACL, token, PID/SID, audit, cleanup | ✅ (код) | `nethelper/src/main.rs`; ручная UAC-проверка — см. `docs/network-binding.md` |
| Elevated add/verify/remove helper | ⛔ | требует UAC/администратора и реального интерфейса; команды в `docs/network-binding.md` |
| Live DA через второй хост/VM | ⛔ | нет Kepware/второго DA-хоста в среде; не считается выполненным |

### Живые прогоны 2026-10-26

- `cargo test --test ua_search -- --ignored` — **2 passed**: CODESYS
  `127.0.0.1:4840`, найдены оба endpoint (`NTBK700582:4840`, `127.0.0.1:4840`).
- `cargo test --test ua_smoke -- --ignored` — **2 failed**: `таймаут активации
  OPC UA сессии`; сервер отвечает на `GetEndpoints`, но не активирует сессию в
  текущем состоянии среды. Не считается зелёным.
- Live DA/helper — ⛔, нет второго DA-хоста/Kepware.
- `cargo test --tests` (все интеграционные бинарники сразу) — ⛔ по среде:
  не хватает места на диске и `LNK1140` (PDB 32-bit) при линковке больших
  тестовых бинарников; `cargo check --all-targets` проходит.

---

## Настройки (`settings.rs`)

| Функция | Статус | Тест / файл |
|---|---|---|
| Значения по умолчанию (EleSy 2.1) | ✅ | `options_defaults_match_elesy`, `settings_defaults_field_values` — `src/settings.rs` |
| update_interval = 1 c | ✅ | `options_defaults_match_elesy` — `src/settings.rs` |
| operation_timeout = 60 c | ✅ | `options_defaults_match_elesy` — `src/settings.rs` |
| test_interval = 60 c | ✅ | `options_defaults_match_elesy` — `src/settings.rs` |
| Флаги show_log / show_script | ✅ | `options_defaults_match_elesy` — `src/settings.rs` |
| Serde roundtrip JSON (все поля) | ✅ | `serde_roundtrip_json_preserves_all_fields`, `settings_defaults_field_values` — `src/settings.rs` |
| Пароль не попадает в JSON (`#[serde(skip)]`) | ✅ | `password_is_skipped_in_json` — `src/settings.rs` |
| Fallback при отсутствии файла | ✅ | `load_missing_file_falls_back_to_defaults` — `src/settings.rs` |
| Сохранение → загрузка roundtrip | ✅ | `save_then_load_roundtrips` — `src/settings.rs` |
| Недавние точки подключения (dedupe/перестановка/лимит 12) | ✅ | `remember_dedups_reorders_and_truncates`, `remember_keeps_only_twelve_endpoints` — `src/settings.rs` |
| last_da_host / last_da_server / last_ua_url при `remember`; **`last_ua_url` пуст по умолчанию (без пресетов)** | ✅ | `remember_records_da_host_and_server` — `src/settings.rs`; пустой дефолт — `settings_defaults_field_values`, `load_missing_file_falls_back_to_defaults` — `src/settings.rs` |
| Упорядочивание колонок из контекстного меню (`column_order` + `columns.order`) | ✅ | `serde_roundtrip_json_preserves_all_fields`, `persist_column_order_updates_resolved_and_columns`, `resolved_order_falls_back_when_column_order_is_none`, `resolved_order_ignores_unknown_entries_and_fills_missing` — `src/settings.rs` |
| Клэмп выпадающих интервалов (update 1..=60, op/test 1..=1800, deadband) | ✅ | `clamp_options_enforces_interval_and_timeout_ranges` — `src/app.rs`; границы в `src/constants.rs` (`constants::tests`) |

## Колонки (`columns.rs`)

| Функция | Статус | Тест / файл |
|---|---|---|
| Порядок колонок (ровно 9, без дубликатов) | ✅ | `column_order_is_exact` — `src/columns.rs` |
| Заголовки колонок («Полное имя», «Записываемый тип», «Тип», «Значение», «Качество», «Время изменения», «OРС сервер», «Описание») — канонич. заголовки CSV (Delphi) через `csv_title()` | ✅ | `column_titles_match_elesy_constants` — `src/columns.rs` |
| Пустой заголовок колонки «Состояние» | ✅ | `column_titles_match_elesy_constants` — `src/columns.rs` |
| Подсказки (hint) колонок (Delphi-канонич. `csv_hint()`) | ✅ | `column_hints_are_present` — `src/columns.rs` |
| Ширины колонок > 0 | ✅ | `column_widths_are_positive` — `src/columns.rs` |
| Набор по умолчанию видим (все) | ✅ | `columnset_default_shows_every_column` — `src/columns.rs` |
| Скрытие/показ (`set_visible`) | ✅ | `set_visible_hides_and_reshows` — `src/columns.rs` |
| Показ несуществующей колонки добавляет её в `order` | ✅ | `set_visible_appends_unknown_column_to_order` — `src/columns.rs` |
| Идемпотентность set_visible | ✅ | `set_visible_is_idempotent` — `src/columns.rs` |
| move_up / move_down (в т.ч. границы) | ✅ | `move_up_reorders`, `move_down_reorders`, `move_on_missing_column_is_noop` — `src/columns.rs` |
| reset колонок | ✅ | `reset_restores_defaults` — `src/columns.rs` |
| CSV-экранирование (кавычки/запятая/перевод строки) | ⛔ | `csv_escape` — приватная функция в `src/ui.rs:1539`, вне зоны изменений |
| Форматирование значения/качества/метки времени для CSV | ⛔ | Хелперы в `src/ui.rs`/`src/app.rs` (заголовок+строки CSV). Значение форматируется через `Display` — покрыто в `src/bff/model.rs` (`value_format`, `datetime_format`) |

## Журнал (`journal.rs`)

| Функция | Статус | Тест / файл |
|---|---|---|
| Запись уровней Info/Warn/Error | ✅ | `add_records_level_and_text_in_order`, `distinct_level_helpers_map_correctly` — `src/journal.rs` |
| Порядок записей (новейшая в конце, время монотонно) | ✅ | `entries_are_sorted_newest_last` — `src/journal.rs` |
| Кап и сброс старых записей (по умолчанию 5000) | ✅ | `append_cap_trims_oldest_when_over_capacity`, `load_over_capacity_trims_to_max` — `src/journal.rs` |
| Ниже капа — без обрезки | ✅ | `below_capacity_does_not_trim` — `src/journal.rs` |
| clear | ✅ | `clear_empties_journal` — `src/journal.rs` |
| Формат строки сохранения `… [LEVEL] текст` | ✅ | `save_writes_expected_line_structure` — `src/journal.rs` |
| Загрузка (пропуск пустых строк, кап) | ✅ | `load_populates_entries_and_caps` — `src/journal.rs` |
| Сохранение → загрузка roundtrip | ✅ | `save_then_load_roundtrip_preserves_text` — `src/journal.rs` |
| Локализация записей (`Entry.l10n`, `render(lang)`, `save_localized`) | ✅ | `Entry` хранит `l10n: Option<(Tr, Vec<String>)>`; `save` по-прежнему рендерит ru для файлов/тестов; экспорт из UI — `save_localized(path, app.lang())` — `src/journal.rs` / `src/ui.rs` |
| Фильтрация по уровню | ⛔ | В `journal.rs` нет логики фильтрации (хранит все уровни); отображение фильтруется в `src/ui.rs`, вне зоны изменений |

## Модель BFF (`bff/model.rs`, герметичные тесты)

| Функция | Статус | Тест / файл |
|---|---|---|
| Качество UA: маппинг severity | ✅ | `quality_from_ua_severity_mapping` — `src/bff/model.rs` |
| `Value::as_f64` | ✅ | `value_as_f64`, `new_value_variants_as_f64` — `src/bff/model.rs` |
| Формат `Value` (числа, bool, строка, NaN/inf, буфер, массив) | ✅ | `value_format`, `datetime_format`, `bytes_format_is_hex_with_truncation`, `array_format_truncates_after_16`, `node_id_format`, `qualified_name_format`, `localized_text_format` — `src/bff/model.rs`; интеграционный `tests/value_types.rs` |
| Полные UA типы: `Value::NodeId`, `QualifiedName`, `LocalizedText` (Display/as_f64/parse) | ✅ | `tests/value_types.rs` (6 тестов), `src/bff/model.rs` |
| Полные UA типы: ByteString → hex (≥16 байт + «…»), массивы → «[…] …N эл.» (≥16 элементов) | ✅ | `bytes_value_is_hex_with_truncation_marker`, `array_value_truncates_after_sixteen_elements` — `tests/value_types.rs` |
| Да/UA: конвертация `Value` → OPC UA `Variant` по реальному типу узла (все скалярные типы, массивы, диапазонные ошибки) | ✅ | `typed_*`, `typed_integer_out_of_range`, `typed_array_*`, `unknown_type_infers_from_value` — `src/bff/ua.rs` |
| UA write: NodeId/QualifiedName/LocalizedText (parse `ns=;s=` / `i=` форм), отказ от структурированных (ExtensionObject/Guid/XmlElement) с понятной ошибкой | ✅ | `typed_write_supports_new_scalar_types`, `infer_write_supports_new_value_types` — `src/bff/ua.rs` |
| UA массивы: чтение N-мерных (nested `Value::Array`), запись одномерных; отказ от многомерной записи | ✅ | `variant_to_value_flattens_multi_dim_array`, `nested_array_write_is_rejected` — `src/bff/ua.rs` |
| UA read: StatusCode → `Other("0x…")`, ExtensionObject → `structured:<type>` | ✅ | `variant_to_value_maps_new_types` — `src/bff/ua.rs`; live `ua_type_support_reads_without_panic_and_handles_unknown_types` — `tests/ua_types.rs` |
| Да/UA: имена типов (`ua_type_name`) | ✅ | `ua_type_name_mapping` — `src/bff/ua.rs` |
| Да: roundtrip `Value` → COM VARIANT → `Value` для VT_BOOL/I1/I2/I4/I8/UI1/UI2/UI4/UI8/R4/R8/CY/BSTR/DATE и массивов | ✅ | `roundtrip_vt_*`, `roundtrip_array_of_*`, `roundtrip_variant_array_without_hint_infers_element_type`, `roundtrip_scalar_to_array_of_one_via_array_bit`, `i1_negative_roundtrip_preserves_sign`, `unsupported_vt_returns_error` — `src/bff/da_variant.rs` |
| Browse с фильтром по классу узла (UA; DA — без изменений) | ✅ | live `ua_variable_filter_browse_returns_variables_only` — `tests/ua_types.rs` (`--ignored`) |
| Browse Views (синтетический узел «Представления», выбор View для browse) | ✅ | live `ua_browse_views_lists_views_and_browsing_in_them_does_not_error` — `tests/ua_types.rs` (`--ignored`); CODESYS: представлений нет → graceful |
| ResolveBrowsePath / «Добавить по имени» (путь → NodeId; первый target) | ✅ | live `ua_resolve_browse_path_resolves_objects` — `tests/ua_types.rs` (`--ignored`); `0:Objects → i=85` |
| Типизированные операции: `Operation` enum, `as_str`, `Display`, `Operation::ALL` (ASCII/уникальность), маппинг `CommandKind::operation()` | ✅ | `operation_codes_are_ascii_and_cover_the_emitters`, `command_kind_maps_to_its_operation` — `src/bff/model.rs` |
| Запрет сырых строк операции в BFF/app (`operation == "`, `operation: "`, `operation.to_string()`, `op::`) | ✅ | `no_raw_operation_strings_or_dispatch` — `tests/operation_guard.rs` |
| Маршрутизация команд и ошибки отправки: `Gateway::send -> Result`, журнал + состояние сессий, `shutdown()` с ограниченным join | ✅ | `send_after_shutdown_returns_error`, `shutdown_joins_workers_without_hanging_and_is_idempotent` — `tests/gateway_shutdown.rs`; `on_exit_shuts_down_the_gateway` — `tests/ui_interactions.rs` |

---

## Сквозные функции приложения

| Функция | Статус | Тест / файл |
|---|---|---|
| Меню и диалоги | ⛔ | Логика в `src/ui.rs`, вне зоны изменений |
| Подключение/отключение OPC UA | ✅ (интеграционный) | `server_browse_add_and_read`, `server_invalid_node_read_does_not_panic` — `tests/ua_smoke.rs` (`--ignored`, требует UA-сервера) |
| Дискавери UA-endpoint'ов перед подключением (GetEndpoints) и выбор политики/режима | ✅ (live `--ignored`) | `server_search_enumerates_endpoints` — `tests/ua_security.rs`; типизированный `resolve_security()` — `src/bff/ua.rs` |
| UA Auto-выбор политики/режима (None → Basic256Sha256 Sign → SignAndEncrypt → первый endpoint) | ✅ | `auto_prefers_none_then_basic256_sha256`, `auto_sign_mode_matches_over_encrypt`, `concrete_mode_with_auto_policy_picks_first_matching`, `explicit_policy_and_mode_are_honored` — `src/bff/ua.rs` |
| Типизированные `UaSecurityPolicy`/`UaSecurityMode`: serde roundtrip, стабильные токены, миграция legacy (`"Авто"`, `"Auto"`, `"None"`, short/full URI), неизвестное = ошибка, маппинг в URI, совместимость policy/mode | ✅ | `security_enums_serde_roundtrip_and_stable_tokens`, `security_legacy_strings_deserialize`, `security_unknown_strings_are_errors_not_auto`, `security_policy_uri_mapping`, `security_pair_compatibility`, `endpoint_security_legacy_json_migrates_to_enums` — `src/bff/model.rs` |
| Строгая резолюция: явная политика/режим должны быть в списке endpoint'ов, иначе ошибка (без fallback); неизвестная политика сервера — ошибка; пустой список — ошибка | ✅ | `explicit_unsupported_choice_is_an_error_not_a_fallback`, `explicit_mode_not_offered_is_an_error`, `explicit_policy_not_offered_is_an_error`, `invalid_policy_mode_combination_is_an_error`, `empty_endpoint_list_is_an_error`, `unknown_advertised_policy_is_an_error_not_none`, `security_error_prefix_roundtrips_for_the_ui_wrapper` — `src/bff/ua.rs` |
| Миграция выбора политики/режима в настройках (default Auto, legacy строки, неизвестное = ошибка, отсутствующие поля = Auto) | ✅ | `ua_security_selection_defaults_to_auto_and_roundtrips`, `legacy_security_strings_in_settings_migrate`, `unknown_security_string_in_settings_is_an_error`, `old_settings_missing_security_selection_loads_as_auto` — `src/settings.rs` |
| Подключение с явной политикой None/None (+ session timeout + trust) | ✅ (live `--ignored`) | `server_connect_explicit_none_policy` — `tests/ua_security.rs` |
| Подключение с Auto-политикой резолвится против реального сервера, info содержит политику/режим | ✅ (live `--ignored`) | `server_connect_auto_resolves_security` — `tests/ua_security.rs`; `live_auto_security_still_connects` — `tests/security_live.rs` |
| Явная Basic256Sha256/SignAndEncrypt против сервера без такого endpoint'а — понятная ошибка, не fallback | ✅ (live `--ignored`) | `live_explicit_unadvertised_security_is_an_error` — `tests/security_live.rs` |
| Персист «Доверять сертификату сервера» и таймаут сессии (120 c по умолчанию) | ✅ | `ua_security_defaults_are_strict`, `ua_security_fields_serde_roundtrip`, `old_settings_missing_ua_fields_load_with_defaults` — `src/settings.rs`; `connect_dialog_trust_and_timeout_restore_from_settings` — `tests/ui_interactions.rs` |
| Элементы безопасности в диалоге подключения UA (политика/режим/доверие/идентичность/таймаут) | ✅ | `connect_dialog_ua_security_controls_render`, `connect_dialog_trust_checkbox_defaults_and_toggles` — `tests/ui_interactions.rs` |
| Типизированные security-combo: значения из enum, выбор сохраняется в настройках | ✅ | `connect_dialog_security_combos_are_typed_and_persist` — `tests/ui_interactions.rs` |
| Невалидное сочетание политики/режима и необъявленный endpoint — локализованная ошибка в диалоге, подключения нет | ✅ | `connect_dialog_invalid_security_pair_shows_error_and_does_not_connect`, `connect_dialog_unadvertised_security_choice_shows_error_and_does_not_connect` — `tests/ui_interactions.rs` |
| Поиск серверов в сети (mDNS/LDS, `FindServersOnNetwork`) | ✅ (live `--ignored`) | `server_network_search_reaches_server` — `tests/ua_security.rs` |
| Поиск OPC UA серверов (диалог подключения) — хост/порт пусты по умолчанию | ✅ | `connect_dialog_ua_search_empty_on_open` — `tests/ui_interactions.rs` |
| Событие `UaServersFound` заполняет результаты поиска | ✅ | `ua_servers_found_populates_results_state` — `tests/ui_interactions.rs` |
| Поиск OPC UA серверов на 127.0.0.1 | ✅ (интеграционный) | `search_ua_servers_on_loopback` — `tests/ua_search.rs` (`--ignored`, CODESYS на 127.0.0.1:4840) |
| Поиск OPC UA серверов через «localhost» (IPv4-first) | ✅ (интеграционный) | `search_ua_servers_via_localhost_resolves_ipv4_first` — `tests/ua_search.rs` (`--ignored`) |
| Подключение/отключение OPC DA | ⛔ | Требует запущенный DA-сервер (Kepware/Matrikon); сейчас сервис внезапно остановлен |
| Обзор дерева (Browse) | ✅ (интеграционный, UA) | `server_browse_add_and_read` — `tests/ua_smoke.rs` (`--ignored`) |
| Восстановление соединения (auto_reconnect), экспоненциальный backoff 1/2/4/8…, лимит 20 попыток, единый `ReconnectState` (`Idle`/`Waiting`/`Stopped`/`Exhausted`) | ✅ | `reconnect_delay_doubles_exponentially`, `reconnect_delay_never_overflows` — `src/app.rs`; `reconnect_attempts_cap_stops_the_loop_and_shows_exhausted`, `reconnect_stop_sets_flag_and_changes_hint`, `servers_row_reconnect_counter_and_stop_resume_toggle`, `disconnect_keeps_container_and_does_not_arm_reconnect` — `tests/ui_interactions.rs` |
| Сохранение строк/дерева после потери связи (значения помечаются «устаревшие данные») | ✅ | `disconnect_event_keeps_rows_and_marks_them_stale` — `tests/ui_interactions.rs` |
| Автопереподписка всех строк + переобзор корня + немедленное чтение после переподключения | ✅ | `reconnect_connected_resubscribes_rows_requests_browse_and_read` — `tests/ui_interactions.rs` |
| Подавление фонового опроса во время обзора (read-tick suppressed при root_loading) | ✅ | `read_tick_is_suppressed_while_root_loading` — `tests/ui_interactions.rs` |
| UA push-подписка: после AddItems создаётся одна подписка на сессию, DataChange-колбэк шлёт `EventKind::Values` без периодического Read | ✅ (live `--ignored`) | `ua_subscription_pushes_values_without_reads_and_writes` — `tests/ua_subscription.rs` (pure push CurrentTime + write-push) |
| UA push-подписка: `Active` отключает периодический Read; `Inactive`/`Failed` возвращаются к опросу (DA продолжает поллить) | ✅ | `ua_subscription_active_stops_periodic_read`, `ua_failed_subscription_falls_back_to_polling`, `da_keeps_polling_unaffected_by_subscription_flag` — `tests/ui_interactions.rs` |
| Переподключение UA: Connected → re-add items → подписка/мониторинг пересоздаются, push возобновляется | ✅ | юнит `ua_reconnect_resubscribes_rows_and_resets_subscription_flag` — `tests/ui_interactions.rs`; live `ua_subscription_resumes_after_reconnect` — `tests/ua_subscription.rs` (`--ignored`) |
| RemoveItems: удаление контролируемых элементов; Disconnect — удаление подписки (`SubscriptionState::Inactive`) | ✅ | логика в `src/bff/ua.rs` (`remove_items`/`teardown_subscription`); событие `SubscriptionState` обрабатывается в `src/app.rs` |
| Состояние подписки в информационном popup сервера (`Подписка: активна/неактивна/ошибка`) | ✅ | `server_info_popup_shows_subscription_state` — `tests/ui_interactions.rs` |
| Корреляция `request_id`: событие с неизвестным/просроченным id игнорируется; совпавший id принимается один раз; push (id 0) принимается всегда; Disconnect+Connect не даёт старому `Connected` менять новую сессию | ✅ | `stale_reply_with_unknown_request_id_is_ignored`, `matching_request_id_reply_is_accepted`, `push_event_with_zero_request_id_is_accepted`, `disconnect_connect_does_not_let_old_connected_mutate_new_state` — `tests/ui_interactions.rs` |
| Рекурсивный add-children коррелирует `Browse` по выданному им `request_id` (ручной/чужой Browse не съедается) | ✅ | `add_children_ignores_unrelated_browse_request_id` — `src/app.rs`; существующие `add_children_*`/`add_dropped_tree_*` обновлены на реальные id |
| Мёртвая зона (абс.): опция в «Параметры», settings-поле, DataChangeFilter Absolute deadband | ✅ | `DeadbandLabel`/`DeadbandHint` (i18n ru/en/zh); юниты `deadband_filter_builds_absolute`, `deadband_filter_off_by_default`, `parsed_absolute_deadband_matches` — `src/bff/ua.rs`; serde `ua_deadband_defaults_off_and_serde_roundtrips` — `src/settings.rs`. **async-opcua 0.19 экспонирует `DataChangeFilter` c `DeadbandType::Absolute` → deadband реализован полностью (не только задокументирован).** |
| Обновление (Browse) не ждёт очереди периодических чтений: драйн очереди команд в UA-воркере | ✅ | `controls_jump_over_reads_without_losing_them`, `control_command_classification` — `src/bff/ua.rs`; live `browse_refresh_jumps_over_queued_reads` — `tests/ua_refresh_speed.rs` (`--ignored`, требуемый UA-сервер) |
| Добавление сигналов (выбранный/дочерние/по имени) | ✅ (по имени: пути UA) | Live `ua_resolve_browse_path_resolves_objects`; «Добавить по имени» распознаёт не-NodeId имя как BrowsePath, добавляет первый target (`src/app.rs`) |
| Drag & drop: перетаскивание узлов дерева на таблицу сигналов (`TreeDragPayload`: ветка → рекурсивно все дочерние Item через `AddChildrenJob`, Item → строка, дедупликация/циклы через visited-set, лимит `ADD_CHILDREN_CAP`=5000 с warn, кросс-сессия → warn «только активный сервер») | ✅ | Модель: `add_dropped_tree_*`, рекурсивный `add_children_*` — `src/app.rs`. **Headless proof** реального drag-жеста (press → move>6px → hover → release через `RawInput`-кадры и тестовые хуки `src/ui.rs`, драйв `dnd_set_drag_payload` → `dnd_drop_zone`): `ui::dnd_proof::drag_item_onto_tags_table_adds_row`, `ui::dnd_proof::plain_click_on_tree_row_does_not_drop` — `src/ui.rs` (kittest-`event()` не способен удерживать кнопку между кадрами, поэтому драйв идёт через прямой `Context::run_ui`+`RawInput`) |
| Таблица: колонки | ✅ | `columns::tests` — `src/columns.rs` |
| Таблица: статусы строк | ⏳ | Агент UI, фаза 1 |
| Таблица: копирование | ⏳ | Агент UI, фаза 1 |
| Запись значения (тип, записываемый тип, авто-тип) | ✅ | Юнит: `roundtrip_*` — `src/bff/da_variant.rs`, `typed_*` — `src/bff/ua.rs`; интеграция: `ua_write_readback_and_restore` — `tests/ua_write.rs`, `da_write_roundtrip` — `tests/da_write.rs` |
| Опрос и интервалы (UPDATE/TEST-циклы) | ⛔ | Цикл в `src/app.rs:638-674` (использует `interval*`), вне зоны изменений |
| Восстановление соединения (auto_reconnect) | ✅ | см. строки выше (backoff/лимит, сохранение строк, переподписка после переподключения); `reconnect_stop_sets_flag_and_changes_hint` — `tests/ui_interactions.rs` |
| CSV: загрузка | ⏳ | Агент CSV, фаза 2 (парсер в `src/ui.rs`) |
| CSV: сохранение | ⛔ | `save_tree_csv`/`csv_escape` в `src/ui.rs`, вне зоны изменений |
| Настройки (обычные) | ✅ | `settings::tests` — `src/settings.rs` |
| Журнал | ✅ | `journal::tests` — `src/journal.rs` |
| Скрипты: редактор | ⏳ | Редактор скриптов, фаза 2 |
| Скрипты: интерпретатор | ⛔ | Интерпретатор не реализован (в проекте не существует) |
| Клавиатура / ПКМ (контекстное меню) | ✅ | `tests/ui_interactions.rs`: `arrow_down_moves_and_shift_down_extends_selection`, `ctrl_a_selects_all_rows`, `delete_removes_selected_row`, `tree_row_context_menu_copies_node_name`, `table_row_context_menu_removes_selected` |
| Фокус дерева по клику в панели тегов (в т.ч. пустое место) | ✅ | `tree_click_focus_enables_arrow_navigation`, `tree_item_click_keeps_focus_for_arrows` — `tests/ui_interactions.rs` |
| Сброс фокуса дерева при фокусе текстового поля | ✅ | `tree_focus_cleared_by_typing_in_text_field` — `tests/ui_interactions.rs` |
| Enter по узлу дерева: подписанный Item → write-диалог; неподписанный → добавить строку без диалога; ветка → развернуть/свернуть | ✅ | `tree_enter_on_subscribed_item_opens_write_dialog`, `tree_enter_on_unsubscribed_item_adds_row_without_dialog` — `tests/ui_interactions.rs` |
| Диалог подключения: открытие на вкладке OPC DA + авто-поиск (da_pending=true) | ✅ | `connect_dialog_opens_on_da_tab_with_auto_discovery` — `tests/ui_interactions.rs` |
| Диалог подключения: DA авто-поиск комбинированный (этот компьютер + известные удалённые хосты; без пресета адреса) | ✅ | `connect_dialog_da_auto_search_uses_combined_flag` — `tests/ui_interactions.rs` |
| Диалог подключения: единая схема полей, порт виден только на вкладке UA, поле порта пустое по умолчанию | ✅ | `connect_dialog_port_field_visibility_per_tab` — `tests/ui_interactions.rs` |
| Диалог подключения: пустой UA-хост запускает комбинированный поиск (network + 127.0.0.1 + localhost; результаты объединяются) | ✅ | `connect_dialog_ua_empty_host_starts_combined_search` — `tests/ui_interactions.rs` |
| Диалог подключения: выбор DA-сервера включает подключение / заполняет состояние | ✅ | `connect_dialog_select_da_server_enables_connect` — `tests/ui_interactions.rs` |
| Диалог подключения: выбор результата UA-поиска заполняет URL | ✅ | `connect_dialog_select_ua_result_fills_url` — `tests/ui_interactions.rs` |
| Диалог столбцов: кнопки «↑/↓» сразу переставляют строки и персистятся | ✅ | `columns_dialog_down_moves_row_and_persists` — `tests/ui_interactions.rs` |
| Диалог столбцов: DnD перетаскивание строки за «ручку» на «вставку» переставляет и персистит | ✅ | `ui::dnd_proof::columns_dialog_drag_row_reorders_and_persists` — `src/ui.rs` (headless RawInput: grip → gap, как в `drag_item_onto_tags_table_adds_row`) ; модель `move_to_insert_*` — `src/columns.rs` |
| Диалог столбцов: клавиатурный фолбэк Alt/Ctrl+↑/↓ перемещает сфокусированную строку и персистит | ✅ | `columns_dialog_keyboard_alt_down_reorders_and_persists`, `columns_dialog_keyboard_alt_up_reorders_and_persists` — `tests/ui_interactions.rs` |
| Контекстное меню области тегов («Столбцы...», «Добавить по имени...», «Обновить ветку», «Сохранить CSV...», «Выделить все», «Снять выделение») | ✅ | `row_menu_wins_over_panel_menu_and_background_opens_panel` — `tests/ui_interactions.rs` (ПКМ по строке → меню строки; ПКМ по фону/строке-счётчику → панельное меню) |
| Приоритет контекстных меню: ПКМ по строке открывает меню строки, а не панельное | ✅ | `row_menu_wins_over_panel_menu_and_background_opens_panel` — `tests/ui_interactions.rs` |
| Ctrl+A, затем Del удаляет выбранные строки (регрессия фокуса) | ✅ | `ctrl_a_then_delete_removes_rows` — `tests/ui_interactions.rs` |
| Esc закрывает верхний диалог (write-диалог) | ✅ | `esc_closes_write_dialog` — `tests/ui_interactions.rs` |
| Enter подтверждает основное действие (write-диалог → «Записать») | ✅ | `enter_confirms_write_dialog` — `tests/ui_interactions.rs` |
| Подтверждение записи (confirm_write) | ✅ | `tests/ui_interactions.rs`: `write_dialog_shows_confirmation_when_enabled`, `write_confirm_no_keeps_write_dialog`, `write_without_confirm_writes_immediately` |
| Переключение вкладок Скрипт / Журнал (взаимоисключающие) | ✅ | `tests/ui_interactions.rs`: `tab_exclusive_switch_keeps_flags_and_changes_view`, `single_tab_does_not_show_unavailable_tab` |
| Упорядочивание колонок ПКМ «Влево/Вправо» + персист | ✅ | `tests/ui_interactions.rs`: `column_reorder_persists_in_settings`; юнит `src/settings.rs` |
| Прогресс импорта CSV (модальный TFormLoad) | ⏳ | `start_csv_import` + `csv_import_dialog`; покрытие прогресса/отмены — ручная проверка (юнит: разбор строк покрыт через `import_tree_csv`) |
| Индикация записи «Запись...» и итог «Записано: N из M» | ✅ | `tests/ui_interactions.rs`: `pending_write_shows_status_label_then_summary`; логика завершения в `src/app.rs` (`on_event::Wrote`) |
| Всплывающее окно по индикатору сервера | ✅ | `tests/ui_interactions.rs`: `status_popup_opens_from_indicator_state` |
| Переподключение: счётчик/backoff и остановка попыток | ✅ | `tests/ui_interactions.rs`: `reconnect_stop_sets_flag_and_changes_hint`; логика в `src/app.rs` |
| Вкладки-контейнеры сессий (tabs architecture): создание вкладки «Двойной щелчок по серверу» | ✅ | `open_tab_for_endpoint_creates_visible_active_session` — `tests/ui_interactions.rs` (путь через `open_tab_for_endpoint`, тот же, что двойной щелчок) |
| Клик/выбор вкладок () — активная сессия и отрисовка её данных | ✅ | `tab_switch_changes_active_session_and_rendered_rows` — `tests/ui_interactions.rs` |
| Полоса вкладок рендерится ВНУТРИ области тегов (без отдельной оконной верхней панели `session_tabs`) | ✅ | `tab_strip_is_in_tags_area_not_top_panel` (код-уровень: `top_tab_panel_enabled() == false`) — `tests/ui_interactions.rs` |
| В полосе вкладок НЕТ кнопки «Подключиться»/«Подключиться...» (подключение — только слева и в меню) | ✅ | `tab_strip_has_no_connect_button` — `tests/ui_interactions.rs` |
| Кнопка «+» в КОНЦЕ полосы вкладок («Новое подключение») открывает диалог подключения | ✅ | `tab_strip_plus_opens_connect_dialog` — `tests/ui_interactions.rs` |
| Переполнение вкладками: горизонтальная прокрутка в заголовке тегов, таблица не ломается | ✅ | `tab_strip_many_tabs_scroll_without_breaking_table` — `tests/ui_interactions.rs` |
| Дедублирование контейнера по endpoint (повторное открытие реактивирует ту же сессию) | ✅ | `reopen_tab_for_same_endpoint_does_not_duplicate` — `tests/ui_interactions.rs` |
| «×» вкладки скрывает только вид (контейнер/подключение сохраняются) | ✅ | `tab_close_button_only_hides_view`, `close_tab_hides_view_keeps_rows_and_keeps_connection` — `tests/ui_interactions.rs` |
| Close-вкладки (×): скрывает вид, сохраняет строки/дерево/адрес, НЕ отключает Connected (закрытие ≠ отключение) | ✅ | `close_tab_hides_view_keeps_rows_and_keeps_connection` — `tests/ui_interactions.rs` |
| Close активной вкладки → активация ближайшей видимой | ✅ | `close_active_tab_activates_nearest_visible` — `tests/ui_interactions.rs` |
| «Удалить из списка» (ПКМ по вкладке) + подтверждение → полное удаление контейнера и строк | ✅ | `tab_context_menu_remove_from_list_drops_container`, `remove_session_drops_container_and_rows` — `tests/ui_interactions.rs` |
| Пустое состояние вкладок (нет видимых) — только локализованная подсказка (без кнопки) | ✅ | `tab_bar_shows_empty_state_even_with_hidden_containers` — `tests/ui_interactions.rs` |
| **Левая панель «Серверы»**: полная ширина «Добавить сервер» сверху открывает диалог | ✅ | `servers_add_button_opens_connect_dialog` — `tests/ui_interactions.rs` |
| Строка сервера — таблица 3 колонок: точка-состояние + имя + «×» (деструктивно) | ✅ | `servers_row_shows_state_name_and_remove` — `tests/ui_interactions.rs` |
| Имя сервера: суффикс « — <host>» для удалённого DA (unit-композиция) | ✅ | `compose_name_for_remote_da_appends_host`, `servers_row_applies_remote_host_suffix` — `src/app.rs` / `tests/ui_interactions.rs` |
| Удаление строки «×» требует подтверждения `RemoveSession`; контейнер+строки удаляются только после «Да», отмена сохраняет | ✅ | `servers_row_remove_requires_confirm_and_drops_after_yes`, `servers_row_remove_cancel_keeps_container` — `tests/ui_interactions.rs` |
| Иконка переподключения «↻ N/лимит» + «⏸»/«▶» переключаются по состоянию попыток | ✅ | `servers_row_reconnect_counter_and_stop_resume_toggle` — `tests/ui_interactions.rs`. Иконки рисуются painter-ом (нет шрифтовой «каши»); доступное имя кнопки = локализованный тултип (`WidgetInfo::labeled`), по нему и идут тесты (не по глифу «×»/«↻») |
| Тулбар «Подключиться/Отключиться/↻ Переподключиться» — корректные enabled-состояния | ✅ | `toolbar_enable_states_follow_active_session` — `tests/ui_interactions.rs` |
| Отключение (меню «Отключиться») сохраняет контейнер/строки и НЕ вооружает автопереподключение | ✅ | `disconnect_keeps_container_and_does_not_arm_reconnect` — `tests/ui_interactions.rs` |
| Полное имя/значение в буфер обмена | ⏳ | копирование реализовано в `src/ui.rs` (`copy_text` + статус), прямое покрытие — отсутствует |
| Диалог информации о сервере (About + «Подключённые серверы») | ⏳ | блок в `about_dialog`; vendor/version недоступны из BFF |
| Подтверждение отключения («Отключиться от сервера "X"?») | ⏳ | `request_disconnect` + `ConfirmDialog`; тест не добавлен (см. ui-parity.md) |
| Состояние «дерево не загружено» | ⏳ | текст в `navigation` при пустом дереве |
| Интеграционные тесты UA | ✅ (запуск вручную) | `tests/ua_smoke.rs`, `tests/ua_write.rs`, запуск `cargo test --test ua_smoke -- --ignored`, `cargo test --test ua_write -- --ignored --nocapture` |
| Грациозное закрытие UA (жизненный цикл): 5× connect/browse/add/read/disconnect + 3× поиск, нет `ESTABLISHED`-сокетов после | ✅ (live `--ignored`) | `ua_lifecycle_two_dozen_connections_close_cleanly` — `tests/ua_lifecycle.rs`; реализация грациозного `disconnect()`/close на таймауте подключения в `src/bff/ua.rs` |
| Ограничение размера UA-запросов: Browse page cap 100 + continuation, Read пачками по 100 | ✅ (live `--ignored`) | `ua_large_browse_read_and_cycles` — `tests/ua_large.rs` (120 items в одной команде Read + 5 циклов) |
| Интеграционные тесты DA | ⏳ (env-гейт) | `da_write_roundtrip` — `tests/da_write.rs`; запуск `cargo test --test da_write -- --ignored --nocapture` при `DA_TEST_PROGID` (например `Kepware.KEPServerEX.V6`) |

---

## Локализация (i18n)

| Функция | Статус | Тест / файл |
|---|---|---|
| Словарь строк ru/en/zh: каждому ключу `Tr` есть непустое значение во всех трёх языках | ✅ | `every_key_is_nonempty_in_all_languages` — `src/i18n.rs` |
| Язык по умолчанию — русский (ru); En/Zh-метки по-прежнему переводятся | ✅ | `default_language_is_russian` — `src/i18n.rs`; `fresh_app_defaults_to_russian` — `tests/ui_interactions.rs` |
| `Lang` сериализации ("ru"/"en"/"zh") + `Default` = Ru | ✅ | `lang_serde_roundtrips` — `src/i18n.rs` |
| Переключение известных меток (Сигналы→Signals→信号, Язык→Language→语言, опции меню RU/EN/ZH) | ✅ | `english_and_chinese_switch_known_labels` — `src/i18n.rs` |
| `fmt` плейсхолдеры `{}` / `{0}` (позиционные и повторяющиеся) | ✅ | `fmt_substitutes_positional_and_named` — `src/i18n.rs` |
| Поле `Settings.lang` + serde fallback на ru при отсутствии/неизвестном коде; явно сохранённый язык ("en"/"zh") не перезаписывается | ✅ | `unknown_or_missing_lang_falls_back_to_russian`, `settings_defaults_field_values`, `serde_roundtrip_json_preserves_all_fields`, `fresh_settings_default_to_russian_and_persisted_lang_is_kept` — `src/settings.rs`; `language_switch_persists_in_settings` (персист Ru затем Zh) — `tests/ui_interactions.rs` |
| Меню «Язык/Language/语言»: выбор RU/EN/ZH переключает интерфейс на лету | ✅ | `language_menu_switches_ui_language` — `tests/ui_interactions.rs` (метки меню/вкладок; пункты подменю через словарь в `i18n.rs`) |
| Персист языка в settings.json | ✅ | `language_switch_persists_in_settings` — `tests/ui_interactions.rs` |
| Переключение языка не очищает строки и выделение | ✅ | `language_switch_keeps_rows_and_selection` — `tests/ui_interactions.rs` |
| Новые ключи i18n: `AddServer`, `RemoveServerTooltip`, `ReconnectTooltip`, `ReconnectCount`, `NewConnection`, `TableEmptyInner`, `AddChildrenProgress`, `AddChildrenCapHit`, `FolChildrenNone` (ru/en/zh) | ✅ | `every_key_is_nonempty_in_all_languages` — `src/i18n.rs` |
| Локализация журнала: запись хранит ключ+аргументы (`l10n`), `render(lang)` пересобирает текст, `save_localized(path, lang)` для экспорта | ✅ | Дизайн: `src/journal.rs`; юнит покрытие строк сохранения `save_writes_expected_line_structure`, `save_then_load_roundtrip_preserves_text` — `src/journal.rs` |
| Экспорт журнала вызывается через `save_localized` с текущим языком (`save_journal`) | ✅ | Реализация в `src/ui.rs`; ручная проверка |
| CSV-экспорт/импорт сохраняют исходный русский формат (заголовок `server;item_id;name;value;quality;time` и значения строк без перевода) — совместимость с оригиналом Delphi | ✅ (зафиксировано) | `csv_save_load_roundtrip_preserves_rows` — `tests/ui_interactions.rs`; формат не локализуется (см. примечание ниже) |

Примечание о CSV: экспорт/импорт не локализуются — заголовок (`server;item_id;name;value;quality;time`) и разделители остаются как в Delphi-оригинале; канонические русские заголовки колонок доступны как `columns::ColumnId::csv_title()`/`csv_hint()` (не локализуются, для совместимости с Delphi). Локализуются только журнальные сообщения о результате операций с файлами. «нет данных»/«ожидание значения» — серверные маркеры-данные строк таблицы и по-прежнему локализуются через i18n (`NoData`/`WaitingValue`), но замена маркера определяется по sentinel-коду `Quality::CODE_PLACEHOLDER_*`, а не по тексту.

## Итог запусков

| Команда | Результат |
|---|---|
| `cargo test --lib` | 148 passed, 0 failed, 0 ignored |
| `cargo test --doc` | 0 doc-тестов |
| `cargo test --test value_types` | 6 passed, 0 failed, 0 ignored |
| `cargo test --test ui_interactions` | 84 passed, 0 failed |
| `cargo test --test operation_guard` | 1 passed, 0 failed |
| `cargo test --test gateway_shutdown` | 2 passed, 0 failed |
| `cargo test --test no_hardcoded_ui_strings` | 2 passed, 0 failed |
| `cargo test --test ua_types -- --ignored --nocapture` | 5 passed |
| `cargo test --test ua_smoke -- --ignored --nocapture` | 2 passed |
| `cargo test --test ua_write -- --ignored --nocapture` | 1 passed |
| `cargo test --test ua_refresh_speed -- --ignored --nocapture` | 1 passed |
| `cargo test --test ua_search -- --ignored --nocapture` | 2 passed |
| `cargo test --test ua_security -- --ignored --nocapture` | 4 passed |
| `cargo test --test ua_subscription -- --ignored --nocapture` | 2 passed (pure push CurrentTime + write-push; reconnect re-subscription) |
| `cargo test --test ua_lifecycle -- --ignored --nocapture` | 1 passed (5× cycle connect/browse/add/read/disconnect + 3× one-shot search) |
| `cargo test --test ua_large -- --ignored --nocapture` | 1 passed (Browse cap 100 + continuation; 120-item AddItems; 120-key Read in one command; +5 cycles) |
| `cargo build --bin opcda-egui` | чистая сборка, без новых warning |

### Количество тестов по модулям (новые)

| Модуль | Новых тестов |
|---|---|
| `src/settings.rs` | 17 (`+1`: русский дефолт/персист явного языка) |
| `src/columns.rs` | 15 (`+3` DnD `move_to_insert_*`) |
| `src/journal.rs` | 10 |
| `src/ui.rs` | 3 headless DnD proof (`ui::dnd_proof::drag_item_onto_tags_table_adds_row`, `ui::dnd_proof::plain_click_on_tree_row_does_not_drop`; **+1 `columns_dialog_drag_row_reorders_and_persists`**) — реальный drag-жест через `Context::run_ui`+`RawInput` и тестовые хуки |
| `src/i18n.rs` | 5 (lang serde + `Default=Ru`, полнота словаря ru/en/zh, дефолт ru, переключение известных меток, format placeholders; +8 новых `Tr`-ключей `Ctx*`/`ColumnsDragHandle`/`ColumnsReorderHint`, покрыты `every_key_is_nonempty_in_all_languages`) |
| `src/app.rs` | 9 (+6 → backoff/имя сервера/рекурсивный add_children; **+1 `clamp_options_enforces_interval_and_timeout_ranges`; +1 `every_operation_has_a_localized_label`; +1 `add_children_ignores_unrelated_browse_request_id`**) |
| `src/constants.rs` | 6 (UA-дефолты, reconnect cap, диапазоны опций, размеры UI, browse-капы + page cap 100 / read chunk 100 / discovery guard, длины усечения) |
| `src/bff/model.rs` | **2** (`operation_codes_are_ascii_and_cover_the_emitters` — теперь `Operation`; `command_kind_maps_to_its_operation`) |
| `src/bff/da_variant.rs` | 23 (VT-roundtrip + массивы) |
| `src/bff/ua.rs` | 30 (маппинг `Value` → UA `Variant`, имена типов; классификация команд + драйн очереди; новые типы; безопасность; **+2 новых: подписка — `deadband_filter_*`, `parsed_absolute_deadband_matches`, `subscription_monitoring_parameters_shape`**) |
| `tests/ua_types.rs` | 5 (live `--ignored`: filter-browse, Views, ResolveBrowsePath, типы без паники, массивы/структуры) |
| `tests/value_types.rs` | 6 (unit: новые `Value`-варианты, hex-байты, усечение массивов) |
| `tests/ui_interactions.rs` | 109 (**+7 network cache/refresh/selection/IP/binding тестов; +1 `on_exit_shuts_down_the_gateway`; +6 request-id/subscription-state тестов; ранее +3 контекстных меню/клавиатуры**) |
| `tests/operation_guard.rs` | 1 (статический запрет сырых operation-строк/`op::`) |
| `tests/gateway_shutdown.rs` | 2 (send после shutdown = Err; bounded/idempotent shutdown) |
| `tests/ua_refresh_speed.rs` | 1 (live `--ignored`: скорость Browse при подписках) |
| `tests/ua_search.rs` | 2 (live `--ignored`: 127.0.0.1 и localhost) |
| `tests/ua_security.rs` | 4 (live `--ignored`: endpoints, explicit None connect, Auto connect, mDNS/LDS network search) |
| `tests/ua_subscription.rs` | 2 (live `--ignored`: pure push + write-push; reconnect re-subscription) |
| `tests/ua_lifecycle.rs` | 1 (live `--ignored`: 5× connect/browse/add/read/disconnect + 3× one-shot search; проверка грациозного закрытия каналов; Read после AddItems проверяет порядок/полноту) |
| `tests/ua_large.rs` | 1 (live `--ignored`: скан дерева с cap 100 + continuation, 120 items в одной AddItems, 120 ключей в одной Read-команде, 5 полных циклов) |

### Поведение клавиатуры/фокуса и раскладки (данный проход)

- Фокус при открытии: write-диалог → поле «Значение», connect-диалог → хост (DA)/URL (UA), add-signal → «Имя сигнала», options → первый контрол. Защищено от ломки kittest-тестов (`focus_pending` сбрасывается на кадре открытия).
- Фокус дерева: клик в любой точке панели тегов (включая пустое место под узлами) ставит флаг `app.tree_focused`; стрелки/Enter/F5 работают по флагу. Флаг сбрасывается, когда сфокусировано текстовое поле или открыт модальный диалог (`text_edit_focused` / `dialogs.any_open()`). Клики по обычным кнопкам/переключателям фокус дерева НЕ сбрасывают — иначе стрелки перестали бы работать после клика.
- Enter по узлу дерева: ветка → развернуть/свернуть; Item уже подписанный (есть строка с тем же session+item_id) → открыть write-диалог для этой строки; Item неподписанный → добавить строку (как двойной клик) без write-диалога, в журнал пишется «Сигнал добавлен в таблицу».
- Диалог подключения: единая схема полей для DA/UA (список серверов → адрес → порт (только UA) → поиск → пользователь/пароль → URL-строка → Подключить/Отмена). По умолчанию вкладка OPC DA и сразу запускается комбинированный авто-поиск (`discover_da_all`: этот компьютер + известные удалённые хосты из истории; `da_pending=true`, спиннер в списке). Поля «Адрес»/«Порт» пусты по умолчанию (без пресетов; плейсхолдеры, не значения), под полями подсказка «Пусто — поиск на этом компьютере и в локальной сети». При открытии вкладки UA с пустыми полями запускается комбинированный поиск (mDNS/LDS + 127.0.0.1:4840 + localhost:4840, результаты объединяются/дедуплицируются). Выбор DA-сервера заполняет ProgID/URL и включает «Подключиться»; выбор результата UA-поиска заполняет URL.
- Esc закрывает верхний диалог (не приложение); Enter подтверждает: write → «Записать», connect → «Подключиться», add-signal → «Добавить», confirm → «Да».
- При открытой поверх квитанции подтверждения (confirm_write) write-диалог не реагирует на Enter/Esc — реагирует только верхний (confirm) диалог.
- Все модальные окна центрированы через `.anchor(Align2::CENTER_CENTER, [0.0, 0.0])`; попап по индикатору сервера остаётся у статус-бара.
- Статус-бар и нижние вкладки переструктурированы против наложений: укороченные подписи («Подключено: …», «Сигналов: N | Выбрано: M», «Обновлено HH:MM:SS», «Повторное подключение N/20 через M с», «Попытки исчерпаны (20)», «Обновление дерева...»), длинные endpoint-URL обрезаются до ~36 символов с «…», длинные подсказки и заметка о скриптах вынесены в tooltip, вкладки «Журнал»/«Скрипт» с tooltip.

### Производительность обновления (BUG 1) и переподключение (BUG 2)

- BUG 1: «Обновить» не должен ждать очередь периодических чтений. В UA-воркере контрольная команда (Browse/Connect/AddItems/Write/RemoveItems/Disconnect/Test/SearchUaServers) обрабатывается сразу, а накопленные в очереди чтения переупорядочиваются в конец (не отбрасываются — ответные чтения не теряются). На стороне приложения фоновый опрос подавляется, пока идёт обзор (`root_loading` / `item.loading`). Live-замер: Browse при 10 подписках ≈ **10 мс** (`tests/ua_refresh_speed.rs`).
- BUG 2: backoff 1/2/4/8… с максимумом 20 попыток, после чего цикл останавливается (статус «Попытки исчерпаны (20)», ручной сброс через «Восстановить подключение»). При потере связи строки и дерево остаются на месте, значения помечаются «устаревшие данные»; при восстановлении все строки автоматически переподписываются, читаются и корень дерева переобозревается.

Добавлено в этом проходе:
- Диалог столбцов «↑/↓»: кнопки сразу переставляют строки в собственном списке диалога (локальный `order` + `persist_column_order(&order)` + `save()`), таблица за диалогом перерисовывается; перемещённая строка подсвечивается ~1 с и под списком показывается «Перемещено: <Имя>». Заголовочное меню «Влево/Вправо» использует ту же логику и осталось рабочим.
- **Элегантное перемещение колонок (DnD)**: в диалоге «Настройка столбцов» каждая строка получила painter-ручку (3 короткие линии + точка — без шрифтовых глифов), которую можно перетащить (payload = `ColumnId`) на «вставку» между строками; при наведении рисуется акцентная линия-индикатор, на сброс строка вставляется (`ColumnSet::move_to_insert`, персист через `persist_column_order`), таблица за диалогом обновляется сразу. Клавиатурный фолбэк: Alt/Ctrl+↑/↓ перемещают сфокусированную (или первую) строку. Кнопки «↑/↓» остались как фолбэк. Перетаскивание ЗАГОЛОВКОВ таблицы не реализовано (egui_extras резервирует заголовок под resize/контекстное меню) — см. ui-parity.md.
- **Контекстное меню области тегов** («Столбцы...», «Добавить по имени...», «Обновить ветку», «Сохранить CSV...», «Выделить все», «Снять выделение») привязано только к строке-счётчику «Сигналов: N | Выбрано: M» и к пустому телу таблицы — поверхностям, которые никогда не перекрывают строку или заголовок, поэтому ПКМ по строке всегда открывает меню строки (без наложений).
- **Язык по умолчанию — русский**: `Lang::default()` и `default_lang()` возвращают `ru`; `Settings::lang()` фолбэчит на `Ru` при неизвестном/пустом коде. Явно сохранённый в `settings.json` язык (например `"en"`) не перезаписывается. Обновлены тесты, предполагавшие китайский дефолт.
- Полная поддержка UA-типов: `Value::NodeId` / `QualifiedName` / `LocalizedText`; ByteString показывается hex (16 байт + «…»), массивы — «[…] …N эл.» (16 элементов); чтение многомерных массивов (nested `Value::Array`), StatusCode → `Other("0x…")`, ExtensionObject → `structured:<тип>`; запись этих типов по типу узла, отказ от структурированных и многомерных массивов с понятной ошибкой.
- Browse: фильтр по классу узла (`filter: Option<u8>`, «Только переменные» в тулбаре дерева), выбор View (`view_id`), синтетический узел «Представления», BrowsePath-резолв («Добавить по имени» для не-NodeId имени).
- Live-наблюдения (CODESYS на 127.0.0.1:4840): `0:Objects → i=85`; представлений (Views) на сервере нет → graceful; переменные читаются, включая `LocalizedText` («[en-Us] Application»), качество Good; фильтр-бrowse вернул только Variables.

Добавлены тесты: юнит — `tests/value_types.rs` (6), модуль `src/bff/ua.rs` (+6 к 14); live — `tests/ua_types.rs` (5, `--ignored`). `cargo test --lib` итого: **95 passed, 0 failed, 0 ignored**.

## Вкладки-контейнеры сессий (tabs architecture)

- Модель: серверное подключение — персистентный контейнер (`endpoint + rows + tree`). У контейнера есть флаг `SessionView.visible`. Закрытие вкладки (×) ставит `visible=false`, сохраняя строки/дерево/адрес; Connected-контейнер не отключается (закрытие ≠ отключение). Повторное открытие через двойной щелчок по серверу в диалоге реактивирует ту же сессию (правило дедупликации: `protocol + address`, у DA host — лишь фильтр компьютера). Отключение («Отключиться»/статус-попап) никогда не удаляет контейнер и не закрывает вкладку — только разрывает связь и помечает строки «устаревшие данные». Единственный деструктивный путь — ПКМ по вкладке → «Удалить из списка» (+ подтверждение), который отключает и полностью удаляет контейнер и его строки.
- Полоса вкладок — узкая строка ВНУТРИ центральной (табличной) панели, над таблицей сигналов, а НЕ оконная верхняя панель. По вкладке на видимый контейнер: цветная точка (зелёный/жёлтый/красный) + метка «DA: <сервер>» / «UA: <endpoint>» (обрезана ~30 символов) + «×» (скрыть), ПКМ → «Удалить из списка», tooltip — полный endpoint + состояние. Кнопок «+» и «Подключиться...» в полосе НЕТ (пустое состояние — только локализованная подсказка). При большом числе вкладок — горизонтальная прокрутка в заголовке тегов (таблица не ломается). Вкладки остаются доступны даже при свёрнутой левой панели «Серверы».
- Диалог подключения: одинарный клик по строке списка DA/UA — выделение (прежнее поведение); двойной щелчок — открывает/восстанавливает вкладку (`open_tab_for_endpoint`), не создавая дубликата. Кнопка «Подключиться» использует ту же дедупликацию.

Новые строки i18n (ru/en/zh): `AddTabTooltip`, `CloseTabTooltip`, `DoubleClickServerHint`, `RemoveFromList`, `RemoveFromListConfirmTitle`, `RemoveFromListConfirmText`, `EmptyTabsHint`. Добавлено в этом проходе: `EmptySearchHint`, `UaHostHint`, `UaPortHint`.

## Безопасность OPC UA

- Подключение перед соединением запрашивает фактические endpoint'ы (`GetEndpoints`), затем типизированный выбор `UaSecurityPolicy`/`UaSecurityMode` резолвится по реальному набору сервера (`resolve_security -> Result`): `Auto` — `None` → `Basic256Sha256+Sign` → `Basic256Sha256+SignAndEncrypt` → первый распознанный endpoint. Явная политика/режим должны совпасть с объявленным endpoint'ом; иначе возвращается ошибка со списком предложений (без fallback). Невалидные сочетания (None policy + Sign/SignAndEncrypt) и неизвестные значения сервера — тоже ошибки.
- `UaSecurityPolicy::as_uri()` — единственный маппинг политики в полный URI (без строковых префиксов); Auto URI не имеет. Serde принимает legacy `"Авто"`/`"Auto"`/`"None"`, короткие имена и полные URI, неизвестное значение — ошибка (не Auto).
- Диалог подключения UA: комбо «Политика»/«Режим безопасности» строятся из типизированных `ALL` с локализованными подписями (по умолчанию «Авто»); выбор сразу сохраняется в настройках и валидируется, невалидный/необъявленный вариант блокирует подключение и показывает локализованную ошибку + детали сервера. Чекбокс «Доверять сертификату сервера» (по умолчанию ВЫКЛ — строгий PKI), выбор идентичности (X509 доступен на endpoint'ах с ненулевым режимом), таймаут сессии (по умолчанию 120 000 мс), кнопка «Поиск в сети» (mDNS/LDS `FindServersOnNetwork`).
- Настройки персистят флаг доверия, таймаут сессии и типизированный выбор политики/режима (default `Auto`); старые `settings.json` без новых полей загружаются со значениями по умолчанию, legacy-строки мигрируют через enum-deserializer.
- Клиент использует общий PKI-каталог (`pki_dir`), при включённом доверии серверные сертификаты сохраняются и пропускаются строгие проверки; ошибки сертификата выносятся с подсказкой про «Доверять сертификату сервера»/PKI. О About показывает путь PKI и subject клиентского сертификата.
- Live-проверка (CODESYS на 127.0.0.1:4840, SecurityPolicy None, анонимно): поиск endpoint'ов, подключение с явной None/None и с Auto, сетевой поиск — все 4 теста `tests/ua_security.rs` зелёные. Дополнительно `tests/security_live.rs`: Auto по-прежнему подключается, а явная Basic256Sha256/SignAndEncrypt против сервера без такого endpoint'а возвращает понятную ошибку (не fallback).

## Подписки OPC UA (DataChange push)

- **Модель**: для каждой UA-сессии создаётся ровно ОДНА подписка (лениво, при первом `AddItems`). Приложение отдаёт воркеру конфиг (`CommandKind::ConfigureUaSubscription { publishing_interval_ms, deadband }`) на каждый (пере)подключении; интервал = интервал обновления приложения (по умолчанию 1000 мс), `lifetime_count = 60`, `keep_alive = 20`, `max_notifications = 0` (без лимита), `priority = 0`, `publishing_enabled = true`.
- **Monitor**: `client_handle = ItemKey` (u32), `sampling_interval = 0` (быстрейший серверный шаг), `queue_size = 1`, `discard_oldest = true`, `monitoring_mode = Reporting`, `timestamps_to_return = Both`. DataChange callback (`DataChangeCallback`) шлёт `EventKind::Values { values: Vec<(key, DataValue)> }` в общий event-канал (request_id = 0) — строки обновляются БЕЗ периодического Read, через тот же обработчик `Values`, что и Read.
- **Push vs Poll**: app-состояние `SessionView.subscription: SubscriptionState { Inactive, Active, Failed }`. Только `Active` для UA-сессии подавляет периодический `Read` (значения приходят по push); `Inactive` и `Failed` возвращаются к опросу. DA-сессии опрашиваются как раньше (состояние игнорируется). Успешный мониторинг шлёт `SubscriptionState { state: Active }`; ошибка создания/мониторинга — `Failed`; teardown/нет элементов — `Inactive`. Состояние видно в информационном popup сервера.
- **Переподключение**: новый Connect создаёт свежий `UaSession` (subscription_id = None). App на `Connected` шлёт `ConfigureUaSubscription` (новый интервал/манда) и повторно `AddItems` (register_rows) → воркер пересоздаёт подписку и пере-мониторит все строки. Отдельного бекоффа не нужно — это тот же авто-реконнект. Live-подтверждено: push возобновляется после disconnect+reconnect (`ua_subscription_resumes_after_reconnect`).
- **RemoveItems/Disconnect**: `remove_items` удаляет контролируемые элементы для удалённых ключей, при опустении сессии удаляет подписку целиком и шлёт `SubscriptionState { state: Inactive }`; `Disconnect` вызывает `teardown_subscription` перед **грациозным закрытием**: `Session::disconnect()` (CloseSession + CloseSecureChannel на проводе), затем ожидает завершения event-loop воркера; `abort()` — только как крайняя мера, если грациозное закрытие не укладывается в таймаут.
- **Мёртвая зона (абс.)**: async-opcua 0.19 **экспонирует** `DataChangeFilter` с `DeadbandType::Absolute` (см. `async-opcua-types-0.19.0/src/data_change.rs`, `generated/types/data_change_filter.rs`). Добавлена опция «Мёртвая зона (абс.)» в диалог «Параметры» (`settings.Options.ua_deadband`, default 0 = выкл, serde default), воркер строит `ExtensionObject::from_message(DataChangeFilter { trigger: StatusValue, deadband_type: Absolute, deadband_value })`. Деад-банд применяется к подписке при её создании (пере)подключения; изменение опции вступает в силу при следующем пересоздании подписки (после reconnect). Note: CODESYS-нода датчика не удерживает запись (цикл ПЛК перезаписывает значение) → live-тест находит «липкую» перезаписываемую ноду (`StatusDB.ChokeB_Size`) через trial-write/read-back с задержкой.
- **Новые события/команды/i18n**: `EventKind::SubscriptionState { state: SubscriptionState }`, `CommandKind::ConfigureUaSubscription { publishing_interval_ms, deadband }`; i18n ключи `DeadbandLabel`/`DeadbandHint` + `SubscriptionLabel`/`SubscriptionActive`/`SubscriptionInactive`/`SubscriptionFailed` (ru/en/zh). DA-воркер игнорирует `ConfigureUaSubscription` (no-op arm).
- **Live-наблюдения** (CODESYS 127.0.0.1:4840, None/anon): pure push `CurrentTime` → `DateTime(2026-10-25T11:51:04Z)` с `Good` качеством и timestamp застрял ~1 с publish interval; write `I64(0)→I64(1)` на ноду сразу же пришёл push-ом (`PUSH write value=I64(1)`); после disconnect+reconnect push `DateTime(...11:50:58Z)` возобновился.

## Корреляция запросов / ответов (`request_id`)

- **Модель**: `App::send` выдаёт монотонный `request_id != 0` и кладёт `(SessionId, RequestId)` в `pending_requests` (HashSet). `App::on_event` принимает непустой id только если пара найдена, затем удаляет её; событие с неизвестным/уже принятым id отбрасывается до любого изменения состояния. События с `request_id == 0` (DataChange push, `SubscriptionState`, runtime-ошибки воркера) принимаются всегда.
- **Сброс**: `Disconnect`, `Connect`/`reconnect`, auto-reconnect и ошибка отправки очищают все ожидающие id сессии (ошибка отправки — для всех сессий умершего протокола). Успешный `Connected` также сбрасывает остатки старого линка перед отправкой новых запросов. Поэтому «старый» `Connected`/`Values`/`Browse` от заменённого Disconnect+Connect не может изменить новую сессию.
- **Исключение «несколько событий на запрос»**: у команды ровно один request-id-коррелированный ответ; сопутствующие `Log`/`Disconnected` (таймаут, диагностика AddItems) отправляются с id 0. Это сохраняет обработку таймаутного закрытия, не оставляя id в ожидании.
- **add-children**: `AddChildrenJob.in_flight: HashMap<RequestId, String>` (id → node_id). Ответ продвигает job только при точном совпадении id (и node_id); ручной Browse или ответ старой задачи игнорируются.

## Грациозное закрытие OPC UA (жизненный цикл / тесты `ua_lifecycle`)

- **Причина** серверных ошибок `OpcUa_SecureListener_EndSendResponse: SecureChannel N has no transport connection!` + `Error 0x80310000 while sending message`: клиент разрывал TCP/secure-канал, пока сервер ещё отправлял ответы. В `src/bff/ua.rs` было два `abort()` — на `CommandKind::Disconnect` (мгновенно прерывало event-loop воркера прямо посреди запросов, CloseSession/CloseChannel не доходили до сервера) и на таймауте подключения (аборт во время установки канала). Поиск/дискавери уже были грациозными: методы async-opcua `find_servers`, `get_server_endpoints_from_url`, `find_servers_on_network` внутри создают временный канал, добросовестно закрывают его (`close_channel().await` = CloseSecureChannel) и дренируют event-loop до `TransportPollResult::Closed` перед возвратом, поэтому дроп одноразового `Client` не держит открытых соединений.
- **Исправление** (только `src/bff/ua.rs`): добавлен бюджет `GRACEFUL_CLOSE_TIMEOUT = 5 c`. `Disconnect` теперь: 1) `teardown_subscription` (удаление подписки), 2) `session.session.disconnect().await` в таймауте — шлёт CloseSession и CloseSecureChannel и ждёт состояния Disconnected, 3) ожидает `JoinHandle` event-loop в том же таймауте (он завершается сам после грациозного закрытия), 4) `abort()` — только если грациозное закрытие/ожидание не уложились. Путь таймаута подключения теперь тоже делает грациозный `disconnect()` + ожидание event-loop (без `abort()` первым делом).
- **Live-проверка** `tests/ua_lifecycle.rs` (1 тест, `--ignored`): 5× цикл `connect → browse i=85 → AddItems(2259, 2258) → Read → disconnect` с обязательным `Disconnected` каждый цикл + 3× одноразовый поиск `SearchUaServers` (`FindServers` + `GetEndpoints`), всё за один общий воркер; без зависаний. Read проверяет, что батчинг не теряет и не переставляет ключи. После завершения тестового процесса на 4840 нет `ESTABLISHED`-соединений.
- **Наблюдения по сокетам** (CODESYS `CODESYSControlService.exe`, PID на 4840): до фикса запущенный `opcda-egui.exe` держал `ESTABLISHED` на 4840 (признак бага). После фикса, после всех live-тестов, на 4840 остаются только `LISTENING` (сервер) и `TIME_WAIT` от локально закрытых каналов (норма после FIN/ACK, самоликвидируются ~2 мин) — действующих `ESTABLISHED`-соединений нет.
- **Лог CODESYS**: файловый лог OPC UA-стека с `CmpOPCUAStack`/`SecureListener`/`0x80310000` не обнаружился в `C:\ProgramData\CODESYS`, `C:\Program Files\CODESYS*`, `%LOCALAPPDATA%\CODESYS` (там только device.xml/lib-docs/IDE-временные файлы); runtime контроллера пишет журнал через консоль CODESYS/Event-журнал Windows, недоступные для сравнения строк до/после. Поэтому Δ по `0x80310000` замерить не удалось — опираемся на грациозную семантику закрытия + контроль сокетов.
- **Результат live-тестов**: `ua_lifecycle` 1 ✅, `ua_large` 1 ✅, `ua_smoke` 2 ✅, `ua_write` 1 ✅, `ua_search` 2 ✅, `ua_types` 5 ✅, `ua_security` 4 ✅, `ua_subscription` 2 ✅, `ua_refresh_speed` 1 ✅; `cargo build --bin opcda-egui` чисто. `cargo test --lib` 148 ✅, `cargo test --test ui_interactions` 84 ✅.

## Ограничение размера запросов OPC UA и таймауты (chunk budget)

- **Причина A** (лог CODESYS 2026-10-25 15:38): `OpcUa_TcpStream_Flush: Flush no. 5 with 5 max flushes and final flag 0 -> Too many chunks!` + `0x80800000` — один ответ Browse/Read не влезал в `MaxChunkCount = 5` сервера, потому что клиент просил неограниченный объём за один запрос.
- **Константы** (`src/constants.rs`): `BROWSE_MAX_REFERENCES_PER_NODE = 100` (refs на страницу Browse; реальная самая широкая ветка CODESYS `…/Application/Programs/MAIN_PRG` содержит 149 прямых ссылок, поэтому cap 100 гарантированно прогоняет continuation-цикл на живых данных и держит каждый ответ далеко ниже 5 чанков); `READ_MAX_ITEMS_PER_CALL = 100` (`ReadValueId` на один `session.read`); `DISCOVERY_GUARD_TIMEOUT = 20 c` (жёсткий предохранитель для one-shot discovery, см. ниже).
- **Browse**: cap передаётся в `session.browse(..., BROWSE_MAX_REFERENCES_PER_NODE, view)` для обычного обзора и обзора Views; `browse_next`/release-continuation сохранены, поэтому возвращаются ВСЕ узлы (live: 114 прямых ссылок MAIN_PRG при cap 100 → continuation сработал, счётчик совпал). У `TranslateBrowsePathsToNodeIds` параметра max references нет — там только таймаут.
- **Read**: `read_values_chunked` режет `ReadValueId` на пачки по `READ_MAX_ITEMS_PER_CALL`, выполняет их последовательно и склеивает результаты в порядке запроса (`index_of` вместо O(n²) поиска). Тот же helper используется для атрибутов DataType/AccessLevel в `AddItems` (2 запроса на item — иначе 100+ item в одной AddItems снова давали бы один большой Read) и для `Test` (CurrentTime). `RemoveItems` удаляет monitored items пачками по тому же лимиту.
- **Политика таймаута/брошенных ответов** (причина B): каждый запрос выполняется через `timeout_drain`: по истечении `IO_TIMEOUT` (15 c) future НЕ бросается, а ещё до `GRACEFUL_CLOSE_TIMEOUT` (5 c) доводится до ответа, чтобы в полёте не осталось ответа. Если и за это время ответ не пришёл (сервер не отвечает), future бросается — это последний случай, когда ответ мог остаться в полёте, и тогда закрытие идёт как last resort. Сессия помечается `timed_out` и больше не переиспользуется: обработчик удаляет её из карты, делает грациозный `Session::disconnect()` (CloseSession + CloseSecureChannel; флаг delete_subscriptions передаётся в самом CloseSession, отдельного DeleteSubscriptions нет), ждёт event-loop, и только при невыполнении в `GRACEFUL_CLOSE_TIMEOUT = 5 c` вызывает `abort()`. Затем шлются `Error` (+ уже готовый `Values`/`AddItems`/`Wrote`/`Removed`, чтобы запрос UI разрешился) и `Disconnected`.
- **Точки, покрытые политикой**: Browse (+ browse_next/release), BrowseViews, ResolveBrowsePath, Read (любая пачка), Write, Test, AddItems (атрибуты, CreateSubscription, CreateMonitoredItems), RemoveItems (DeleteMonitoredItems пачками), teardown subscription (DeleteSubscription). Обычный `Disconnect` по-прежнему сначала делает `teardown_subscription`, затем грациозный `disconnect()`; `abort()` — только последняя мера.
- **Connect**: внешний `CONNECT_TIMEOUT` вокруг `connect_to_matching_endpoint` при срабатывании не оставляет объекта сессии (её ещё нет в карте); путь `wait_for_connection` при таймауте уже делает грациозный `disconnect()`. Внешний таймаут на самой установке канала — остаточный риск (см. ниже).
- **One-shot discovery** (`FindServers`/`GetServersOnNetwork`/`GetEndpoints`): discovery-клиент строится с `request_timeout(UA_SEARCH_TIMEOUT = 10 c)`, а внешний guard — `DISCOVERY_GUARD_TIMEOUT = 20 c`. Библиотека сама таймаутит запрос, затем `get_endpoints`/`find_servers` закрывает временный канал и дренирует event-loop до `Closed`; внешний guard срабатывает только если завис даже connect. Файлового лога стека CODESYS (`OpcUa_TcpStream_Flush`/`0x80800000`/`0x80310000`) в `C:\ProgramData\CODESYS`, `C:\Program Files\CODESYS*`, `%LOCALAPPDATA%\CODESYS` не найдено (только библиотечная документация CmpOPCUAStack); в Application Event Log за последние 2 часа записей OPC UA-стека нет — Δ по логу недоступна.

### Live-наблюдения `ua_large` (CODESYS 127.0.0.1:4840, None/anon)

- Скан дерева: 46 Browse-команд / 46 веток за ~0.95 с; 482 уникальные переменные; максимум узлов, возвращённых одной командой Browse (со всеми страницами) = 149; крупнейшая ветка `Objects/DeviceSet/CODESYS Control Win V3 x64/Resources/Application/Programs/MAIN_PRG` — 114 прямых items; continuation подтверждён (114 > cap 100).
- `AddItems` 120 items одной командой — ~20 мс; `Read` 120 ключей ОДНОЙ командой (внутри 2 вызова по 100 + 20) — ~21 мс, 120/120 значений, 120 Good, порядок ключей сохранён, приблизительный payload ~13.8 КБ.
- 5 циклов connect → browse i=85 → AddItems 120 → Read 120 → disconnect: каждый Read ~20 мс, 120/120 Good; после процесса на 4840 только `LISTENING` (PID сервера), `TIME_WAIT` от закрытых клиентских сокетов и НИ ОДНОГО `ESTABLISHED` клиента.
- Live-набор после изменения: `ua_smoke` 2 ✅, `ua_write` 1 ✅, `ua_types` 5 ✅, `ua_search` 2 ✅, `ua_security` 4 ✅, `ua_subscription` 2 ✅, `ua_refresh_speed` 1 ✅, `ua_lifecycle` 1 ✅, `ua_large` 1 ✅; `cargo test --lib` 148 ✅; `cargo test --test ui_interactions` 84 ✅; `cargo build --bin opcda-egui` без warning.

## Устранение хардкода (константы, i18n, op-коды)

- **`src/constants.rs`** — единый источник именованных констант (таймауты UA, reconnect cap/дилей, диапазоны опций, размеры UI, длины усечения, browse-капы). Экспортирован из `lib.rs`. Используется в `app.rs`/`ui.rs`/`settings.rs`/`bff/ua.rs`/`bff/da.rs`.
- **Кламп интервала обновления приведён к 1..=60** (было 1..=3600 в `apply_options` при слайдере 1..=60) — поведение согласовано со спецификацией TrackBar и `ui-parity.md`. Реализовано через чистый `clamp_options()` (без сайд-эффекта `settings.save()`), покрыто `clamp_options_enforces_interval_and_timeout_ranges`.
- **`last_ua_url` по умолчанию пустой** (без пресета), соответствуя требованию продукта; обновлены тесты настроек и `Remember`.
- **Качество**: вместо сравнения `row.quality.text == "нет данных"` используются sentinel-коды `Quality::CODE_PLACEHOLDER_NO_DATA/WAITING` + `is_placeholder_no_data()`. Текст маркеров берётся из i18n (`NoData`/`WaitingValue`), отображение не изменилось.
- **Операции**: `bff::model::op` содержит ASCII-константы (`CONNECT`, `BROWSE`, `READ`, `WRITE`, `TEST`, `SEARCH_SERVERS`, `SEARCH_NETWORK`, `DISCOVER`, `BROWSE_VIEWS`, `RESOLVE_PATH`, `UA_RUNTIME`, …). Эмиттеры ошибок в `ua.rs`/`da.rs` и сопоставление в `app.rs` (`operation == op::SEARCH_SERVERS` и т.п.) используют их; `operation_label` мапит их на локализованные `Tr`. Cyrillic-строки `"поиск серверов"/"поиск в сети"` заменены на ASCII `"search_servers"/"search_network"`.
- **Строки UI → i18n**: эндонимы языков — `Lang::endonym()`; суффикс секунд — `Tr::SecondsShortSuffix`; «…на {host}» — `Tr::ServerHostSuffix`; «Поиск OPC UA серверов в сети…» — `Tr::SearchingUaNetwork`; заголовок ConfirmDialog, если не задан — `Tr::ConfirmTitle`. Остались только не-Cyrillic символы (● × ↑ ↓ ∞ …) и псевдонимы bool-данных («да»/«нет»/«是»/«否») — они вынесены в whitelist.
- **Защита от рецидива**: `tests/no_hardcoded_ui_strings.rs` читает `src/ui.rs`/`src/app.rs`, снимает комментарии и `#[cfg(test)]`-блоки (brace-matched, с учётом строк), находит Cyrillic в строковых литералах и падает, если найдено что-то вне whitelist. 2 теста ✅.
- **CSV-заголовки**: легаси `ColumnId::title()/hint()` переименованы в `csv_title()/csv_hint()` с комментарием о каноническом Delphi-формате; отображение использует `title_tr()/hint_tr()`. CSV-импорт/экспорт не менялся (`csv_save_load_roundtrip_preserves_rows` зелёный).
