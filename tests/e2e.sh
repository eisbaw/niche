#!/usr/bin/env bash
# Engine smoke test: builds tests/fixtures/site via the flake's mkSite
# entry point and asserts the resulting site has the expected structure.
# The fixture is intentionally tiny so this stays fast; deeper coverage
# lives in cargo unit/integration tests.
set -euo pipefail

cd "$(dirname "$0")/.."
exec nix flake check --extra-experimental-features 'nix-command flakes' "$@"
