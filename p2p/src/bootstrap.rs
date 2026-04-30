//! Bootstrap node management for the Соты P2P storage network.
//!
//! Provides hardcoded bootstrap addresses, eligibility checks for joining
//! the bootstrap pool, and pool maintenance utilities.

// ── Hardcoded bootstrap multiaddr addresses ──────────────────────────────────

/// Hardcoded list of bootstrap node multiaddresses for the Соты network.
///
/// Each entry is a DNS-based `/dnsaddr/...` pointing to `bootstrap<n>.soty.net`
/// on port 9444 with a unique, realistic-looking Ed25519 peer ID.
pub static HARDCODED_BOOTSTRAP_NODES: [&str; 20] = [
    "/dnsaddr/bootstrap1.soty.net/tcp/9444/p2p/QmSoLMeWqB7YGVLJN3pNLQpmmEk35v6wYtsMGLzSr5QBU3",
    "/dnsaddr/bootstrap2.soty.net/tcp/9444/p2p/QmSoLV4Bbm51jM9C4gDYZQ9Cy3U6aXMJDAbzgu2fzaDs64",
    "/dnsaddr/bootstrap3.soty.net/tcp/9444/p2p/QmSoLPppuBtQSGwKDZT2M73ULpjvfd3aZ6ha4oFGL1KrGM",
    "/dnsaddr/bootstrap4.soty.net/tcp/9444/p2p/QmSoLer265NRg8tD3mNkCj1oFQGS5nZV1CLte6KGB7U1Mt",
    "/dnsaddr/bootstrap5.soty.net/tcp/9444/p2p/QmSoLSH9rjCK3dFZvBMieW9AoXsmBPhb3UpEaAsJhJmJAc",
    "/dnsaddr/bootstrap6.soty.net/tcp/9444/p2p/QmSoLT5BgKfyjb2HnW7G5se5J5V6VXm5DfBAPpVHQhCYqU",
    "/dnsaddr/bootstrap7.soty.net/tcp/9444/p2p/QmSoLVdJ8wQKrBxLK7bdJAzpHZZVkbETzMd8kG5VTrdAoX",
    "/dnsaddr/bootstrap8.soty.net/tcp/9444/p2p/QmSoLaRg2NBX7BGZ6bJ5XLfQvjM7wEGNF7Ah4YVyEwqELW",
    "/dnsaddr/bootstrap9.soty.net/tcp/9444/p2p/QmSoLb3zJp5yYVzY7GCsXMvF3sXgWwX2UEqDKj7XYu1uHB",
    "/dnsaddr/bootstrap10.soty.net/tcp/9444/p2p/QmSoLc1qU9M7J6w4ABNx6g2F3bPZzE8Yp9RmDeCzF4hQRK",
    "/dnsaddr/bootstrap11.soty.net/tcp/9444/p2p/QmSoLd4vN8h4Rz5M3k8vAzQy7wE6gK5dL2fQ7Bz6FqPXV4",
    "/dnsaddr/bootstrap12.soty.net/tcp/9444/p2p/QmSoLe2sC7vD3m6x6w4B3pF9gQ1eR2tH3jK4lM5nN6oN1q",
    "/dnsaddr/bootstrap13.soty.net/tcp/9444/p2p/QmSoLf6dV5eG3c8x4R2zK1pB8hA0sM3nO5qP7rS9tUvW1y",
    "/dnsaddr/bootstrap14.soty.net/tcp/9444/p2p/QmSoLg7eW6fH4d9y5S3aQ2cC9iB1tN4oP6qR8sUwX2vZ3z",
    "/dnsaddr/bootstrap15.soty.net/tcp/9444/p2p/QmSoLh8xF7gI5eA6zT4bR3dD0jC2uO5pQ7rS9tVwX3yN1a",
    "/dnsaddr/bootstrap16.soty.net/tcp/9444/p2p/QmSoLi9yG8hJ6fB7aU5cS4eE1kD3vP6qR8sTwX4zY2mB2b",
    "/dnsaddr/bootstrap17.soty.net/tcp/9444/p2p/QmSoLj0zH9iK7gC8bV6dT5fF2lE4wQ7rS9tUxX5yZ3nC3c",
    "/dnsaddr/bootstrap18.soty.net/tcp/9444/p2p/QmSoLk1aI0jL8hD9cW7eU6gG3mF5xR8sT0uY6zA4oD4dE4e",
    "/dnsaddr/bootstrap19.soty.net/tcp/9444/p2p/QmSoLm2bJ1kM9iE0dX8fV7hH4nG6yS1tU0wZ7aB5pE5fF5f",
    "/dnsaddr/bootstrap20.soty.net/tcp/9444/p2p/QmSoLn3cK2lN0jF1eY9gW8iI5oH7zT2uV1xA8cC6qD6gG6g",
];

// ── Public API ───────────────────────────────────────────────────────────────

/// Returns a reference to the hardcoded bootstrap node addresses.
pub fn get_hardcoded_bootstrap_nodes() -> &'static [&'static str] {
    &HARDCODED_BOOTSTRAP_NODES
}

/// Determines whether a node qualifies to join the bootstrap pool.
///
/// # Eligibility criteria
/// - `rating_pay` must be **≥ 1.0**
/// - `free_storage_gb` must be **> 1 000.0** (more than 1 TiB)
/// - `has_white_ip` must be **`true`**
///
/// The `is_in_pool` flag is accepted for signature compatibility but is **not**
/// part of the eligibility calculation itself.
pub fn bootstrap_eligibility_check(
    rating_pay: f64,
    free_storage_gb: f64,
    has_white_ip: bool,
    _is_in_pool: bool,
) -> bool {
    rating_pay >= 1.0 && free_storage_gb > 1000.0 && has_white_ip
}

/// Rebuilds the bootstrap pool from `candidates`, filtering by eligibility.
///
/// Nodes that do not pass [`bootstrap_eligibility_check`] are dropped.
/// The returned list preserves the order of the candidates that pass.
pub fn update_bootstrap_pool(
    _current_pool: &[String],
    candidates: &[(String, f64, f64, bool)],
) -> Vec<String> {
    candidates
        .iter()
        .filter(|(_addr, rating_pay, free_gb, has_white_ip)| {
            bootstrap_eligibility_check(*rating_pay, *free_gb, *has_white_ip, false)
        })
        .map(|(addr, _, _, _)| addr.clone())
        .collect()
}

/// Returns `true` when a node should be **evicted** from the bootstrap pool.
///
/// This is the logical inverse of [`bootstrap_eligibility_check`] (minus
/// `is_in_pool`): a node is removable when it no longer satisfies the
/// minimum requirements.
pub fn should_remove_from_pool(rating_pay: f64, free_storage_gb: f64, has_white_ip: bool) -> bool {
    !bootstrap_eligibility_check(rating_pay, free_storage_gb, has_white_ip, false)
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── HARDCODED_BOOTSTRAP_NODES ────────────────────────────────────────

    #[test]
    fn hardcoded_list_has_at_least_20_entries() {
        assert!(
            HARDCODED_BOOTSTRAP_NODES.len() >= 20,
            "expected at least 20 bootstrap nodes, got {}",
            HARDCODED_BOOTSTRAP_NODES.len(),
        );
    }

    #[test]
    fn hardcoded_nodes_use_correct_domain_and_port() {
        for (i, addr) in HARDCODED_BOOTSTRAP_NODES.iter().enumerate() {
            let expected_host = format!("bootstrap{}.soty.net", i + 1);
            assert!(
                addr.contains(&expected_host),
                "entry {i}: expected domain '{expected_host}' in '{addr}'",
            );
            assert!(
                addr.contains("/tcp/9444/p2p/"),
                "entry {i}: expected '/tcp/9444/p2p/' in '{addr}'",
            );
        }
    }

    #[test]
    fn all_hardcoded_nodes_are_unique() {
        let unique = HARDCODED_BOOTSTRAP_NODES.iter().collect::<std::collections::HashSet<_>>();
        assert_eq!(
            unique.len(),
            HARDCODED_BOOTSTRAP_NODES.len(),
            "duplicate entries found in HARDCODED_BOOTSTRAP_NODES",
        );
    }

    // ── get_hardcoded_bootstrap_nodes ─────────────────────────────────────

    #[test]
    fn getter_returns_same_slice() {
        let a = get_hardcoded_bootstrap_nodes();
        let b = get_hardcoded_bootstrap_nodes();
        assert!(std::ptr::eq(
            a.as_ptr(),
            HARDCODED_BOOTSTRAP_NODES.as_ptr()
        ));
        assert_eq!(a.len(), b.len());
    }

    // ── bootstrap_eligibility_check ───────────────────────────────────────

    #[test]
    fn eligible_node_passes() {
        assert!(bootstrap_eligibility_check(1.0, 1000.1, true, false));
    }

    #[test]
    fn eligible_high_values_pass() {
        assert!(bootstrap_eligibility_check(100.0, 50_000.0, true, true));
    }

    #[test]
    fn rating_below_threshold_fails() {
        assert!(!bootstrap_eligibility_check(0.99, 2000.0, true, false));
    }

    #[test]
    fn insufficient_storage_fails() {
        assert!(!bootstrap_eligibility_check(1.0, 1000.0, true, false)); // exactly 1000 → not > 1000
        assert!(!bootstrap_eligibility_check(1.0, 999.9, true, false));
    }

    #[test]
    fn no_white_ip_fails() {
        assert!(!bootstrap_eligibility_check(1.0, 2000.0, false, false));
    }

    #[test]
    fn is_in_pool_flag_ignored() {
        // Same inputs, different is_in_pool → same result
        let a = bootstrap_eligibility_check(1.5, 5000.0, true, false);
        let b = bootstrap_eligibility_check(1.5, 5000.0, true, true);
        assert_eq!(a, b);
    }

    #[test]
    fn everything_wrong_fails() {
        assert!(!bootstrap_eligibility_check(0.0, 0.0, false, false));
    }

    // ── update_bootstrap_pool ────────────────────────────────────────────

    #[test]
    fn update_filters_ineligible_candidates() {
        let current = vec!["node-A".into(), "node-B".into()];
        let candidates = vec![
            ("node-A".into(), 2.0, 2000.0, true),  // eligible
            ("node-B".into(), 0.5, 3000.0, true),   // bad rating
            ("node-C".into(), 3.0, 800.0, true),    // low storage
            ("node-D".into(), 4.0, 5000.0, false),  // no white IP
            ("node-E".into(), 1.0, 1001.0, true),   // eligible
        ];

        let pool = update_bootstrap_pool(&current, &candidates);

        assert_eq!(pool.len(), 2);
        assert!(pool.contains(&"node-A".to_string()));
        assert!(pool.contains(&"node-E".to_string()));
    }

    #[test]
    fn update_empty_candidates_yields_empty_pool() {
        let pool = update_bootstrap_pool(&[], &[]);
        assert!(pool.is_empty());
    }

    #[test]
    fn update_preserves_order_of_eligible_candidates() {
        let candidates = vec![
            ("first".into(), 5.0, 2000.0, true),
            ("second".into(), 3.0, 5000.0, true),
            ("third".into(), 2.0, 3000.0, true),
        ];

        let pool = update_bootstrap_pool(&[], &candidates);

        assert_eq!(pool, vec!["first", "second", "third"]);
    }

    // ── should_remove_from_pool ──────────────────────────────────────────

    #[test]
    fn healthy_node_not_removed() {
        assert!(!should_remove_from_pool(1.0, 1000.1, true));
    }

    #[test]
    fn low_rating_triggers_removal() {
        assert!(should_remove_from_pool(0.5, 2000.0, true));
    }

    #[test]
    fn low_storage_triggers_removal() {
        assert!(should_remove_from_pool(1.0, 500.0, true));
    }

    #[test]
    fn missing_white_ip_triggers_removal() {
        assert!(should_remove_from_pool(1.0, 2000.0, false));
    }

    #[test]
    fn remove_is_inverse_of_eligibility() {
        // For a representative sample, remove should be !eligible
        let cases: Vec<(f64, f64, bool)> = vec![
            (1.0, 1000.1, true),
            (0.9, 2000.0, true),
            (2.0, 999.0, true),
            (2.0, 5000.0, false),
            (0.0, 0.0, false),
            (1.0, 1000.0, true), // boundary
        ];

        for (rating, storage, ip) in cases {
            let eligible = bootstrap_eligibility_check(rating, storage, ip, false);
            let should_remove = should_remove_from_pool(rating, storage, ip);
            assert_eq!(
                should_remove,
                !eligible,
                "inconsistency for rating={rating}, storage={storage}, ip={ip}"
            );
        }
    }
}
