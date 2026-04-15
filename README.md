# VaultKeeper P2P

Децентрализованное P2P-хранилище данных с клиентским шифрованием.

## Архитектура

- **Ядро**: Rust + libp2p
- **Шифрование**: XChaCha20-Poly1305 + Argon2id (клиентское, ключи не покидают устройство)
- **Кодирование**: Reed-Solomon (3/5) erasure coding
- **Дискавери**: Kademlia DHT + 3 bootstrap-узла
- **Леджер**: SQLite + gossip-sync Merkle Ledger (offline-first)
- **Биллинг**: Decimal precision, 10% платформе / 90% хранителям
- **Изоляция**: seccomp + cgroups (Linux), WASM sandbox (Win/macOS)
- **Подписки**: Архив (0 RUB), Стандарт (199 RUB), Премиум (499 RUB)
- **Клиент**: Tauri v2 (Desktop + Mobile) + Headless CLI

## Структура проекта

```
vaultkeeper-p2p/
├── core/           # Шифрование, чанкинг, EC-кодирование, BIP39, Merkle
├── p2p/            # libp2p, Kademlia DHT, GossipSub, heartbeat
├── billing/        # Ежечасный расчёт, 90/10 сплит, freeze/export
├── storage/        # seccomp/cgroups sandbox, shard store, replication
├── ledger/         # SQLite schema, gossip sync, Merkle root, conflict
├── cli/            # Headless daemon (vaultkeeperd), systemd, API
├── tauri/          # Tauri v2 desktop/mobile frontend
├── tests/          # Unit, integration, property-based tests
├── ci/             # GitHub Actions: CI, release, lint
├── docs/           # ARCHITECTURE, SECURITY, BILLING, DEPLOY, etc.
├── legal/          # Оферта, договор хранителя, 54-ФЗ, 152-ФЗ
└── scripts/        # build_and_verify.sh, cross_compile, audit
```

## Быстрый старт

```bash
# Сборка
cargo build --release -p vaultkeeper-cli

# Инициализация узла
./target/release/vaultkeeperd init

# Запуск демона
./target/release/vaultkeeperd start

# Сгенерировать ключи восстановления
./target/release/vaultkeeperd keys generate
```

## Безопасность

- Шифрование ТОЛЬКО на клиенте (XChaCha20-Poly1305)
- Ключи обнуляются в памяти при drop (Zeroize derive)
- Восстановление через BIP39 мнемонику (12/24 слова)
- Хост-узлы изолированы через seccomp (блокировка fork/exec/socket)
- Proof-of-Storage через Merkle tree challenge-response

## Юрисдикция

- РФ: услуга "координация P2P-сети"
- Соответствие 54-ФЗ и 152-ФЗ
- Хранители: самозанятые (НПД) / ИП
- Платежи: ЮKassa + СБП

## Лицензия

AGPL-3.0-or-later
