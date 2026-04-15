# Чеклист самопроверки VaultKeeper P2P

Данный документ содержит исчерпывающую карту соответствия: каждое требование спецификации отображается на файл реализации и тест/метод верификации. Используйте этот чеклист для аудита полноты реализации и регрессионного тестирования.

---

## 1. Шифрование

### 1.1 Алгоритм XChaCha20-Poly1305

| Требование | Файл реализации | Тест / Верификация |
|-----------|----------------|-------------------|
| Использование XChaCha20-Poly1305 AEAD | `core/src/encryption.rs` | `tests/unit/core_tests.rs` — тест `test_xchacha20_roundtrip` |
| Ключ 256 бит (32 байта) | `core/src/encryption.rs` — `EncryptionKey::new()` | Проверка длины ключа в тесте `test_key_length` |
| Nonce 192 бит (24 байта) | `core/src/encryption.rs` — `encrypt_chunk()` | Проверка длины nonce в тесте `test_nonce_length` |
| AEAD-тег 128 бит (16 байт) | `core/src/encryption.rs` — `encrypt_chunk()` | Проверка размера зашифрованного вывода: plaintext + 16 |
| Уникальность nonce (CSPRNG) | `core/src/encryption.rs` — `generate_nonce()` | `tests/unit/core_tests.rs` — тест `test_nonce_uniqueness` (10000 nonce без коллизий) |
| Обнаружение подмены данных (AEAD verify) | `core/src/encryption.rs` — `decrypt_chunk()` | `tests/unit/core_tests.rs` — тест `test_tampered_ciphertext_fails` |

### 1.2 Вывод ключа Argon2id

| Требование | Файл реализации | Тест / Верификация |
|-----------|----------------|-------------------|
| Алгоритм Argon2id | `core/src/encryption.rs` — `derive_key()` | Проверка использования `argon2::Argon2::new(argon2::Algorithm::Argon2id, ...)` |
| Память: 64 МиБ (m=65536) | `core/src/encryption.rs` | `tests/unit/core_tests.rs` — тест `test_argon2id_params` |
| Итерации: 3 (t=3) | `core/src/encryption.rs` | `tests/unit/core_tests.rs` — тест `test_argon2id_params` |
| Параллелизм: 4 (p=4) | `core/src/encryption.rs` | `tests/unit/core_tests.rs` — тест `test_argon2id_params` |
| Соль 16 байт (случайная) | `core/src/encryption.rs` | `tests/unit/core_tests.rs` — тест `test_salt_generation` |
| Выходной ключ 32 байта | `core/src/encryption.rs` | `tests/unit/core_tests.rs` — тест `test_derived_key_length` |

---

## 2. Чанкинг

| Требование | Файл реализации | Тест / Верификация |
|-----------|----------------|-------------------|
| Размер чанка: 4 МиБ (4 194 304 байт) | `core/src/chunking.rs` — `CHUNK_SIZE` | `tests/unit/core_tests.rs` — тест `test_chunk_size_exact` |
| Последний чанк дополняется (padding) | `core/src/chunking.rs` — `chunk_file()`, `core/src/padding.rs` | `tests/unit/core_tests.rs` — тест `test_last_chunk_padding` |
| Padding записывается в метаданные | `core/src/chunking.rs` — `ChunkMeta` | `tests/unit/core_tests.rs` — тест `test_padding_metadata` |
| Padding отсекается при восстановлении | `core/src/padding.rs` — `unpad_chunk()` | `tests/unit/core_tests.rs` — тест `test_unpad_roundtrip` |
| Корректное разбиение файлов произвольного размера | `core/src/chunking.rs` — `chunk_file()` | `tests/unit/core_tests.rs` — тесты для 1 байта, 4 МиБ, 4 МиБ + 1 байт, 100 МиБ |

---

## 3. Erasure Coding (Рид-Соломон)

| Требование | Файл реализации | Тест / Верификация |
|-----------|----------------|-------------------|
| Reed-Solomon с параметрами (3, 5) | `core/src/erasure.rs` — `encode()` | `tests/unit/core_tests.rs` — тест `test_erasure_encode_produces_5_shards` |
| 3 шард(а) данных + 2 чётности | `core/src/erasure.rs` | `tests/unit/core_tests.rs` — тест `test_data_parity_split` |
| Восстановление из любых 3 из 5 шардов | `core/src/erasure.rs` — `decode()` | `tests/unit/core_tests.rs` — тест `test_recover_from_any_3` |
| Все комбинации C(5,3)=10 проверены | `tests/unit/core_tests.rs` | `tests/unit/core_tests.rs` — параметрический тест для всех 10 комбинаций |
| Декодирование с 4 и 5 шардами | `core/src/erasure.rs` — `decode()` | `tests/unit/core_tests.rs` — тест `test_recover_from_4`, `test_recover_from_5` |
| Ошибка при <3 шардов | `core/src/erasure.rs` — `decode()` | `tests/unit/core_tests.rs` — тест `test_decode_fails_with_2_shards` |
| Property-based тестирование | — | `tests/property/erasure_property.rs` — 1000 итераций с Proptest |

---

## 4. BIP39 Восстановление

| Требование | Файл реализации | Тест / Верификация |
|-----------|----------------|-------------------|
| 24 слова мнемоники | `core/src/bip39_recovery.rs` — `generate_mnemonic()` | `tests/unit/core_tests.rs` — тест `test_mnemonic_24_words` |
| Энтропия 256 бит | `core/src/bip39_recovery.rs` | `tests/unit/core_tests.rs` — тест `test_entropy_256_bits` |
| Словарь BIP39 (английский) | `core/src/bip39_recovery.rs` | Проверка слов из стандартного словаря BIP39 |
| Контрольная сумма (last word) | `core/src/bip39_recovery.rs` — `validate_mnemonic()` | `tests/unit/core_tests.rs` — тест `test_checksum_validation` |
| Вывод seed через PBKDF2-HMAC-SHA512 | `core/src/bip39_recovery.rs` — `mnemonic_to_seed()` | `tests/unit/core_tests.rs` — тест `test_seed_derivation` |
| Восстановление ключа из мнемоники | `core/src/bip39_recovery.rs` — `recover_key()` | `tests/unit/core_tests.rs` — тест `test_recovery_roundtrip` |
| Обнаружение невалидной мнемоники | `core/src/bip39_recovery.rs` — `validate_mnemonic()` | `tests/unit/core_tests.rs` — тест `test_invalid_mnemonic_rejected` |
| Zeroize мнемоники из памяти | `core/src/bip39_recovery.rs` | Проверка реализации trait `Zeroize` |

---

## 5. Merkle-деревья

| Требование | Файл реализации | Тест / Верификация |
|-----------|----------------|-------------------|
| Построение Merkle-дерева по хешам | `core/src/merkle.rs` — `MerkleTree::new()` | `tests/unit/core_tests.rs` — тест `test_merkle_tree_construction` |
| SHA-256 для хеширования листьев | `core/src/merkle.rs` | `tests/unit/core_tests.rs` — тест `test_merkle_leaf_hash` |
| Вычисление корневого хеша | `core/src/merkle.rs` — `root()` | `tests/unit/core_tests.rs` — тест `test_merkle_root` |
| Audit path (путь от листа к корню) | `core/src/merkle.rs` — `audit_path()` | `tests/unit/core_tests.rs` — тест `test_audit_path` |
| Верификация audit path | `core/src/merkle.rs` — `verify_proof()` | `tests/unit/core_tests.rs` — тест `test_verify_proof` |
| Обнаружение подмены листа | `core/src/merkle.rs` | `tests/unit/core_tests.rs` — тест `test_tampered_leaf_detected` |
| Merkle-дерево для 1, 2, 3, 4+ листьев | `core/src/merkle.rs` | `tests/unit/core_tests.rs` — тесты `test_merkle_odd_leaves`, `test_merkle_single_leaf` |

---

## 6. P2P-слой (libp2p)

### 6.1 Обнаружение узлов (Kademlia)

| Требование | Файл реализации | Тест / Верификация |
|-----------|----------------|-------------------|
| Использование libp2p | `p2p/Cargo.toml` — зависимости | Проверка зависимостей: `libp2p`, `kad` |
| Kademlia DHT для обнаружения | `p2p/src/discovery.rs` | Интеграционный тест подключения двух узлов |
| Bootstrap-узлы в конфигурации | `p2p/src/config.rs`, `cli/src/config.rs` | `tests/unit/core_tests.rs` — парсинг конфигурации |
| Peer ID (ed25519) | `p2p/src/node.rs` | Генерация и верификация Peer ID |

### 6.2 GossipSub

| Требование | Файл реализации | Тест / Верификация |
|-----------|----------------|-------------------|
| GossipSub для распространения сообщений | `p2p/src/gossip.rs` | `tests/unit/core_tests.rs` — тест публикации/подписки |
| Топики: ledger-updates, shard-requests, host-announcements | `p2p/src/gossip.rs` | Проверка определения топиков в коде |
| Ретрансляция gossip-сообщений | `p2p/src/gossip.rs` | Интеграционный тест с 3 узлами |

### 6.3 Heartbeat

| Требование | Файл реализации | Тест / Верификация |
|-----------|----------------|-------------------|
| Интервал: 15 минут (900 секунд) | `p2p/src/heartbeat.rs` — `HEARTBEAT_INTERVAL` | Проверка константы в исходном коде |
| Отправка heartbeat пиру | `p2p/src/heartbeat.rs` — `send_heartbeat()` | Мок-тест отправки heartbeat |
| Обработка ответа heartbeat | `p2p/src/heartbeat.rs` — `handle_heartbeat_response()` | Мок-тест обработки ответа |
| Mark peer as offline при 3 пропущенных heartbeat | `p2p/src/heartbeat.rs` | `tests/unit/core_tests.rs` — тест `test_peer_offline_after_3_missed` |

---

## 7. Proof-of-Storage

| Требование | Файл реализации | Тест / Верификация |
|-----------|----------------|-------------------|
| Merkle-дерево вызовов (challenge) | `p2p/src/challenge.rs` — `create_challenge()` | `tests/unit/core_tests.rs` — тест `proof_of_storage_challenge` |
| Случайный индекс листа (CSPRNG) | `p2p/src/challenge.rs` | Проверка использования `SystemRandom` |
| Таймаут ответа: 5 секунд | `p2p/src/challenge.rs` — `CHALLENGE_TIMEOUT` | Проверка константы |
| Верификация ответа хоста | `p2p/src/challenge.rs` — `verify_challenge_response()` | `tests/unit/core_tests.rs` — тест `test_verify_challenge_response` |
| Пометка узла как ненадёжного (3 неудачи подряд) | `p2p/src/challenge.rs` | `tests/unit/core_tests.rs` — тест `test_host_unreliable_after_3_failures` |

---

## 8. Биллинг

### 8.1 Тарифы и расчёты

| Требование | Файл реализации | Тест / Верификация |
|-----------|----------------|-------------------|
| Базовая ставка: 0.30 RUB/TiB/час | `billing/src/rates.rs` — `BASE_RATE` | `tests/unit/core_tests.rs` — тест `test_base_rate_0_30` |
| Множитель HDD: 1.0 | `billing/src/rates.rs` | `tests/unit/core_tests.rs` — тест `test_hdd_multiplier` |
| Множитель SSD: 1.5 | `billing/src/rates.rs` | `tests/unit/core_tests.rs` — тест `test_ssd_multiplier` |
| Множитель NVMe: 2.0 | `billing/src/rates.rs` | `tests/unit/core_tests.rs` — тест `test_nvme_multiplier` |
| Множители репликации: 1x-4x | `billing/src/rates.rs` | `tests/unit/core_tests.rs` — параметрический тест |
| Подушка: +25% | `billing/src/rates.rs` — `CUSHION_PCT` | `tests/unit/core_tests.rs` — тест `test_cushion_25_pct` |
| Тип `rust_decimal::Decimal` (28 знаков) | `billing/src/types.rs` | `tests/unit/core_tests.rs` — тест `test_decimal_precision` |

### 8.2 Распределение доходов

| Требование | Файл реализации | Тест / Верификация |
|-----------|----------------|-------------------|
| Доля платформы: 10% | `billing/src/calculator.rs` | `tests/unit/core_tests.rs` — тест `test_platform_share_10pct` |
| Доля хостов: 90% | `billing/src/calculator.rs` | `tests/unit/core_tests.rs` — тест `test_host_share_90pct` |
| Пропорциональное распределение между хостами | `billing/src/calculator.rs` | `tests/unit/core_tests.rs` — тест `test_proportional_distribution` |
| Нет потерь при округлении | `billing/src/calculator.rs` | `tests/unit/core_tests.rs` — тест `test_no_rounding_loss` |

### 8.3 Заморозка

| Требование | Файл реализации | Тест / Верификация |
|-----------|----------------|-------------------|
| Баланс ≤ 0 → заморозка | `billing/src/freeze.rs` — `check_freeze()` | `tests/unit/core_tests.rs` — тест `test_freeze_on_zero_balance` |
| Период экспорта: 48 часов | `billing/src/freeze.rs` — `FREEZE_EXPORT_HOURS` | `tests/unit/core_tests.rs` — тест `test_freeze_48h_export_period` |
| Разморозка при пополнении | `billing/src/freeze.rs` — `unfreeze()` | `tests/unit/core_tests.rs` — тест `test_unfreeze_on_deposit` |
| Жёсткое удаление после 48 часов | `billing/src/freeze.rs` — `hard_delete()` | `tests/unit/core_tests.rs` — тест `test_hard_delete_after_48h` |
| Уведомление пользователя при заморозке | `billing/src/freeze.rs` | Мок-тест отправки уведомления |

---

## 9. Ledger (Распределённый реестр)

| Требование | Файл реализации | Тест / Верификация |
|-----------|----------------|-------------------|
| SQLite как хранилище | `ledger/src/store.rs` | `tests/integration/ledger_integration.rs` — тест подключения |
| Схема базы данных | `ledger/src/schema.rs` | `tests/integration/ledger_integration.rs` — миграция schema |
| Gossip-синхронизация | `ledger/src/gossip_sync.rs` | `tests/integration/ledger_integration.rs` — тест синхронизации двух узлов |
| Merkle-корень записи | `ledger/src/store.rs` | `tests/integration/ledger_integration.rs` — тест вычисления корня |
| Разрешение конфликтов: LWW | `ledger/src/conflict.rs` — `resolve_conflict()` | `tests/integration/ledger_integration.rs` — тест `test_lww_resolution` |
| Tiebreaker: меньший хеш | `ledger/src/conflict.rs` | `tests/integration/ledger_integration.rs` — тест `test_hash_tiebreaker` |
| Версионирование записей | `ledger/src/store.rs` | `tests/integration/ledger_integration.rs` — тест версионности |

---

## 10. Подписки

| Требование | Файл реализации | Тест / Верификация |
|-----------|----------------|-------------------|
| Уровень Archive: 0 RUB/мес, 5 GiB | `billing/src/subscription.rs` | `tests/unit/core_tests.rs` — тест `test_archive_tier` |
| Уровень Standard: 199 RUB/мес, 500 GiB | `billing/src/subscription.rs` | `tests/unit/core_tests.rs` — тест `test_standard_tier` |
| Уровень Premium: 499 RUB/мес, 5 TiB | `billing/src/subscription.rs` | `tests/unit/core_tests.rs` — тест `test_premium_tier` |
| Pro-rata перерасчёт при смене уровня | `billing/src/subscription.rs` — `pro_rata_calc()` | `tests/unit/core_tests.rs` — тест `test_pro_rata_upgrade` |
| Точная арифметика в pro-rata | `billing/src/subscription.rs` | `tests/unit/core_tests.rs` — тест `test_pro_rata_decimal_precision` |

---

## 11. Песочница (Sandbox)

| Требование | Файл реализации | Тест / Верификация |
|-----------|----------------|-------------------|
| seccomp-bpf фильтр при запуске хоста | `storage/src/sandbox.rs` — `apply_seccomp()` | `tests/unit/core_tests.rs` — тест `test_seccomp_filter_applied` |
| Белый список системных вызовов | `storage/src/sandbox.rs` | Ревью кода: проверка списка разрешённых syscall |
| Блокировка execve, clone, mount, ptrace | `storage/src/sandbox.rs` | `tests/unit/core_tests.rs` — тест `test_blocked_syscalls` |
| cgroups v2: CPU max 50% | `storage/src/sandbox.rs` — `apply_cgroups()` | `tests/unit/core_tests.rs` — тест `test_cgroup_cpu_limit` |
| cgroups v2: память max 512 МиБ | `storage/src/sandbox.rs` | `tests/unit/core_tests.rs` — тест `test_cgroup_memory_limit` |
| cgroups v2: IOPS max 100 | `storage/src/sandbox.rs` | `tests/unit/core_tests.rs` — тест `test_cgroup_iops_limit` |
| cgroups v2: max 32 процессов | `storage/src/sandbox.rs` | `tests/unit/core_tests.rs` — тест `test_cgroup_pids_limit` |
| Флаг `sandbox_enabled` в конфигурации | `storage/src/sandbox.rs`, `cli/src/config.rs` | Парсинг конфигурации |

---

## 12. Репликация

| Требование | Файл реализации | Тест / Верификация |
|-----------|----------------|-------------------|
| Уровни репликации: 1x, 2x, 3x, 4x | `storage/src/replication.rs` | `tests/unit/core_tests.rs` — параметрический тест |
| 1x: 5 шардов (3 данных + 2 чётности) | `storage/src/replication.rs` | `tests/unit/core_tests.rs` — тест `test_replication_1x_shard_count` |
| 2x: 10 шардов (6 данных + 4 чётности) | `storage/src/replication.rs` | `tests/unit/core_tests.rs` — тест `test_replication_2x_shard_count` |
| Распределение шардов по разным узлам | `storage/src/replication.rs` — `distribute_shards()` | `tests/unit/core_tests.rs` — тест `test_shards_on_different_hosts` |
| Перераспределение при выходе узла | `storage/src/replication.rs` — `rebalance_shards()` | `tests/unit/core_tests.rs` — тест `test_rebalance_on_host_exit` |

---

## Инструкции по использованию чеклиста

### Проведение аудита

1. Для каждой строки проверьте, что файл реализации существует и содержит соответствующий код
2. Убедитесь, что тест существует и проходит (`cargo test`)
3. При отсутствии теста — создайте его и добавьте в соответствующий файл
4. Отметьте проверенные строки в столбце «Статус»

### Регрессионное тестирование

Перед каждым релизом запустите полный набор тестов:

```bash
# Модульные тесты
cargo test --workspace

# Интеграционные тесты
cargo test --test "integration/*"

# Property-based тесты
cargo test --test "property/*"

# С проверкой покрытия
cargo tarpaulin --workspace --out Html
```

### Критерии готовности к релизу

- [ ] Все строки чеклиста проверены (✅)
- [ ] Все тесты проходят (0 failed)
- [ ] Покрытие кода ≥ 80%
- [ ] Property-based тесты: минимум 1000 итераций без сбоев
- [ ] Нет предупреждений компилятора (0 warnings)
