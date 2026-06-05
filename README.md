# zapret2-tui

**Terminal UI for zapret2 — DPI bypass manager for Linux**

[![CI](https://github.com/ni9aii/zapret2-tui/actions/workflows/ci.yml/badge.svg)](https://github.com/ni9aii/zapret2-tui/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/License-MIT%20OR%20Apache--2.0-blue.svg)](https://github.com/ni9aii/zapret2-tui#license)

---

## English

### What is zapret2-tui?

A terminal-based interface for managing [zapret2](https://github.com/bol-van/zapret2) — a DPI bypass toolkit for Linux. Control the `nfqws2` daemon and firewall rules without leaving your terminal.

### Features

- **Real-time status monitoring** — See daemon and firewall state at a glance
- **One-key control** — Start/stop/restart with `s`, `r`, `q`
- **Log streaming** — Live output from nfqws2
- **Multi-tab interface** — Status, Profiles, Logs, Settings
- **Safe defaults** — Graceful handling of missing/misconfigured state

### Requirements

- Linux (nftables or iptables + NFQUEUE kernel support)
- Rust toolchain (for building)
- zapret2 installed at `/opt/zapret2` (configurable)

### Installation

```bash
git clone https://github.com/ni9aii/zapret2-tui
cd zapret2-tui
cargo install --path .
```

Or build a release binary:

```bash
cargo build --release
sudo install -m 755 target/release/zapret2-tui /usr/local/bin/
```

### Usage

```bash
# Run with default config
zapret2-tui

# Run with custom config
zapret2-tui -c /path/to/config

# Show help
zapret2-tui --help
```

**Controls:**

| Key | Action |
|-----|--------|
| `s` | Toggle start/stop |
| `r` | Restart zapret2 |
| `Tab` / `Shift+Tab` | Switch tabs |
| `q` | Quit |

### Configuration

Default config path: `/opt/zapret2/config`

```bash
# Example /opt/zapret2/config
ZAPRET_BASE=/opt/zapret2
NFQWS2_ENABLE=1
NFQWS2_OPT="--qnum=200 --hostlist=/opt/zapret2/files/youtube.txt"
QNUM=200
FWTYPE=nftables
DESYNC_MARK=0x40000000
```

### Architecture

```
src/main.rs           Entry point, CLI args via clap
src/app.rs            App state, controller integration
src/ui.rs             ratatui rendering
crates/zapret2-core   Core library (config, daemon, firewall, profile)
```

### Development

```bash
# Run tests
cargo test --workspace

# Check formatting
cargo fmt -- --check

# Clippy
cargo clippy --all-targets --workspace -- -D warnings
```

### License

Dual-licensed under MIT OR Apache-2.0.

---

## Русский

### Что такое zapret2-tui?

Текстовый интерфейс для управления [zapret2](https://github.com/bol-van/zapret2) — набором инструментов для обхода DPI в Linux. Управляйте демоном `nfqws2` и правилами firewall без выхода из терминала.

### Возможности

- **Мониторинг в реальном времени** — Состояние демона и firewall на виду
- **Управление одной клавишей** — Start/stop/restart: `s`, `r`, `q`
- **Логи в реальном времени** — Вывод nfqws2
- **Много вкладок** — Status, Profiles, Logs, Settings
- **Безопасные дефолты** — Корректная обработка отсутствующей конфигурации

### Требования

- Linux (nftables или iptables + поддержка NFQUEUE в ядре)
- Rust toolchain (для сборки)
- zapret2 установлен в `/opt/zapret2` (настраивается)

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
# Запуск с конфигом по умолчанию
zapret2-tui

# Свой конфиг
zapret2-tui -c /path/to/config

# Справка
zapret2-tui --help
```

**Управление:**

| Клавиша | Действие |
|---------|----------|
| `s` | Переключить start/stop |
| `r` | Перезапустить zapret2 |
| `Tab` / `Shift+Tab` | Переключение вкладок |
| `q` | Выход |

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

### Архитектура

```
src/main.rs           Точка входа, CLI через clap
src/app.rs            Состояние приложения, контроллер
src/ui.rs             Рендеринг ratatui
crates/zapret2-core   Библиотека (config, daemon, firewall, profile)
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

### Лицензия

Двойное лицензирование: MIT ИЛИ Apache-2.0.