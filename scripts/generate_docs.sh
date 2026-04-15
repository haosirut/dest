#!/bin/bash
set -euo pipefail

# =============================================================================
# VaultKeeper P2P — Documentation Generation Script
# =============================================================================
# Скрипт для автоматической генерации документации:
#   1. cargo doc --workspace (с полным набором флагов)
#   2. Копирование результатов в выходной каталог
#   3. Генерация отчёта о результатах
# =============================================================================

readonly SCRIPT_NAME="$(basename "$0")"
readonly PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
readonly DEFAULT_OUTPUT_DIR="${PROJECT_ROOT}/docs/output"
readonly REPORT_FILE="${PROJECT_ROOT}/docs_report.txt"

# Цвета
readonly RED='\033[0;31m'
readonly GREEN='\033[0;32m'
readonly YELLOW='\033[1;33m'
readonly CYAN='\033[0;36m'
readonly NC='\033[0m'

# Разрешение использования пользовательского каталога вывода
OUTPUT_DIR="${1:-$DEFAULT_OUTPUT_DIR}"

# -----------------------------------------------------------------------------
# Вспомогательные функции
# -----------------------------------------------------------------------------

log_info()  { echo -e "${CYAN}[INFO]${NC} $*"; }
log_pass()  { echo -e "${GREEN}[PASS]${NC} $*"; }
log_fail()  { echo -e "${RED}[FAIL]${NC} $*"; }
log_warn()  { echo -e "${YELLOW}[WARN]${NC} $*"; }

log_section() {
    echo ""
    echo -e "${CYAN}════════════════════════════════════════════${NC}"
    echo -e "${CYAN}  $*${NC}"
    echo -e "${CYAN}════════════════════════════════════════════${NC}"
    echo ""
}

# -----------------------------------------------------------------------------
# Предварительные проверки
# -----------------------------------------------------------------------------

pre_checks() {
    log_section "Предварительные проверки"

    # Проверка Rust toolchain
    if ! command -v rustdoc &>/dev/null; then
        log_fail "rustdoc не найден. Убедитесь, что Rust установлен."
        exit 1
    fi
    log_pass "rustdoc: $(rustdoc --version)"

    # Проверка cargo
    if ! command -v cargo &>/dev/null; then
        log_fail "cargo не найден"
        exit 1
    fi
    log_pass "cargo: $(cargo --version)"

    # Проверка наличия Cargo.toml в корне проекта
    if [[ ! -f "${PROJECT_ROOT}/Cargo.toml" ]]; then
        log_fail "Cargo.toml не найден в ${PROJECT_ROOT}"
        exit 1
    fi
    log_pass "Cargo.toml найден"

    # Проверка наличия workspace
    local is_workspace
    is_workspace=$(rg -c '^\[workspace\]' "${PROJECT_ROOT}/Cargo.toml" 2>/dev/null || echo "0")
    if [[ "$is_workspace" -gt 0 ]]; then
        log_pass "Workspace обнаружен"
    else
        log_info "Отдельный проект (не workspace)"
    fi

    # Создание выходного каталога
    mkdir -p "$OUTPUT_DIR"
    log_pass "Выходной каталог: ${OUTPUT_DIR}"
}

# -----------------------------------------------------------------------------
# Генерация документации через cargo doc
# -----------------------------------------------------------------------------

generate_docs() {
    log_section "Генерация документации (cargo doc --workspace)"

    local doc_start
    doc_start=$(date +%s)

    local cargo_doc_args=(
        --workspace
        --no-deps
        --document-private-items
        --all-features
    )

    log_info "Запуск cargo doc ${cargo_doc_args[*]}..."

    if RUSTDOCFLAGS="--enable-index-page -Z unstable-options" \
       cargo doc "${cargo_doc_args[@]}" 2>&1; then
        local doc_end
        doc_end=$(date +%s)
        local doc_duration=$((doc_end - doc_start))
        log_pass "Документация сгенерирована за ${doc_duration} сек."
        return 0
    else
        log_fail "Ошибка генерации документации"
        return 1
    fi
}

# -----------------------------------------------------------------------------
# Копирование документации в выходной каталог
# -----------------------------------------------------------------------------

copy_docs() {
    log_section "Копирование документации в ${OUTPUT_DIR}"

    local source_dir="${PROJECT_ROOT}/target/doc"

    if [[ ! -d "$source_dir" ]]; then
        log_fail "Каталог с документацией не найден: ${source_dir}"
        return 1
    fi

    # Очистка выходного каталога (сохраняем .gitkeep если есть)
    if [[ -d "${OUTPUT_DIR}" ]]; then
        log_info "Очистка выходного каталога..."
        find "$OUTPUT_DIR" -mindepth 1 -not -name '.gitkeep' -delete 2>/dev/null || true
    fi

    # Копирование
    log_info "Копирование файлов из ${source_dir}..."
    if cp -r "$source_dir"/* "$OUTPUT_DIR"/ 2>&1; then
        log_pass "Документация скопирована успешно"
    else
        log_fail "Ошибка копирования документации"
        return 1
    fi

    # Подсчёт результатов
    local html_count=0
    local css_count=0
    local js_count=0
    local total_size=0

    html_count=$(find "$OUTPUT_DIR" -name "*.html" 2>/dev/null | wc -l | tr -d ' ')
    css_count=$(find "$OUTPUT_DIR" -name "*.css" 2>/dev/null | wc -l | tr -d ' ')
    js_count=$(find "$OUTPUT_DIR" -name "*.js" 2>/dev/null | wc -l | tr -d ' ')

    log_info "Статистика документации:"
    log_info "  HTML-файлов:      ${html_count}"
    log_info "  CSS-файлов:       ${css_count}"
    log_info "  JS-файлов:        ${js_count}"

    # Размер выходного каталога
    if command -v du &>/dev/null; then
        total_size=$(du -sh "$OUTPUT_DIR" 2>/dev/null | cut -f1)
        log_info "  Общий размер:      ${total_size}"
    fi

    # Проверка наличия index.html
    if [[ -f "${OUTPUT_DIR}/index.html" ]]; then
        log_pass "index.html присутствует"
    elif [[ -f "${OUTPUT_DIR}/doc/index.html" ]]; then
        log_pass "doc/index.html присутствует"
    else
        log_warn "index.html не найден в выходном каталоге"
    fi

    # Проверка наличию страницы-индекса workspace (если unstable-options сработал)
    if [[ -f "${OUTPUT_DIR}/all.html" ]]; then
        log_pass "Workspace index page (all.html) сгенерирована"
    fi
}

# -----------------------------------------------------------------------------
# Генерация отчёта
# -----------------------------------------------------------------------------

generate_report() {
    log_section "ИТОГОВЫЙ ОТЧЁТ"

    {
        echo "=== VaultKeeper P2P — Отчёт о генерации документации ==="
        echo "Дата: $(date '+%Y-%m-%d %H:%M:%S %Z')"
        echo "Хост: $(hostname)"
        echo "Rust: $(rustc --version 2>/dev/null || echo 'не установлен')"
        echo "Каталог вывода: ${OUTPUT_DIR}"
        echo ""

        if [[ -d "$OUTPUT_DIR" ]]; then
            local html_count=0
            html_count=$(find "$OUTPUT_DIR" -name "*.html" 2>/dev/null | wc -l | tr -d ' ')
            local total_size
            total_size=$(du -sh "$OUTPUT_DIR" 2>/dev/null | cut -f1 || echo "N/A")
            echo "HTML-файлов: ${html_count}"
            echo "Размер: ${total_size}"
            echo ""
            echo "Статус: OK"
        else
            echo "Статус: FAILED (выходной каталог не создан)"
        fi
    } > "$REPORT_FILE"

    log_info "Отчёт сохранён: ${REPORT_FILE}"
}

# -----------------------------------------------------------------------------
# Главная функция
# -----------------------------------------------------------------------------

main() {
    echo ""
    echo -e "${CYAN}╔══════════════════════════════════════════╗${NC}"
    echo -e "${CYAN}║   VaultKeeper P2P — Doc Generation       ║${NC}"
    echo -e "${CYAN}║   $(date '+%Y-%m-%d %H:%M:%S')                   ║${NC}"
    echo -e "${CYAN}╚══════════════════════════════════════════╝${NC}"

    cd "$PROJECT_ROOT"

    pre_checks

    local doc_result=0
    generate_docs || doc_result=1

    if [[ $doc_result -eq 0 ]]; then
        copy_docs || doc_result=1
    fi

    generate_report

    if [[ $doc_result -eq 0 ]]; then
        echo ""
        echo -e "${GREEN}Документация сгенерирована и скопирована успешно.${NC}"
        echo -e "${GREEN}Откройте: ${OUTPUT_DIR}/index.html${NC}"
        exit 0
    else
        echo ""
        echo -e "${RED}Ошибка при генерации документации. Код возврата: 1${NC}"
        exit 1
    fi
}

main "$@"
