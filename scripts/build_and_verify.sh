#!/bin/bash
set -euo pipefail

# =============================================================================
# VaultKeeper P2P — Build and Verify Script
# =============================================================================
# Выполняет полную проверку проекта: форматирование, линтинг, тесты, сборку.
# Выводит итоговый отчёт с результатами каждого этапа (PASS/FAIL).
# =============================================================================

readonly SCRIPT_NAME="$(basename "$0")"
readonly PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
readonly REPORT_FILE="${PROJECT_ROOT}/build_report.txt"

# Цвета для вывода
readonly RED='\033[0;31m'
readonly GREEN='\033[0;32m'
readonly YELLOW='\033[1;33m'
readonly CYAN='\033[0;36m'
readonly NC='\033[0m'

# Счётчики результатов
PASS_COUNT=0
FAIL_COUNT=0
SKIP_COUNT=0
TOTAL_STEPS=0

# Массив для хранения результатов
declare -a RESULTS=()

# -----------------------------------------------------------------------------
# Вспомогательные функции
# -----------------------------------------------------------------------------

log_info() {
    echo -e "${CYAN}[INFO]${NC} $*"
}

log_pass() {
    echo -e "${GREEN}[PASS]${NC} $*"
}

log_fail() {
    echo -e "${RED}[FAIL]${NC} $*"
}

log_skip() {
    echo -e "${YELLOW}[SKIP]${NC} $*"
}

log_section() {
    echo ""
    echo -e "${CYAN}========================================${NC}"
    echo -e "${CYAN}  $*${NC}"
    echo -e "${CYAN}========================================${NC}"
    echo ""
}

record_result() {
    local step_name="$1"
    local status="$2"
    local message="${3:-}"

    RESULTS+=("${step_name}|${status}|${message}")

    case "$status" in
        PASS) PASS_COUNT=$((PASS_COUNT + 1)) ;;
        FAIL) FAIL_COUNT=$((FAIL_COUNT + 1)) ;;
        SKIP) SKIP_COUNT=$((SKIP_COUNT + 1)) ;;
    esac
    TOTAL_STEPS=$((TOTAL_STEPS + 1))
}

# -----------------------------------------------------------------------------
# Проверка зависимостей
# -----------------------------------------------------------------------------

check_dependencies() {
    log_section "ЭТАП 0: Проверка зависимостей"

    # Проверка Rust toolchain
    if command -v rustc &>/dev/null; then
        local rustc_version
        rustc_version=$(rustc --version)
        log_pass "Rust compiler найден: ${rustc_version}"
        record_result "Rust toolchain" "PASS" "${rustc_version}"
    else
        log_fail "Rust compiler не найден. Установите через rustup: https://rustup.rs/"
        record_result "Rust toolchain" "FAIL" "rustc не найден в PATH"
        echo ""
        generate_report
        exit 1
    fi

    # Проверка Cargo
    if command -v cargo &>/dev/null; then
        local cargo_version
        cargo_version=$(cargo --version)
        log_pass "Cargo найден: ${cargo_version}"
        record_result "Cargo" "PASS" "${cargo_version}"
    else
        log_fail "Cargo не найден"
        record_result "Cargo" "FAIL" "cargo не найден в PATH"
        echo ""
        generate_report
        exit 1
    fi

    # Проверка rustfmt
    if command -v rustfmt &>/dev/null; then
        log_pass "rustfmt найден"
        record_result "rustfmt" "PASS"
    else
        log_fail "rustfmt не найден. Установите: rustup component add rustfmt"
        record_result "rustfmt" "FAIL" "rustfmt не найден"
    fi

    # Проверка clippy
    if command -v clippy-driver &>/dev/null; then
        log_pass "clippy найден"
        record_result "clippy" "PASS"
    else
        log_fail "clippy не найден. Установите: rustup component add clippy"
        record_result "clippy" "FAIL" "clippy не найден"
    fi
}

# -----------------------------------------------------------------------------
# Этап 1: Проверка форматирования
# -----------------------------------------------------------------------------

step_fmt_check() {
    log_section "ЭТАП 1: Проверка форматирования (cargo fmt --check)"

    if ! command -v rustfmt &>/dev/null; then
        log_skip "rustfmt не установлен — пропуск проверки форматирования"
        record_result "cargo fmt --check" "SKIP" "rustfmt не установлен"
        return 0
    fi

    log_info "Запуск cargo fmt --check --all..."
    if cargo fmt --check --all 2>&1; then
        log_pass "Форматирование соответствует стандартам"
        record_result "cargo fmt --check" "PASS"
    else
        log_fail "Обнаружены ошибки форматирования. Запустите: cargo fmt --all"
        record_result "cargo fmt --check" "FAIL" "Нарушения форматирования обнаружены"
    fi
}

# -----------------------------------------------------------------------------
# Этап 2: Линтинг (Clippy)
# -----------------------------------------------------------------------------

step_clippy() {
    log_section "ЭТАП 2: Линтинг (cargo clippy)"

    if ! command -v clippy-driver &>/dev/null; then
        log_skip "clippy не установлен — пропуск линтинга"
        record_result "cargo clippy" "SKIP" "clippy не установлен"
        return 0
    fi

    log_info "Запуск cargo clippy --all-targets --all-features -- -D warnings..."
    if cargo clippy --all-targets --all-features -- --deny warnings 2>&1; then
        log_pass "Clippy: предупреждения не обнаружены"
        record_result "cargo clippy" "PASS"
    else
        log_fail "Clippy: обнаружены предупреждения или ошибки"
        record_result "cargo clippy" "FAIL" "Clippy обнаружил проблемы"
    fi
}

# -----------------------------------------------------------------------------
# Этап 3: Тесты
# -----------------------------------------------------------------------------

step_tests() {
    log_section "ЭТАП 3: Запуск тестов (cargo test --workspace)"

    log_info "Запуск cargo test --workspace..."
    if cargo test --workspace 2>&1; then
        log_pass "Все тесты пройдены успешно"
        record_result "cargo test --workspace" "PASS"
    else
        log_fail "Некоторые тесты завершились с ошибкой"
        record_result "cargo test --workspace" "FAIL" "Не все тесты пройдены"
    fi
}

# -----------------------------------------------------------------------------
# Этап 4: Сборка (Release)
# -----------------------------------------------------------------------------

step_build_release() {
    log_section "ЭТАП 4: Сборка Release (cargo build --release)"

    log_info "Запуск cargo build --release..."
    if cargo build --release 2>&1; then
        log_pass "Release-сборка завершена успешно"

        # Информация о собранных артефактах
        local artifacts_dir="${PROJECT_ROOT}/target/release"
        local binary_count=0
        local total_size=0

        if [[ -d "$artifacts_dir" ]]; then
            while IFS= read -r -d '' binary; do
                if file "$binary" | grep -q "ELF\|Mach-O\|PE32"; then
                    binary_count=$((binary_count + 1))
                    local size
                    size=$(stat -c%s "$binary" 2>/dev/null || stat -f%z "$binary" 2>/dev/null || echo 0)
                    total_size=$((total_size + size))
                fi
            done < <(find "$artifacts_dir" -maxdepth 1 -type f -print0 2>/dev/null)
        fi

        log_info "Собрано бинарных файлов: ${binary_count}"
        if [[ $total_size -gt 0 ]]; then
            log_info "Общий размер: $(numfmt --to=iec --suffix=B "$total_size" 2>/dev/null || echo "${total_size} байт")"
        fi

        record_result "cargo build --release" "PASS"
    else
        log_fail "Release-сборка завершилась с ошибкой"
        record_result "cargo build --release" "FAIL" "Ошибка компиляции"
    fi
}

# -----------------------------------------------------------------------------
# Генерация отчёта
# -----------------------------------------------------------------------------

generate_report() {
    log_section "ИТОГОВЫЙ ОТЧЁТ"

    local separator="+------------------------------------------+--------+------------------------------------------+"
    local header="| Этап                                     | Статус | Детали                                   |"

    echo "$separator"
    echo "$header"
    echo "$separator"

    for result in "${RESULTS[@]}"; do
        IFS='|' read -r name status message <<< "$result"

        # Форматирование статуса с цветом
        local status_colored
        case "$status" in
            PASS) status_colored="  PASS  " ;;
            FAIL) status_colored="  FAIL  " ;;
            SKIP) status_colored="  SKIP  " ;;
            *)    status_colored="  ???   " ;;
        esac

        # Обрезка полей до фиксированной ширины
        local name_trimmed
        name_trimmed=$(printf "%-40s" "${name:0:40}")
        local msg_trimmed
        msg_trimmed=$(printf "%-40s" "${message:0:40}")

        echo "| ${name_trimmed} | ${status_colored} | ${msg_trimmed} |"
    done

    echo "$separator"
    echo ""
    echo "  Итого этапов: ${TOTAL_STEPS}"
    echo -e "  ${GREEN}Успешно:${NC}  ${PASS_COUNT}"
    echo -e "  ${RED}С ошибками:${NC} ${FAIL_COUNT}"
    echo -e "  ${YELLOW}Пропущено:${NC} ${SKIP_COUNT}"
    echo ""

    # Запись отчёта в файл
    {
        echo "=== VaultKeeper P2P — Отчёт о сборке ==="
        echo "Дата: $(date '+%Y-%m-%d %H:%M:%S %Z')"
        echo "Хост: $(hostname)"
        echo "Rust: $(rustc --version 2>/dev/null || echo 'не установлен')"
        echo "Cargo: $(cargo --version 2>/dev/null || echo 'не установлен')"
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
    echo -e "${CYAN}╔══════════════════════════════════════════╗${NC}"
    echo -e "${CYAN}║   VaultKeeper P2P — Build & Verify       ║${NC}"
    echo -e "${CYAN}║   $(date '+%Y-%m-%d %H:%M:%S')                   ║${NC}"
    echo -e "${CYAN}╚══════════════════════════════════════════╝${NC}"

    cd "$PROJECT_ROOT"

    # Проверка зависимостей
    check_dependencies

    # Если нет rustfmt или clippy — не прерываемся, просто отмечаем SKIP
    step_fmt_check
    step_clippy
    step_tests
    step_build_release

    # Генерация итогового отчёта
    generate_report

    # Выходной код
    if [[ $FAIL_COUNT -gt 0 ]]; then
        echo -e "${RED}Сборка завершена с ошибками. Код возврата: 1${NC}"
        exit 1
    else
        echo -e "${GREEN}Все проверки пройдены успешно. Код возврата: 0${NC}"
        exit 0
    fi
}

main "$@"
