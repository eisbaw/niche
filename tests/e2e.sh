#!/usr/bin/env bash
# E2E test for the full site build pipeline.
# Runs nix-build site.nix and verifies the output structure and content.
set -euo pipefail

SITE_DIR="result"
PASS=0
FAIL=0

pass() {
    PASS=$((PASS + 1))
    echo "  PASS: $1"
}

fail() {
    FAIL=$((FAIL + 1))
    echo "  FAIL: $1"
}

check_file_exists() {
    local path="$1"
    local label="$2"
    if [ -f "$SITE_DIR/$path" ]; then
        pass "$label exists"
    else
        fail "$label missing at $SITE_DIR/$path"
    fi
}

check_file_contains() {
    local path="$1"
    local pattern="$2"
    local label="$3"
    if [ -f "$SITE_DIR/$path" ] && grep -q "$pattern" "$SITE_DIR/$path"; then
        pass "$label"
    else
        fail "$label (pattern '$pattern' not found in $SITE_DIR/$path)"
    fi
}

echo "=== Building site ==="
nix-build site.nix 2>&1

if [ ! -d "$SITE_DIR" ]; then
    echo "FATAL: nix-build did not produce result/ directory"
    exit 1
fi

echo ""
echo "=== Checking output structure ==="

# Core pages
check_file_exists "index.html" "index.html"
check_file_exists "posts/hello-world/index.html" "hello-world post"
check_file_exists "posts/second-post/index.html" "second-post post"
check_file_exists "archive/index.html" "archive page"
check_file_exists "feed.xml" "RSS feed"
check_file_exists "static/css/main.css" "CSS stylesheet"

# New test posts
check_file_exists "posts/no-tags-post/index.html" "no-tags-post (TASK-0006.02)"
check_file_exists "posts/unicode-test/index.html" "unicode-test post (TASK-0006.04)"
check_file_exists "posts/broken-link-test/index.html" "broken-link-test post (TASK-0006.06)"
check_file_exists "posts/about/index.html" "about page (TASK-0009)"

echo ""
echo "=== Checking content ==="

# Wiki-links resolved: hello-world should link to second-post
check_file_contains "posts/hello-world/index.html" "A Second Post" \
    "wiki-link to second-post resolved in hello-world"
check_file_contains "posts/hello-world/index.html" "/posts/second-post/" \
    "second-post href present in hello-world"

# Site name appears in nav
check_file_contains "index.html" "Nixsite Blog" \
    "site name appears on index page"

# No-tags post renders without error (file existence already checked above)

# Unicode title survives the pipeline
check_file_contains "posts/unicode-test/index.html" "Ownership" \
    "unicode-test title contains 'Ownership'"

# About page has expected content
check_file_contains "posts/about/index.html" "Nix-native static site" \
    "about page contains expected text (TASK-0009)"

# Nav link to about page uses correct URL (slashes are HTML-encoded by the template engine)
check_file_contains "index.html" "posts&#x2F;about" \
    "nav contains link to about page (TASK-0009)"

# Broken wiki-link gets broken-link class
check_file_contains "posts/broken-link-test/index.html" "broken-link" \
    "broken wiki-link has broken-link class (TASK-0006.06)"

# Duplicate slugs: tested by the Nix assertion in site.nix (_slugCheck).
# Cannot be tested in a success-path e2e test. See site.nix comments.

echo ""
echo "=== Results ==="
echo "  Passed: $PASS"
echo "  Failed: $FAIL"

if [ "$FAIL" -gt 0 ]; then
    echo ""
    echo "E2E TESTS FAILED"
    exit 1
fi

echo ""
echo "All e2e tests passed."
