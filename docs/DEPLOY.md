# Руководство по развёртыванию SOTY P2P

## Предварительные требования

### Обязательные зависимости

| Компонент | Минимальная версия | Команда проверки |
|-----------|-------------------|-----------------|
| Rust | 1.75.0 | `rustc --version` |
| SQLite | 3.35.0+ | `sqlite3 --version` |
| systemd | 245+ | `systemd --version` |
| GCC / clang | Любая актуальная | `gcc --version` |

### Рекомендуемые зависимости

| Компонент | Назначение |
|-----------|-----------|
| pkg-config | Автоматическое обнаружение библиотек |
| libssl-dev | TLS-поддержка для libp2p |
| cmake | Сборка нативных зависимостей |
| protoc | Сериализация protobuf (для libp2p) |

### Установка Rust (если не установлен)

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
rustup default stable
```

---

## Сборка из исходного кода

### Клонирование репозитория

```bash
git clone https://github.com/soty/soty-p2p.git
cd soty-p2p
```

### Сборка в режиме релиза

```bash
cargo build --release
```

После сборки бинарные файлы будут доступны в `target/release/`:
- `sotyd` — CLI-демон (из крейта `cli`)
- `tauri-app` — GUI-приложение (только при сборке Tauri)

### Сборка с оптимизациями для продакшн

```bash
CARGO_PROFILE_RELEASE_LTO=true \
CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1 \
CARGO_PROFILE_RELEASE_OPT_LEVEL=3 \
cargo build --release
```

Данные параметры обеспечивают максимальную производительность:
- `LTO=true` — межмодульная оптимизация (Link-Time Optimization)
- `codegen-units=1` — единый модуль кодогенерации (лучше оптимизация, дольше сборка)
- `opt-level=3` — максимальный уровень оптимизации

---

## Установка CLI-демона и настройка systemd

### Копирование бинарного файла

```bash
sudo cp target/release/sotyd /usr/local/bin/
sudo chmod 755 /usr/local/bin/sotyd
sotyd --version
```

### Создание пользователя и каталогов

```bash
sudo useradd --system --home-dir /var/lib/soty --create-home soty
sudo mkdir -p /var/lib/soty/data /var/lib/soty/ledger /var/log/soty
sudo chown -R soty:soty /var/lib/soty /var/log/soty
```

### Конфигурационный файл

Создайте файл `/etc/soty/config.toml`:

```toml
[core]
data_dir = "/var/lib/soty/data"
ledger_dir = "/var/lib/soty/ledger"
log_level = "info"

[p2p]
listen_addr = "/ip4/0.0.0.0/tcp/9444"
bootstrap_nodes = [
    "/dns4/bootstrap1.soty.net/tcp/9444/p2p/12D3KooW...",
    "/dns4/bootstrap2.soty.net/tcp/9444/p2p/12D3KooW...",
]
heartbeat_interval_secs = 900

[api]
listen_addr = "127.0.0.1:8080"
enabled = true

[storage]
sandbox_enabled = true
max_shards_per_host = 10000
disk_type = "ssd"
```

### Systemd-юнит

Файл юнита поставляется в репозитории: `cli/sotyd.service`.

```bash
sudo cp cli/sotyd.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable sotyd
sudo systemctl start sotyd
```

### Проверка работы

```bash
# Статус сервиса
sudo systemctl status sotyd

# Логи
sudo journalctl -u sotyd -f

# Проверка API
curl http://127.0.0.1:8080/api/v1/status
```

---

## Параметры конфигурации

### Полный список параметров

| Параметр | По умолчанию | Описание |
|----------|-------------|----------|
| `core.data_dir` | `./data` | Каталог для хранения локальных данных |
| `core.ledger_dir` | `./ledger` | Каталог для базы данных ledger |
| `core.log_level` | `info` | Уровень логирования: trace, debug, info, warn, error |
| `p2p.listen_addr` | `/ip4/0.0.0.0/tcp/9444` | Адрес для входящих P2P-соединений |
| `p2p.bootstrap_nodes` | `[]` | Список bootstrap-узлов для начального подключения |
| `p2p.heartbeat_interval_secs` | `900` | Интервал heartbeat в секундах (15 минут) |
| `p2p.max_peers` | `50` | Максимальное количество одновременных пиров |
| `api.listen_addr` | `127.0.0.1:8080` | Адрес API-сервера |
| `api.enabled` | `true` | Включение/выключение API |
| `storage.sandbox_enabled` | `true` | Включение песочницы (seccomp + cgroups) |
| `storage.max_shards_per_host` | `10000` | Максимальное количество шардов на одном хосте |
| `storage.disk_type` | `hdd` | Тип диска: hdd, ssd, nvme |
| `billing.base_rate` | `0.30` | Базовая ставка (RUB/TiB/час) |
| `billing.cushion_pct` | `25` | Размер подушки в процентах |

---

## Настройка bootstrap-узла

Bootstrap-узел — это точка входа в P2P-сеть. Новые узлы подключаются к bootstrap-узлам для получения начального списка пиров.

### Генерация Peer ID

```bash
sotyd gen-peer-id
```

Команда выведет Peer ID (строка вида `12D3KooW...`), который нужно указать в конфигурации bootstrap-узла.

### Конфигурация bootstrap-узла

```toml
[p2p]
listen_addr = "/ip4/0.0.0.0/tcp/9444"
bootstrap_nodes = []  # Bootstrap-узлы не подключаются к другим bootstrap-узлам
max_peers = 200       # Bootstrap-узлы поддерживают больше пиров
```

### Рекомендации для bootstrap-узлов

- Выделенный сервер с стабильным интернет-соединением
- Статический IP-адрес или DNS-имя
- Работает 24/7 без перезагрузок
- Минимальные аппаратные требования: 1 vCPU, 512 МиБ RAM, 10 ГиБ SSD

---

## Требования к межсетевому экрану (Firewall)

### Обязательные порты

| Порт | Протокол | Назначение | Источник |
|------|----------|-----------|----------|
| 9444 | TCP | P2P-соединения (libp2p) | Любой (0.0.0.0/0) |
| 8080 | TCP | API-сервер (только локальный) | 127.0.0.1 |

### Настройка с UFW

```bash
# Разрешить P2P-порт
sudo ufw allow 9444/tcp comment "SOTY P2P"

# API-сервер — только локальный, не открывать наружу
# sudo ufw allow 8080/tcp  # НЕ ВЫПОЛНЯТЬ для публичного сервера
```

### Настройка с iptables

```bash
# Разрешить P2P-порт
sudo iptables -A INPUT -p tcp --dport 9444 -j ACCEPT

# Сохранить правила
sudo iptables-save > /etc/iptables/rules.v4
```

### Настройка с firewalld (CentOS/RHEL)

```bash
sudo firewall-cmd --permanent --add-port=9444/tcp
sudo firewall-cmd --reload
```

---

## Мониторинг через API

API-сервер доступен на порту 8080 (по умолчанию только на localhost).

### Основные эндпоинты

| Метод | Путь | Описание |
|-------|------|----------|
| GET | `/api/v1/status` | Статус узла (онлайн/офлайн, uptime) |
| GET | `/api/v1/peers` | Список подключённых пиров |
| GET | `/api/v1/storage/usage` | Объём хранимых данных |
| GET | `/api/v1/storage/shards` | Список хранимых шардов |
| GET | `/api/v1/billing/balance` | Текущий баланс |
| GET | `/api/v1/billing/history` | История транзакций |
| GET | `/api/v1/ledger/root` | Текущий Merkle-корень ledger |
| GET | `/api/v1/ledger/sync-status` | Статус синхронизации |

### Пример запросов

```bash
# Статус узла
curl -s http://127.0.0.1:8080/api/v1/status | jq .

# Баланс
curl -s http://127.0.0.1:8080/api/v1/billing/balance | jq .

# Список пиров
curl -s http://127.0.0.1:8080/api/v1/peers | jq .
```

### Интеграция с Prometheus (опционально)

Для продвинутого мониторинга можно использовать экспортёр метрик:

```bash
# Включить метрики в конфигурации
[api]
metrics_enabled = true
metrics_path = "/metrics"
```

Метрики доступны по адресу `http://127.0.0.1:8080/metrics` в формате Prometheus.

---

## Обновление

### Обычное обновление

```bash
# 1. Остановить сервис
sudo systemctl stop sotyd

# 2. Обновить исходный код
cd soty-p2p
git pull origin main

# 3. Пересобрать
cargo build --release

# 4. Заменить бинарный файл
sudo cp target/release/sotyd /usr/local/bin/

# 5. Запустить сервис
sudo systemctl start sotyd

# 6. Проверить
sudo systemctl status sotyd
```

### Миграция базы данных

При обновлении, требующем миграцию схемы SQLite:

```bash
sotyd migrate
```

Команда автоматически создаёт резервную копию базы данных перед миграцией.

### Откат

```bash
# Переключиться на предыдущую версию
git checkout v1.2.3
cargo build --release
sudo systemctl stop sotyd
sudo cp target/release/sotyd /usr/local/bin/
sudo systemctl start sotyd
```
