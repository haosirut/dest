#!/bin/bash
set -euo pipefail

# =============================================================================
# VaultKeeper P2P — Security Audit Script
# =============================================================================
# Скрипт для проведения автоматизированного аудита безопасности:
#   1. cargo audit (проверка зависимостей на известные уязвимости)
#   2. Поиск unsafe-кода в рабочем пространстве
#   3. Проверка лицензий зависимостей
#   4. Поиск жёстко закодированных секретов (hardcoded secrets)
# =============================================================================

readonly SCRIPT_NAME="$(basename "$0")"
readonly PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
readonly REPORT_FILE="${PROJECT_ROOT}/security_audit_report.txt"

# Цвета
readonly RED='\033[0;31m'
readonly GREEN='\033[0;32m'
readonly YELLOW='\033[1;33m'
readonly CYAN='\033[0;36m'
readonly NC='\033[0m'

# Счётчики
VULN_COUNT=0
UNSAFE_COUNT=0
LICENSE_ISSUES=0
SECRET_FINDINGS=0
declare -a FINDINGS=()

# Пороги (WARN/CRIT)
readonly UNSAFE_WARN_THRESHOLD=0
readonly SECRET_WARN_THRESHOLD=0

# -----------------------------------------------------------------------------
# Вспомогательные функции
# -----------------------------------------------------------------------------

log_info()  { echo -e "${CYAN}[INFO]${NC} $*"; }
log_pass()  { echo -e "${GREEN}[PASS]${NC} $*"; }
log_warn()  { echo -e "${YELLOW}[WARN]${NC} $*"; }
log_fail()  { echo -e "${RED}[FAIL]${NC} $*"; }

log_section() {
    echo ""
    echo -e "${CYAN}════════════════════════════════════════════${NC}"
    echo -e "${CYAN}  $*${NC}"
    echo -e "${CYAN}════════════════════════════════════════════${NC}"
    echo ""
}

add_finding() {
    local severity="$1" category="$2" message="$3" file="${4:-}" line="${5:-}"
    local entry="[${severity}] ${category}: ${message}"
    if [[ -n "$file" ]]; then
        entry="${entry} (файл: ${file}"
        if [[ -n "$line" ]]; then
            entry="${entry}:${line}"
        fi
        entry="${entry})"
    fi
    FINDINGS+=("$entry")

    case "$severity" in
        CRITICAL|HIGH) VULN_COUNT=$((VULN_COUNT + 1)) ;;
    esac
}

# -----------------------------------------------------------------------------
# Этап 1: cargo audit
# -----------------------------------------------------------------------------

audit_dependencies() {
    log_section "ЭТАП 1: Проверка зависимостей на уязвимости (cargo audit)"

    if ! command -v cargo-audit &>/dev/null; then
        log_warn "cargo-audit не установлен. Установка..."
        if cargo install cargo-audit 2>&1; then
            log_pass "cargo-audit установлен успешно"
        else
            log_fail "Не удалось установить cargo-audit. Пропуск проверки."
            add_finding "HIGH" "cargo-audit" "Не удалось установить cargo-audit"
            return 1
        fi
    fi

    # Проверка наличия Cargo.lock
    if [[ ! -f "${PROJECT_ROOT}/Cargo.lock" ]]; then
        log_warn "Cargo.lock не найден. Генерация..."
        cargo generate-lockfile 2>&1 || true
    fi

    log_info "Запуск cargo audit..."
    local audit_output
    local audit_exit_code=0

    if audit_output=$(cargo audit --json 2>&1); then
        audit_exit_code=$?
    else
        audit_exit_code=$?
    fi

    # Парсинг JSON-вывода (если доступен jq)
    if command -v jq &>/dev/null; then
        local vuln_count_json
        vuln_count_json=$(echo "$audit_output" | jq '.vulnerabilities.count // 0' 2>/dev/null || echo "unknown")

        if [[ "$vuln_count_json" == "0" ]]; then
            log_pass "Уязвимостей в зависимостях не обнаружено"
        else
            log_fail "Обнаружено уязвимостей: ${vuln_count_json}"

            # Извлечение деталей уязвимостей
            echo "$audit_output" | jq -r '.vulnerabilities.list[]? | "\(.package.name) v\(.package.version): \(.advisory.id) - \(.advisory.title)"' 2>/dev/null | while IFS= read -r vuln; do
                if [[ -n "$vuln" ]]; then
                    log_fail "  → ${vuln}"
                    add_finding "HIGH" "dependency-vuln" "$vuln"
                fi
            done
        fi
    else
        # Fallback: текстовый вывод
        if [[ $audit_exit_code -eq 0 ]]; then
            log_pass "cargo audit: уязвимостей не обнаружено"
        else
            log_fail "cargo audit: обнаружены проблемы"
            add_finding "HIGH" "dependency-vuln" "cargo audit вернул ненулевой код (без jq — детали в текстовом выводе)"
            echo "$audit_output"
        fi
    fi

    return $audit_exit_code
}

# -----------------------------------------------------------------------------
# Этап 2: Поиск unsafe-кода
# -----------------------------------------------------------------------------

check_unsafe_code() {
    log_section "ЭТАП 2: Поиск unsafe-блоков в исходном коде"

    local unsafe_files=()
    local total_unsafe=0

    # Поиск unsafe-блоков во всех .rs-файлах
    while IFS= read -r -d '' rsfile; do
        local file_unsafe=0
        local line_num=0

        while IFS= read -r line; do
            line_num=$((line_num + 1))
            # Игнорируем комментарии
            local trimmed
            trimmed=$(echo "$line" | sed 's|//.*||' | xargs 2>/dev/null || echo "$line")

            if [[ "$trimmed" == *"unsafe"* ]]; then
                # Проверяем, что это именно ключевое слово, а не часть идентификатора
                if echo "$trimmed" | rg -q '\bunsafe\b'; then
                    file_unsafe=$((file_unsafe + 1))
                    total_unsafe=$((total_unsafe + 1))
                    log_warn "  ${rsfile#${PROJECT_ROOT}/}:${line_num}: ${trimmed}"
                    add_finding "MEDIUM" "unsafe-code" "unsafe-код обнаружен" "${rsfile#${PROJECT_ROOT}/}" "$line_num"
                fi
            fi
        done < "$rsfile"

        if [[ $file_unsafe -gt 0 ]]; then
            unsafe_files+=("${rsfile#${PROJECT_ROOT}/} (${file_unsafe} вызовов)")
        fi
    done < <(find "${PROJECT_ROOT}" -name "*.rs" -not -path "*/target/*" -print0 2>/dev/null)

    echo ""
    if [[ $total_unsafe -eq 0 ]]; then
        log_pass "Unsafe-код не обнаружен"
        UNSAFE_COUNT=0
    else
        log_warn "Обнаружено unsafe-вызовов: ${total_unsafe} в ${#unsafe_files[@]} файле(ах)"
        UNSAFE_COUNT=$total_unsafe

        echo "  Файлы с unsafe-кодом:"
        for f in "${unsafe_files[@]}"; do
            echo "    - ${f}"
        done
    fi
}

# -----------------------------------------------------------------------------
# Этап 3: Проверка лицензий зависимостей
# -----------------------------------------------------------------------------

check_licenses() {
    log_section "ЭТАП 3: Проверка лицензий зависимостей"

    local cargo_toml="${PROJECT_ROOT}/Cargo.toml"
    local license_allowed=("MIT" "Apache-2.0" "Apache-2.0 WITH LLVM-exception" "BSD-2-Clause" "BSD-3-Clause" "ISC" "Unicode-DFS-2016" "Zlib" "0BSD" "CC0-1.0" "Unlicense")

    if [[ ! -f "$cargo_toml" ]]; then
        log_warn "Cargo.toml не найден"
        return 1
    fi

    if ! command -v cargo &>/dev/null; then
        log_fail "cargo не найден"
        return 1
    fi

    # Используем cargo tree для получения списка зависимостей
    log_info "Получение списка зависимостей через cargo tree..."
    local deps
    deps=$(cargo tree --depth 1 --prefix none 2>/dev/null | sort -u | sed 's/ .*//' || true)

    if [[ -z "$deps" ]]; then
        log_warn "Не удалось получить список зависимостей"
        return 1
    fi

    local problem_licenses=()

    for dep in $deps; do
        # Пропуск корневого пакета
        if [[ "$dep" == "vaultkeeper"* ]]; then
            continue
        fi

        # Поиск лицензии в файле манифеста зависимости
        local dep_path
        dep_path=$(find "${PROJECT_ROOT}/target/registry/src" -maxdepth 4 -type d -name "$dep" 2>/dev/null | head -1 || true)

        if [[ -n "$dep_path" ]]; then
            local dep_license=""
            # Попытка чтения лицензии из Cargo.toml зависимости
            if [[ -f "${dep_path}/Cargo.toml" ]]; then
                dep_license=$(rg -o '^license\s*=\s*"([^"]+)"' "${dep_path}/Cargo.toml" -r '$1' 2>/dev/null | head -1 || true)
                if [[ -z "$dep_license" ]]; then
                    dep_license=$(rg -o '^license\s*=\s*"([^"]+)"' "${dep_path}/Cargo.toml" -r '$1' 2>/dev/null | head -1 || true)
                fi
            fi

            if [[ -z "$dep_license" ]]; then
                log_warn "  ${dep}: лицензия не указана"
                problem_licenses+=("${dep}: ЛИЦЕНЗИЯ НЕ УКАЗАНА")
                add_finding "LOW" "license" "Лицензия не указана для ${dep}" "$dep"
            else
                local license_ok=false
                for allowed in "${license_allowed[@]}"; do
                    if [[ "$dep_license" == *"$allowed"* ]]; then
                        license_ok=true
                        break
                    fi
                done

                if [[ "$license_ok" == "false" ]]; then
                    log_warn "  ${dep}: лицензия ${dep_license} — требует проверки"
                    problem_licenses+=("${dep}: ${dep_license}")
                    add_finding "MEDIUM" "license" "Нестандартная лицензия: ${dep} -> ${dep_license}" "$dep"
                fi
            fi
        fi
    done

    echo ""
    if [[ ${#problem_licenses[@]} -eq 0 ]]; then
        log_pass "Все зависимости имеют допустимые лицензии"
        LICENSE_ISSUES=0
    else
        log_warn "Обнаружено проблем с лицензиями: ${#problem_licenses[@]}"
        LICENSE_ISSUES=${#problem_licenses[@]}
    fi
}

# -----------------------------------------------------------------------------
# Этап 4: Поиск жёстко закодированных секретов
# -----------------------------------------------------------------------------

check_hardcoded_secrets() {
    log_section "ЭТАП 4: Поиск жёстко закодированных секретов"

    local secret_patterns=(
        'password\s*=\s*"[^"]+"'
        'secret\s*=\s*"[^"]+"'
        'api_key\s*=\s*"[^"]+"'
        'apikey\s*=\s*"[^"]+"'
        'token\s*=\s*"[^"]{20,}"'
        'private_key\s*=\s*"[^"]+"'
        'AKIA[0-9A-Z]{16}'                # AWS Access Key
        'AIza[0-9A-Za-z\-_]{35}'          # Google API Key
        'ghp_[0-9a-zA-Z]{36}'             # GitHub PAT
        'gho_[0-9a-zA-Z]{36}'             # GitHub OAuth
        'glpat-[0-9a-zA-Z\-]{20}'         # GitLab PAT
        'xox[baprs]-[0-9a-zA-Z\-]{10,}'   # Slack tokens
        'sk_live_[0-9a-zA-Z]{24,}'        # Stripe
        'rk_live_[0-9a-zA-Z]{24,}'        # Stripe
        '-----BEGIN (RSA |EC |DSA )?PRIVATE KEY-----'
    )

    local total_findings=0
    local files_to_scan=()

    # Определяем файлы для сканирования (Rust, TOML, YAML, JSON, .env)
    while IFS= read -r -d '' f; do
        files_to_scan+=("$f")
    done < <(find "${PROJECT_ROOT}" \
        \( -name "*.rs" -o -name "*.toml" -o -name "*.yaml" -o -name "*.yml" \
           -o -name "*.json" -o -name "*.env" -o -name "*.sh" \) \
        -not -path "*/target/*" \
        -not -path "*/.git/*" \
        -not -path "*/node_modules/*" \
        -print0 2>/dev/null)

    for pattern in "${secret_patterns[@]}"; do
        for file in "${files_to_scan[@]}"; do
            local matches
            matches=$(rg -n "$pattern" "$file" 2>/dev/null || true)

            if [[ -n "$matches" ]]; then
                while IFS= read -r match; do
                    local match_line
                    match_line=$(echo "$match" | cut -d: -f1)
                    local match_text
                    match_text=$(echo "$match" | cut -d: -f2- | sed 's/\(.\{60\}\).*/\1.../' | xargs 2>/dev/null || echo "$match")

                    log_fail "  ${file#${PROJECT_ROOT}/}:${match_line}: ${match_text}"
                    add_finding "CRITICAL" "hardcoded-secret" "Возможный секрет: ${match_text}" "${file#${PROJECT_ROOT}/}" "$match_line"
                    total_findings=$((total_findings + 1))
                done <<< "$matches"
            fi
        done
    done

    echo ""
    if [[ $total_findings -eq 0 ]]; then
        log_pass "Жёстко закодированные секреты не обнаружены"
        SECRET_FINDINGS=0
    else
        log_fail "Обнаружено возможных секретов: ${total_findings}"
        SECRET_FINDINGS=$total_findings
    fi
}

# -----------------------------------------------------------------------------
# Генерация отчёта
# -----------------------------------------------------------------------------

generate_report() {
    log_section "ИТОГОВЫЙ ОТЧЁТ ПО АУДИТУ БЕЗОПАСНОСТИ"

    echo ""
    echo "  ┌─────────────────────────────────────────────┐"
    echo "  │  Сводка находок                              │"
    echo "  ├─────────────────────────────────────────────┤"
    echo "  │  Уязвимости зависимостей:     ${VULN_COUNT}           │"
    echo "  │  Unsafe-вызовы:               ${UNSAFE_COUNT}           │"
    echo "  │  Проблемы с лицензиями:       ${LICENSE_ISSUES}           │"
    echo "  │  Возможные секреты:           ${SECRET_FINDINGS}           │"
    echo "  │  Всего находок:               ${#FINDINGS[@]}           │"
    echo "  └─────────────────────────────────────────────┘"
    echo ""

    if [[ ${#FINDINGS[@]} -gt 0 ]]; then
        echo "  Детали:"
        for finding in "${FINDINGS[@]}"; do
            echo "    • ${finding}"
        done
        echo ""
    fi

    # Оценка общего результата
    local critical_count=0
    local high_count=0
    for finding in "${FINDINGS[@]}"; do
        if [[ "$finding" == "[CRITICAL]"* ]]; then
            critical_count=$((critical_count + 1))
        elif [[ "$finding" == "[HIGH]"* ]]; then
            high_count=$((high_count + 1))
        fi
    done

    echo "  ┌─────────────────────────────────────────────┐"
    echo "  │  Общая оценка                               │"
    echo "  ├─────────────────────────────────────────────┤"
    if [[ $critical_count -gt 0 ]]; then
        echo -e "  │  Результат: ${RED}КРИТИЧЕСКИЕ ПРОБЛЕМЫ${NC}              │"
    elif [[ $high_count -gt 0 ]]; then
        echo -e "  │  Результат: ${YELLOW}ТРЕБУЕТСЯ ВНИМАНИЕ${NC}              │"
    else
        echo -e "  │  Результат: ${GREEN}АУДИТ ПРОЙДЕН${NC}                    │"
    fi
    echo "  └─────────────────────────────────────────────┘"

    # Сохранение отчёта
    {
        echo "=== VaultKeeper P2P — Отчёт по аудиту безопасности ==="
        echo "Дата: $(date '+%Y-%m-%d %H:%M:%S %Z')"
        echo "Хост: $(hostname)"
        echo ""
        echo "Сводка:"
        echo "  Уязвимости зависимостей: ${VULN_COUNT}"
        echo "  Unsafe-вызовы: ${UNSAFE_COUNT}"
        echo "  Проблемы с лицензиями: ${LICENSE_ISSUES}"
        echo "  Возможные секреты: ${SECRET_FINDINGS}"
        echo "  Всего находок: ${#FINDINGS[@]}"
        echo ""
        echo "Находки:"
        for finding in "${FINDINGS[@]}"; do
            echo "  ${finding}"
        done
    } > "$REPORT_FILE"

    log_info "Отчёт сохранён: ${REPORT_FILE}"
}

# -----------------------------------------------------------------------------
# Главная функция
# -----------------------------------------------------------------------------

main() {
    echo ""
    echo -e "${CYAN}╔══════════════════════════════════════════╗${NC}"
    echo -e "${CYAN}║   VaultKeeper P2P — Security Audit       ║${NC}"
    echo -e "${CYAN}║   $(date '+%Y-%m-%d %H:%M:%S')                   ║${NC}"
    echo -e "${CYAN}╚══════════════════════════════════════════╝${NC}"

    cd "$PROJECT_ROOT"

    # Этап 1: cargo audit
    audit_dependencies || true

    # Этап 2: unsafe-код
    check_unsafe_code

    # Этап 3: лицензии
    check_licenses || true

    # Этап 4: секреты
    check_hardcoded_secrets

    # Генерация отчёта
    generate_report

    # Код возврата
    local has_critical=false
    for finding in "${FINDINGS[@]}"; do
        if [[ "$finding" == "[CRITICAL]"* ]]; then
            has_critical=true
            break
        fi
    done

    if [[ "$has_critical" == "true" ]]; then
        echo -e "${RED}Аудит завершён: обнаружены критические проблемы. Код возврата: 1${NC}"
        exit 1
    else
        echo -e "${GREEN}Аудит завершён без критических проблем. Код возврата: 0${NC}"
        exit 0
    fi
}

main "$@"
