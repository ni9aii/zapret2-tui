# Security Policy

*(English below — Русский ниже)*

## English

### Supported versions

`zapret2-tui` is pre-1.0 software. Security fixes are applied to the latest
release and to the `develop` branch. Older tags are not maintained.

### Reporting a vulnerability

Please report security issues **privately**, not in public issues:

- Preferred: open a [GitHub private security advisory](https://github.com/ni9aii/zapret2-tui/security/advisories/new).
- Alternatively: email `ni9aii@proton.me`.

Include the affected version/commit, reproduction steps, and the impact you
observed. We aim to acknowledge reports within 7 days and to ship a fix or a
mitigation plan within 30 days, depending on severity.

Please give us a reasonable window to release a fix before any public
disclosure.

### Security model

`zapret2-tui` itself runs **unprivileged**. Root-only work (nftables rules and
the nfqws2 daemon) is performed through a small, separate privileged helper:

- The helper binary `zapret2-helper` is the only component that runs as root,
  invoked via `pkexec` under a polkit policy.
- All nfqws2 options pass through an allowlist (`zapret2_core::validation`)
  before they reach the daemon, so the privileged path never executes
  arbitrary arguments.
- Profile names are validated for path-safety before any filesystem write.
- The TUI and the helper share the exact same audited action code path
  (`zapret2_core::actions`); there is a single source of truth for the
  nftables script and the argument allowlist.

When reviewing or reporting, the highest-value areas are: the privileged helper
(`crates/zapret2-helper`), the privilege executors and argument allowlist
(`crates/zapret2-core/src/privilege.rs`, `validation.rs`), and the polkit policy
(`packaging/polkit/`).

---

## Русский

### Поддерживаемые версии

`zapret2-tui` — это до-релизное ПО (pre-1.0). Исправления безопасности
применяются к последнему релизу и ветке `develop`. Старые теги не
поддерживаются.

### Как сообщить об уязвимости

Сообщайте о проблемах безопасности **приватно**, не в публичных issue:

- Предпочтительно: создайте [приватный security advisory на GitHub](https://github.com/ni9aii/zapret2-tui/security/advisories/new).
- Либо: на почту `ni9aii@proton.me`.

Укажите затронутую версию/коммит, шаги воспроизведения и наблюдаемое влияние.
Мы стараемся подтвердить получение в течение 7 дней и выпустить исправление или
план смягчения в течение 30 дней — в зависимости от серьёзности.

Пожалуйста, дайте разумное время на выпуск исправления до публичного раскрытия.

### Модель безопасности

Сам `zapret2-tui` работает **без привилегий**. Действия, требующие root
(правила nftables и демон nfqws2), выполняются через небольшой отдельный
привилегированный helper:

- Бинарь `zapret2-helper` — единственный компонент, работающий от root; он
  вызывается через `pkexec` по polkit-политике.
- Все опции nfqws2 проходят через allowlist (`zapret2_core::validation`) до
  передачи демону, поэтому привилегированный путь никогда не исполняет
  произвольные аргументы.
- Имена профилей проверяются на безопасность пути до любой записи на диск.
- TUI и helper используют один и тот же проверенный код действий
  (`zapret2_core::actions`) — единый источник правды для nftables-скрипта и
  allowlist аргументов.

Наиболее важные для аудита места: привилегированный helper
(`crates/zapret2-helper`), исполнители привилегий и allowlist аргументов
(`crates/zapret2-core/src/privilege.rs`, `validation.rs`) и polkit-политика
(`packaging/polkit/`).
