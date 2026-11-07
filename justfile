default: check

fmt-check:
    cargo fmt --all -- --check

fmt:
    cargo fmt --all

clippy:
    cargo clippy --workspace --all-targets --all-features -- -D warnings

test:
    cargo test --workspace

audit:
    cargo audit

deny:
    cargo deny check

machete:
    cargo machete

check: fmt-check clippy test audit deny machete
