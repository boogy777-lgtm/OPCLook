# Текущий статус

Базовая версия всех крейтов — Rust 1.99; `rust-toolchain.toml` и оба задания CI закрепляют 1.99.1. Windows i686 workspace all-targets typecheck прошёл на 1.99.1 с test-only пропуском ресурсов и manifest; Windows runtime не проверялся. Статус CI для точного HEAD — в [PR #8 checks](https://github.com/boogy777-lgtm/OPCLook/pull/8/checks).

Рефакторинг по темам A–H и исправления B1–B20 реализованы. GUI и headless CLI используют общий Engine; контракт, backend и локализация выделены в отдельные крейты. Текущая матрица решений, проверок и внешних ограничений — в [refactoring-status.md](refactoring-status.md); исходные замечания сохранены в [refactoring-plan.md](refactoring-plan.md) и [аудите](audit-2026-10-27.md) как история, а не текущий список задач.

Локальный `scripts/check-linux.sh` завершился успешно для кода `c5762583f96635b37bc8924295703fde9bdccc8b` (до миграции Rust последующие изменения касались только документации): GUI, backend, Engine, contract, locale и CLI, включая живой sample UA и два B15 proxy-теста. После очистки кэша GUI повторно прошли all-target check, обычная сборка GUI/CLI и все GUI test targets. Rust 1.95.0 выполнил Windows i686 workspace all-targets typecheck с `--locked --offline` и test-only пропуском GUI-ресурсов и helper manifest; это не выполнение Windows. Подробные результаты — в [матрице](refactoring-status.md).

Windows CI содержит native unit tests. Состояние финального HEAD показывает [PR #8 checks](https://github.com/boogy777-lgtm/OPCLook/pull/8/checks); merge требует зелёных проверок точного HEAD. Физические DA, UAC, привилегированный helper и маршруты требуют внешнего стенда.

Пять сцен до F, после F и после H отрендерены и просмотрены; детали и результаты измерения CPU/аллокаций — в [отчёте](reports/refactor-frame-baseline.md). Живой физический OPC DA, Windows UAC, helper pipe и реальные маршруты требуют внешнего стенда. Поздние unsolicited данные игнорируются при отключении и подключении, но после нового соединения старое unsolicited событие не несёт эпоху и не всегда отличимо от нового.

Следующая работа по теме, каталогу действий и интерфейсу описана в [плане для нового чата](next-ui-command-plan.md); её пункты ещё не реализованы.
