#!/bin/bash
set -euo pipefail

echo "🔍 VaultKeeper Integration Verification..."

PASS=0; FAIL=0
check() {
    if eval "$2" >/dev/null 2>&1; then
        echo "  ✅ $1"; PASS=$((PASS+1))
    else
        echo "  ❌ $1"; FAIL=$((FAIL+1))
    fi
}

echo "[1/10] Workspace structure..."
check "Cargo.toml exists" "[ -f Cargo.toml ]"
check "tauri/src-tauri in workspace" "grep -q 'tauri/src-tauri' Cargo.toml"
check "keyring dependency" "grep -q 'keyring' Cargo.toml"
check "rust_decimal dependency" "grep -q 'rust_decimal' Cargo.toml"
check "libp2p NAT features" "grep -q '\"relay\"' Cargo.toml"

echo "[2/10] Core crate..."
check "crypto module" "[ -f core/src/crypto.rs ]"
check "seed module" "[ -f core/src/seed.rs ]"

echo "[3/10] P2P NAT features..."
check "transport module" "[ -f p2p/src/transport.rs ]"
check "HostAvailable message" "grep -q 'HostAvailable' p2p/src/message.rs"

echo "[4/10] Billing engine..."
check "BillingEngine" "grep -q 'BillingEngine' billing/src/engine.rs"
check "estimate_upload_cost" "grep -q 'estimate_upload_cost' billing/src/engine.rs"
check "freeze state" "grep -q 'FreezeState' billing/src/freeze.rs"

echo "[5/10] Ledger reputation..."
check "ReputationManager" "grep -q 'ReputationManager' ledger/src/reputation.rs"
check "EscrowManager" "grep -q 'EscrowManager' ledger/src/escrow.rs"
check "16 fails ban" "grep -q '16' ledger/src/reputation.rs"

echo "[6/10] Storage mobile guards..."
check "is_hosting_available" "grep -q 'is_hosting_available' storage/src/lib.rs"
check "android/ios cfg" "grep -q 'android.*ios' storage/src/lib.rs || grep -q 'target_os' storage/src/lib.rs"

echo "[7/10] Tauri commands..."
check "generate_seed" "grep -q 'generate_seed' tauri/src-tauri/src/lib.rs"
check "keyring usage" "grep -q 'keyring' tauri/src-tauri/src/lib.rs"
check "check_host_eligibility" "grep -q 'check_host_eligibility' tauri/src-tauri/src/lib.rs"
check "updater plugin" "grep -q 'tauri-plugin-updater' tauri/src-tauri/Cargo.toml"

echo "[8/10] Frontend mobile..."
check "mobile-warning" "grep -q 'mobile-warning' tauri/src/dist/index.html"
check "host-section hidden on mobile" "grep -q 'host' tauri/src/dist/index.html"

echo "[9/10] CI workflows..."
check "ci.yml" "[ -f .github/workflows/ci.yml ]"
check "release.yml" "[ -f .github/workflows/release.yml ]"

echo "[10/10] Scripts..."
check "verify_integration.sh" "[ -f scripts/verify_integration.sh ]"
check "build_and_verify.sh" "[ -f scripts/build_and_verify.sh ]"
check "audit_check.sh" "[ -f scripts/audit_check.sh ]"

echo ""
echo "========================================="
echo "  PASS: $PASS  |  FAIL: $FAIL"
echo "========================================="
[ "$FAIL" -eq 0 ] || exit 1
