#!/bin/bash
set -euo pipefail

# =============================================================================
# VaultKeeper P2P — Cross-Compilation Script
# =============================================================================
# Скрипт для кросс-компиляции проекта под целевые платформы:
#   Linux (x86_64, aarch64), macOS (x86_64, aarch64), Windows (x86_64)
# =============================================================================

readonly SCRIPT_NAME="$(basename "$0")"
readonly PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
readonly REPORT_FILE="${PROJECT_ROOT}/cross_compile_report.txt"

# Цвета
readonly RED='\033[0;31m'
readonly GREEN='\033[0;32m'
readonly YELLOW='\033[1;33m'
readonly CYAN='\033[0;36m'
readonly NC='\033[0m'

# Счётчики
PASS_COUNT=0
FAIL_COUNT=0
SKIP_COUNT=0
TOTAL_STEPS=0
declare -a RESULTS=()

# Целевые платформы: target|description|linker_required
declare -a TARGETS=(
    "x86_64-unknown-linux-gnu|Linux x86_64|no"
    "x86_64-unknown-linux-musl|Linux x86_64 (static musl)|musl-gcc"
    "aarch64-unknown-linux-gnu|Linux ARM64 (aarch64)|aarch64-linux-gnu-gcc"
    "aarch64-unknown-linux-musl|Linux ARM64 (static musl)|aarch64-linux-musl-gcc"
    "x86_64-apple-darwin|macOS x86_64 (Intel)|no"
    "aarch64-apple-darwin|macOS ARM64 (Apple Silicon)|no"
    "x86_64-pc-windows-gnu|Windows x86_64 (MinGW)|x86_64-w64-mingw32-gcc"
    "x86_64-pc-windows-msvc|Windows x86_64 (MSVC)|no"
)

# -----------------------------------------------------------------------------
# Вспомогательные функции
# -----------------------------------------------------------------------------

log_info()  { echo -e "${CYAN}[INFO]${NC} $*"; }
log_pass()  { echo -e "${GREEN}[PASS]${NC} $*"; }
log_fail()  { echo -e "${RED}[FAIL]${NC} $*"; }
log_skip()  { echo -e "${YELLOW}[SKIP]${NC} $*"; }

log_section() {
    echo ""
    echo -e "${CYAN}════════════════════════════════════════════${NC}"
    echo -e "${CYAN}  $*${NC}"
    echo -e "${CYAN}════════════════════════════════════════════${NC}"
    echo ""
}

record_result() {
    local name="$1" status="$2" message="${3:-}"
    RESULTS+=("${name}|${status}|${message}")
    case "$status" in
        PASS) PASS_COUNT=$((PASS_COUNT + 1)) ;;
        FAIL) FAIL_COUNT=$((FAIL_COUNT + 1)) ;;
        SKIP) SKIP_COUNT=$((SKIP_COUNT + 1)) ;;
    esac
    TOTAL_STEPS=$((TOTAL_STEPS + 1))
}

# -----------------------------------------------------------------------------
# Проверка Rust toolchain
# -----------------------------------------------------------------------------

check_toolchain() {
    log_section "Проверка Rust toolchain"

    if ! command -v rustup &>/dev/null; then
        log_fail "rustup не найден. Установите: https://rustup.rs/"
        record_result "Rust toolchain" "FAIL" "rustup не найден"
        exit 1
    fi

    local rustc_version
    rustc_version=$(rustc --version)
    log_info "Rust: ${rustc_version}"
    record_result "Rust toolchain" "PASS" "${rustc_version}"

    if ! command -v cargo &>/dev/null; then
        log_fail "cargo не найден"
        record_result "Cargo" "FAIL" "cargo не найден"
        exit 1
    fi
    log_pass "cargo доступен"
}

# -----------------------------------------------------------------------------
# Проверка и установка целевых платформ
# -----------------------------------------------------------------------------

install_targets() {
    log_section "Проверка и установка целевых платформ"

    local installed_targets
    installed_targets=$(rustup target list --installed 2>/dev/null)

    for entry in "${TARGETS[@]}"; do
        IFS='|' read -r target description _ <<< "$entry"

        if echo "$installed_targets" | grep -q "^${target}$"; then
            log_pass "Уже установлен: ${target} (${description})"
            record_result "target: ${target}" "PASS" "Уже установлен"
        else
            log_info "Установка: ${target} (${description})..."
            if rustup target add "$target" 2>&1; then
                log_pass "Установлен: ${target}"
                record_result "target: ${target}" "PASS" "Установлен"
            else
                log_fail "Ошибка установки: ${target}"
                record_result "target: ${target}" "FAIL" "Ошибка установки"
            fi
        fi
    done
}

# -----------------------------------------------------------------------------
# Проверка линкеров для кросс-компиляции
# -----------------------------------------------------------------------------

check_linkers() {
    log_section "Проверка кросс-линкеров"

    for entry in "${TARGETS[@]}"; do
        IFS='|' read -r target description linker <<< "$entry"

        if [[ "$linker" == "no" ]]; then
            log_info "Линкер не требуется: ${target}"
            continue
        fi

        if command -v "$linker" &>/dev/null; then
            log_pass "Линкер найден: ${linker} (для ${target})"
        else
            log_fail "Линкер НЕ найден: ${linker} (для ${target})"
            log_info "  Установите через пакетный менеджер системы"
        fi
    done
}

# -----------------------------------------------------------------------------
# Кросс-компиляция для каждой целевой платформы
# -----------------------------------------------------------------------------

build_targets() {
    log_section "Кросс-компиляция"

    local build_start
    build_start=$(date +%s)

    for entry in "${TARGETS[@]}"; do
        IFS='|' read -r target description linker <<< "$entry"

        echo ""
        log_info "────────────────────────────────────────"
        log_info "Целевая платформа: ${target}"
        log_info "Описание: ${description}"
        echo "────────────────────────────────────────"

        # Определение хост-платформы
        local host_target
        host_target=$(rustc -vV 2>/dev/null | sed -n 's/^host: //p')

        # Пропуск сборки для MSVC, если не на Windows
        if [[ "$target" == *"msvc"* && "$host_target" != *"msvc"* ]]; then
            log_skip "Сборка для MSVC требуется на хосте Windows (текущий: ${host_target})"
            record_result "build: ${target}" "SKIP" "Требуется хост Windows"
            continue
        fi

        # Пропуск сборки для macOS, если не на macOS
        if [[ "$target" == *"-apple-darwin"* && "$host_target" != *"-apple-darwin"* ]]; then
            log_skip "Сборка для macOS требуется на хосте macOS (текущий: ${host_target})"
            record_result "build: ${target}" "SKIP" "Требуется хост macOS"
            continue
        fi

        # Определение наличия линкера для GNU-целей
        if [[ "$linker" != "no" ]] && ! command -v "$linker" &>/dev/null; then
            log_skip "Линкер ${linker} не найден — сборка пропущена"
            record_result "build: ${target}" "SKIP" "Линкер не найден: ${linker}"
            continue
        fi

        # Определение переменной окружения для линкера
        local linker_env=""
        case "$target" in
            *-linux-gnu)
                case "$target" in
                    aarch64-*) linker_env="CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=${linker}" ;;
                    x86_64-*)  linker_env="" ;; # Используется системный линкер
                esac
                ;;
            *-linux-musl)
                case "$target" in
                    aarch64-*) linker_env="CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER=${linker}" ;;
                    x86_64-*)  linker_env="CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=${linker}" ;;
                esac
                ;;
            *-windows-gnu)
                linker_env="CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=${linker}"
                ;;
        esac

        # Запуск сборки
        log_info "Запуск cargo build --release --target ${target}..."

        if eval "${linker_env} cargo build --release --target ${target}" 2>&1; then
            # Определение пути к артефакту
            local artifact_path="${PROJECT_ROOT}/target/${target}/release"
            if [[ "$target" == *"windows"* ]]; then
                artifact_path="${artifact_path}/vaultkeeper.exe"
            else
                artifact_path="${artifact_path}/vaultkeeper"
            fi

            local artifact_size="не найден"
            if [[ -f "$artifact_path" ]]; then
                local size_bytes
                size_bytes=$(stat -c%s "$artifact_path" 2>/dev/null || stat -f%z "$artifact_path" 2>/dev/null || echo 0)
                artifact_size="$(numfmt --to=iec --suffix=B "$size_bytes" 2>/dev/null || echo "${size_bytes} байт")"
            fi

            log_pass "Сборка успешна: ${target} (${artifact_size})"
            record_result "build: ${target}" "PASS" "${artifact_size}"
        else
            log_fail "Ошибка сборки: ${target}"
            record_result "build: ${target}" "FAIL" "Ошибка компиляции"
        fi
    done

    local build_end
    build_end=$(date +%s)
    local build_duration=$((build_end - build_start))
    log_info "Общее время сборки: ${build_duration} сек."
}

# -----------------------------------------------------------------------------
# Генерация отчёта
# -----------------------------------------------------------------------------

generate_report() {
    log_section "ИТОГОВЫЙ ОТЧЁТ ПО КРОСС-КОМПИЛЯЦИИ"

    local separator="+--------------------------------------+--------+--------------------------------------+"
    local header="| Целевая платформа                  | Статус | Детали                              |"

    echo "$separator"
    echo "$header"
    echo "$separator"

    for result in "${RESULTS[@]}"; do
        IFS='|' read -r name status message <<< "$result"

        local name_trimmed
        name_trimmed=$(printf "%-36s" "${name:0:36}")
        local msg_trimmed
        msg_trimmed=$(printf "%-36s" "${message:0:36}")

        echo "| ${name_trimmed} |  ${status}  | ${msg_trimmed} |"
    done

    echo "$separator"
    echo ""
    echo "  Итого целей: ${TOTAL_STEPS}"
    echo -e "  ${GREEN}Успешно:${NC}  ${PASS_COUNT}"
    echo -e "  ${RED}С ошибками:${NC} ${FAIL_COUNT}"
    echo -e "  ${YELLOW}Пропущено:${NC} ${SKIP_COUNT}"
    echo ""

    # Запись в файл
    {
        echo "=== VaultKeeper P2P — Отчёт о кросс-компиляции ==="
        echo "Дата: $(date '+%Y-%m-%d %H:%M:%S %Z')"
        echo "Хост: $(hostname)"
        echo "Rust: $(rustc --version 2>/dev/null)"
        echo ""
        echo "Результаты:"
        for result in "${RESULTS[@]}"; do
            IFS='|' read -r name status message <<< "$result"
            echo "  [${status}] ${name}: ${message}"
        done
        echo ""
        echo "Итого: ${PASS_COUNT} PASS, ${FAIL_COUNT} FAIL, ${SKIP_COUNT} SKIP из ${TOTAL_STEPS}"
    } > "$REPORT_FILE"

    log_info "Отчёт сохранён: ${REPORT_FILE}"
}

# -----------------------------------------------------------------------------
# Главная функция
# -----------------------------------------------------------------------------

main() {
    echo ""
    echo -e "${CYAN}╔══════════════════════════════════════════════╗${NC}"
    echo -e "${CYAN}║   VaultKeeper P2P — Cross-Compilation        ║${NC}"
    echo -e "${CYAN}║   $(date '+%Y-%m-%d %H:%M:%S')                     ║${NC}"
    echo -e "${CYAN}╚══════════════════════════════════════════════╝${NC}"

    cd "$PROJECT_ROOT"

    check_toolchain
    install_targets
    check_linkers
    build_targets
    generate_report

    if [[ $FAIL_COUNT -gt 0 ]]; then
        echo -e "${RED}Кросс-компиляция завершена с ошибками. Код возврата: 1${NC}"
        exit 1
    else
        echo -e "${GREEN}Все успешные сборки завершены. Код возврата: 0${NC}"
        exit 0
    fi
}

main "$@"
