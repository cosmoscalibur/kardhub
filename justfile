default:
    @just --list

setup distro="debian":
    #!/usr/bin/env bash
    set -euo pipefail
    echo "Installing system dependencies..."
    if [ "{{distro}}" = "debian" ]; then
        sudo apt-get update && sudo apt-get install -y libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev libxdo-dev
    elif [ "{{distro}}" = "arch" ]; then
        sudo pacman -S --needed webkit2gtk-4.1 gtk3 libayatana-appindicator xdotool
    else
        echo "Unsupported distro: {{distro}}. Use 'debian' or 'arch'." && exit 1
    fi
    echo "Building workspace..."
    cargo build --workspace
    echo "Installing pre-commit hooks..."
    pip install pre-commit
    pre-commit install
    echo "Setup complete."

build:
    cargo build --workspace

test:
    cargo test --workspace

lint:
    cargo clippy --all-targets -- -D warnings

fmt:
    cargo fmt --check

audit:
    cargo audit

coverage:
    cargo tarpaulin --workspace --out xml --fail-under 60

run:
    cargo run -p kardhub-app
