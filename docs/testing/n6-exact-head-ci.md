# N6 — exact-head CI checklist (2026-11-05)

**PENDING.** `main` HEAD `b4bc7245c5a7788c0e8f171f06507242d2a99394`
не содержит незакоммиченные N3/N4/N5/N6 изменения. Поэтому HEAD SHA не
идентифицирует проверенное рабочее дерево. Без отдельной команды не commit/push;
старые зелёные runs не являются приёмкой будущего commit. После разрешённых
commit и push/PR сверить `head_sha` каждого run с точным SHA кода и документов.
Новый push может отменить старый run из-за concurrency; cancelled не PASS.

## Требуемые hosted jobs на том же SHA

Все четыре workflow запускаются на push в `main` или pull request; отдельного
workflow_dispatch в них нет. Публикация/PR требует разрешения владельца.

- [ ] `windows-x86.yml` → `rust` и `linux`: Windows i686 форматирование,
  сборка app/CLI, boundary, COM/VARIANT isolation, shared/backend, library,
  helper и integration; Linux checks и визуальные артефакты. Windows loopback
  probe имеет `continue-on-error`: зелёный job не доказывает его успех, результат
  проверить отдельно. Игнорируемые network tests не превращаются в live.
- [ ] `softing-sdk-api.yml` → `source-probe` (ubuntu/windows) и `core-win32`
  (sdk/official-mit): четыре jobs, проверять оба input и исходные логи.
- [ ] `softing-da-interop.yml` → `local-com` (sdk/official-mit): два jobs;
  проверить ненулевое число тестов, все gates и strict fixture cleanup.
- [ ] `native-preview-package.yml` → `preview-win32`: no-hook bridge, pinned
  official inputs, packaged private CLI и fixture cleanup. Частный preview не
  является release packaging или доказательством готовой SDK-less сборки.

Итого ожидаются 9 job instances; это число jobs, не tests. Сохранить URLs,
SHA, результаты каждого job и важных non-blocking шагов/artifacts.

## Отдельные local/внешние gates

- fmt, `cargo test -j 2`, `cargo test -j 2 -p opclook-backend`; см. status.
- Игнорируемые CODESYS smoke/search/write требуют работающий разрешённый сервер
  и не выполняются автоматически hosted jobs. Полный search включает localhost.
  Известные #12 остаются отдельными тестами/дефектами.
- Нативные local fixture gates требуют Win32 MSVC/ATL, собранные SDK/bridge,
  `SOFTING_RUNTIME_LIB_DIR`, и иногда elevated COM registration; strict cleanup
  обязателен. Предыдущие локальные N3 receipts — не новый run.
- Remote OPCEnum/DCOM/auth, vendor/physical DA, production Async, parity и
  выпуск/SDK-less — отдельные N4 gates; hosted fixture не доказывает их.
  N1.LICENSE пропущен владельцем для N6, не закрыт и не является утверждением
  о правах распространения.
