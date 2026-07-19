# zapret2-tui (Русский)

### Что такое zapret2-tui?

Текстовый интерфейс для управления [zapret2](https://github.com/bol-van/zapret2) — набором инструментов для обхода DPI в Linux. Управляйте демоном `nfqws2` и правилами firewall без выхода из терминала.

### Возможности

- **Мониторинг в реальном времени** — Состояние демона и firewall на виду
- **Управление одной клавишей** — Start/stop/restart: `s`, `r`, `q`
- **Управление профилями** — Создание, редактирование, удаление и применение из UI
- **Запуск от обычного пользователя** — Привилегированные действия через
  `pkexec`/polkit; режим `direct` для root и серверов
- **Логи в реальном времени** — Вывод nfqws2
- **Много вкладок** — Status, Profiles, Logs, Settings
- **Безопасные дефолты** — Корректная обработка отсутствующей конфигурации

### Требования

- Linux с **nftables** (бэкенд iptables запланирован; его выбор завершается
  явной ошибкой, а не тихим бездействием) и поддержкой NFQUEUE в ядре
- Rust toolchain (для сборки)
- zapret2 установлен в `/opt/zapret2` (настраивается)
- `pkexec` + polkit для запуска от обычного пользователя (необязательно; режим
  `direct` работает без них)

### Установка

```bash
git clone https://github.com/ni9aii/zapret2-tui
cd zapret2-tui
cargo install --path .
```

Или собрать релизный бинарник:

```bash
cargo build --release
sudo install -m 755 target/release/zapret2-tui /usr/local/bin/
```

### Использование

```bash
# Запуск с конфигом по умолчанию (режим привилегий: auto)
zapret2-tui

# Свой конфиг
zapret2-tui -c /path/to/config

# Режим привилегий: auto (по умолчанию) | pkexec | direct
zapret2-tui --privilege-mode=direct   # root / серверы, без pkexec

# Справка
zapret2-tui --help
```

**Управление:**

| Клавиша | Действие |
|---------|----------|
| `s` | Переключить start/stop |
| `r` | Перезапустить zapret2 |
| `Tab` / `Shift+Tab` | Переключение вкладок |
| `↑` / `↓` | Навигация по профилям |
| `Enter` | Применить выбранный профиль |
| `n` / `e` / `d` | Новый / редактировать / удалить профиль (вкладка Profiles) |
| `h` / `?` | Справка |
| `q` / `Esc` | Выход |

В диалоге: `Tab` / `↑↓` — между полями, `Enter` — подтвердить, `Esc` — отмена.

### Привилегии

Управление firewall и демоном требует root. `zapret2-tui` работает без
привилегий и повышает их на каждое действие:

- `--privilege-mode=auto` (по умолчанию) — напрямую, если уже root, иначе
  `pkexec`.
- `--privilege-mode=pkexec` — всегда через `pkexec` (появляется запрос polkit).
- `--privilege-mode=direct` — без `pkexec`; для root, серверов и минимальных TTY.

`pkexec` вызывает небольшой бинарь `zapret2-helper`. Установите его и polkit-политику
(см. `packaging/README.md`):

```bash
cargo build --release
sudo install -Dm755 target/release/zapret2-helper /usr/libexec/zapret2-helper
sudo install -Dm644 packaging/polkit/io.github.ni9aii.zapret2.policy \
  /usr/share/polkit-1/actions/io.github.ni9aii.zapret2.policy
```

Отмена запроса polkit сообщается отдельно и не меняет состояние.

### Логи

Логи пишутся в файл (не в терминал, которым владеет TUI):

```text
$XDG_STATE_HOME/zapret2-tui/zapret2-tui.log
# или, если не задано: ~/.local/state/zapret2-tui/zapret2-tui.log
```

Уровень задаётся переменной `RUST_LOG` (по умолчанию `info`).

### Конфигурация

Путь к конфигу по умолчанию: `/opt/zapret2/config`

```bash
# Пример /opt/zapret2/config
ZAPRET_BASE=/opt/zapret2
NFQWS2_ENABLE=1
NFQWS2_OPT="--qnum=200 --hostlist=/opt/zapret2/files/youtube.txt"
QNUM=200
FWTYPE=nftables
DESYNC_MARK=0x40000000
```

### Документация

- [`CHANGELOG.md`](CHANGELOG.md) — история релизов
- [`docs/architecture.md`](docs/architecture.md) — крейты и поток данных
- [`docs/privilege-model.md`](docs/privilege-model.md) — pkexec/polkit, режимы
- [`docs/profile-management.md`](docs/profile-management.md) — профили и CRUD

### Архитектура

```
src/main.rs           Точка входа, CLI через clap, логирование в файл
src/app.rs            Состояние приложения, контроллер, обработка клавиш
src/ui.rs             Рендеринг ratatui
src/modal.rs          Состояние модалок профилей + валидация
src/logging.rs        Логирование в файл (XDG state dir)
crates/zapret2-core   Библиотека (config, daemon, firewall, profile,
                      actions, privilege)
crates/zapret2-helper Минимальный привилегированный helper (через pkexec)
packaging/polkit      polkit-политика для helper
```

### Разработка

```bash
# Тесты
cargo test --workspace

# Проверка форматирования
cargo fmt -- --check

# Clippy
cargo clippy --all-targets --workspace -- -D warnings
```

#### Интеграционные тесты (реальный zapret2)

Фича `integration` включает сквозные тесты, которые управляют *реальным*
zapret2: применяют правила nftables, запускают `nfqws2` и затем всё убирают.
Требуют root и собранного `nfqws2` в `$ZAPRET_BASE/nfq2/nfqws2`
(upstream: `cd $ZAPRET_BASE/nfq2 && make`). CI-джоб `integration` собирает
zapret2 из исходников и запускает их на привилегированном раннере. Локально:

```bash
sudo env ZAPRET_BASE=/opt/zapret2 cargo test --features integration \
    --package zapret2-core --test integration_zapret
```

### Лицензия

Двойное лицензирование: MIT ИЛИ Apache-2.0.