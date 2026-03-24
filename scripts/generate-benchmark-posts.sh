#!/usr/bin/env bash
# Generate 300 benchmark posts for profiling build latency.
# Usage: bash scripts/generate-benchmark-posts.sh
set -euo pipefail

CONTENT_DIR="${1:-content}"
NUM_POSTS=300

# --- Tag pool (15 tags) ---
TAGS=(rust nix linux performance testing architecture devops python networking security databases containers ci-cd monitoring tooling)

# --- Topic fragments for titles ---
TOPICS=(
  "Profiling Strategies"
  "Build Pipeline Tuning"
  "Cache Invalidation Patterns"
  "Incremental Compilation"
  "Dependency Resolution"
  "Static Analysis"
  "Memory Layout"
  "Concurrency Primitives"
  "Error Handling"
  "Type System Design"
  "Package Management"
  "Reproducible Builds"
  "Network Protocols"
  "Filesystem Performance"
  "Container Orchestration"
  "Log Aggregation"
  "Service Discovery"
  "Load Balancing"
  "Database Indexing"
  "Query Optimization"
  "Deployment Automation"
  "Secrets Management"
  "TLS Configuration"
  "DNS Resolution"
  "Kernel Tuning"
  "Scheduler Internals"
  "Signal Handling"
  "IPC Mechanisms"
  "Binary Serialization"
  "Configuration Management"
)
NUM_TOPICS=${#TOPICS[@]}

# --- Paragraph pool ---
PARA0="When building large-scale systems, one of the most important considerations is how individual components interact under load. A system that performs well in isolation can degrade rapidly when multiple services compete for shared resources. Understanding these dynamics is essential for reliable production deployments."
PARA1="The relationship between build times and developer productivity is well documented. Every second added to a feedback loop compounds across the team. Investing in build infrastructure pays dividends not just in raw throughput but in developer satisfaction and code quality."
PARA2="Caching is often described as one of the two hard problems in computer science. In practice, the difficulty lies not in implementing a cache but in knowing when to invalidate it. Stale data can cause subtle bugs that are difficult to reproduce and diagnose."
PARA3="Reproducibility is a cornerstone of reliable software. When a build produces different outputs from the same inputs, debugging becomes archaeology. Nix addresses this by treating builds as pure functions, but the mental model takes time to internalize."
PARA4="Monitoring is not just about dashboards. The real value comes from alerting on meaningful signals rather than noisy metrics. A good monitoring setup distinguishes between symptoms and causes, helping operators focus on what matters."
PARA5="Containers changed how we think about deployment, but they introduced their own complexity. Image layering, networking overlays, and storage drivers all have performance characteristics that are easy to ignore until they become bottlenecks."
PARA6="Security is not a feature you bolt on at the end. It must be woven into the architecture from the start. Threat modeling during design is far cheaper than patching vulnerabilities in production."
PARA7="Testing at the integration level catches a class of bugs that unit tests cannot. However, integration tests are slower and more fragile. The right testing strategy balances coverage, speed, and maintenance cost."
PARA8="Performance optimization without measurement is guesswork. Profilers, flame graphs, and tracing tools provide the evidence needed to make informed decisions. Premature optimization remains the root of much unnecessary complexity."
PARA9="The choice of data structure often matters more than the choice of algorithm. A hash map versus a sorted array, a B-tree versus a skip list -- these decisions ripple through the entire system's performance profile."
PARA10="Distributed systems introduce failure modes that do not exist in single-process programs. Network partitions, clock skew, and message reordering all require explicit handling. Pretending the network is reliable leads to data loss."
PARA11="Good error messages are a form of documentation. When something goes wrong, the error should tell the operator what happened, why it matters, and ideally what to do about it. Cryptic errors waste everyone's time."
PARA12="Configuration management is a spectrum from simple environment variables to full-blown orchestration systems. The right choice depends on the deployment complexity and the team's operational maturity."
PARA13="Logging is cheap until it is not. High-throughput services can generate gigabytes of logs per hour. Structured logging with appropriate levels and sampling keeps costs manageable without sacrificing observability."
PARA14="The boundary between library and application is worth thinking about carefully. Libraries should be opinionated about their domain but flexible about their environment. Applications should be the opposite."
PARA15="Refactoring is not a luxury; it is maintenance. Code that is not periodically reshaped accumulates accidental complexity until changes become disproportionately expensive. Regular small refactors prevent large rewrites."
PARA16="Continuous integration is only useful if the pipeline is fast and reliable. A CI system that takes forty minutes to run or fails intermittently teaches developers to ignore it. Speed and stability are non-negotiable."
PARA17="Database migrations are one of the riskiest operations in a production system. They require careful planning, testing against realistic data, and a rollback strategy. Schema changes that seem trivial can lock tables for minutes."
PARA18="Rust's ownership model eliminates entire categories of bugs at compile time. The learning curve is steep, but the payoff is code that is both fast and correct. The borrow checker is a demanding but fair teacher."
PARA19="Networking code is inherently asynchronous. Blocking on I/O wastes resources; polling wastes CPU. Event-driven architectures handle this well but require disciplined management of callback complexity and state machines."
NUM_PARAGRAPHS=20

get_para() {
  case $(( $1 % NUM_PARAGRAPHS )) in
    0) echo "$PARA0";; 1) echo "$PARA1";; 2) echo "$PARA2";; 3) echo "$PARA3";;
    4) echo "$PARA4";; 5) echo "$PARA5";; 6) echo "$PARA6";; 7) echo "$PARA7";;
    8) echo "$PARA8";; 9) echo "$PARA9";; 10) echo "$PARA10";; 11) echo "$PARA11";;
    12) echo "$PARA12";; 13) echo "$PARA13";; 14) echo "$PARA14";; 15) echo "$PARA15";;
    16) echo "$PARA16";; 17) echo "$PARA17";; 18) echo "$PARA18";; 19) echo "$PARA19";;
  esac
}

# --- Date generation (rotate through 2022-01 to 2025-12) ---
make_date() {
  local n=$1
  local month=$(( (n % 12) + 1 ))
  local year=$(( 2022 + (n % 4) ))
  local day=$(( (n % 28) + 1 ))
  printf '%04d-%02d-%02d' "$year" "$month" "$day"
}

# --- Pick tags using modular arithmetic (no subshells) ---
pick_tags() {
  local i=$1
  local count=$2
  local result=""
  local t0=$(( (i * 7) % 15 ))
  local t1=$(( (i * 13 + 3) % 15 ))
  local t2=$(( (i * 19 + 7) % 15 ))
  local t3=$(( (i * 23 + 11) % 15 ))

  # Ensure uniqueness by offsetting collisions
  while (( t1 == t0 )); do t1=$(( (t1 + 1) % 15 )); done
  while (( t2 == t0 || t2 == t1 )); do t2=$(( (t2 + 1) % 15 )); done
  while (( t3 == t0 || t3 == t1 || t3 == t2 )); do t3=$(( (t3 + 1) % 15 )); done

  result="\"${TAGS[$t0]}\" \"${TAGS[$t1]}\""
  if (( count >= 3 )); then result="$result \"${TAGS[$t2]}\""; fi
  if (( count >= 4 )); then result="$result \"${TAGS[$t3]}\""; fi
  echo "$result"
}

echo "Generating $NUM_POSTS benchmark posts in $CONTENT_DIR/ ..."

for (( i=1; i<=NUM_POSTS; i++ )); do
  slug=$(printf 'bench-%04d' "$i")
  dir="$CONTENT_DIR/$slug"
  mkdir -p "$dir"

  # --- meta.nix ---
  date=$(make_date "$i")
  topic_idx=$(( i % NUM_TOPICS ))
  topic="${TOPICS[$topic_idx]}"
  title="Benchmark Post $i: $topic"

  num_tags=$(( (i % 3) + 2 ))  # 2-4 tags
  tag_str=$(pick_tags "$i" "$num_tags")

  cat > "$dir/meta.nix" <<METAEOF
{
  slug = "$slug";
  title = "$title";
  date = "$date";
  tags = [ $tag_str ];
  summary = "Exploring $topic in the context of build performance benchmarking.";
  authors = [ "mpedersen" ];
}
METAEOF

  # --- post.md ---
  num_paras=$(( (i % 4) + 2 ))  # 2-5 paragraphs

  {
    echo "# $title"
    echo ""

    for (( p=0; p<num_paras; p++ )); do
      get_para $(( i * 3 + p * 7 ))
      echo ""
    done

    # ~30% get a code block
    if (( i % 10 < 3 )); then
      code_idx=$(( i % 7 ))
      case $code_idx in
        0) cat <<'CODEEOF'
```rust
fn process_batch(items: &[Item]) -> Result<Summary> {
    let mut total = 0;
    for item in items {
        total += item.weight;
        validate(item)?;
    }
    Ok(Summary { count: items.len(), total })
}
```
CODEEOF
          ;;
        1) cat <<'CODEEOF'
```nix
{ pkgs ? import <nixpkgs> {} }:
pkgs.mkShell {
  buildInputs = with pkgs; [
    rustc cargo clippy
    pkg-config openssl
  ];
}
```
CODEEOF
          ;;
        2) cat <<'CODEEOF'
```python
import time

def benchmark(func, iterations=1000):
    start = time.monotonic()
    for _ in range(iterations):
        func()
    elapsed = time.monotonic() - start
    return elapsed / iterations
```
CODEEOF
          ;;
        3) cat <<'CODEEOF'
```bash
#!/usr/bin/env bash
set -euo pipefail

for dir in content/*/; do
  if [[ -f "$dir/meta.nix" ]]; then
    echo "Processing: $dir"
    nix eval --file "$dir/meta.nix" slug
  fi
done
```
CODEEOF
          ;;
        4) cat <<'CODEEOF'
```sql
SELECT p.slug, COUNT(t.name) AS tag_count
FROM posts p
JOIN post_tags pt ON p.id = pt.post_id
JOIN tags t ON pt.tag_id = t.id
GROUP BY p.slug
HAVING COUNT(t.name) > 2
ORDER BY tag_count DESC;
```
CODEEOF
          ;;
        5) cat <<'CODEEOF'
```yaml
services:
  app:
    build: .
    ports:
      - "8080:8080"
    environment:
      DATABASE_URL: postgres://db:5432/app
    depends_on:
      - db
  db:
    image: postgres:15
    volumes:
      - pgdata:/var/lib/postgresql/data
```
CODEEOF
          ;;
        6) cat <<'CODEEOF'
```go
func (s *Server) handleRequest(w http.ResponseWriter, r *http.Request) {
    ctx, cancel := context.WithTimeout(r.Context(), 5*time.Second)
    defer cancel()

    result, err := s.store.Query(ctx, r.URL.Query().Get("q"))
    if err != nil {
        http.Error(w, err.Error(), http.StatusInternalServerError)
        return
    }
    json.NewEncoder(w).Encode(result)
}
```
CODEEOF
          ;;
      esac
      echo ""
      echo "The above snippet illustrates a common pattern encountered during benchmarking."
      echo ""
    fi

    # ~10% get a wiki-link
    if (( i % 10 == 0 )); then
      link_target=$(( (i + 42) % NUM_POSTS + 1 ))
      link_slug=$(printf 'bench-%04d' "$link_target")
      echo "For related discussion, see [[${link_slug}]]."
      echo ""
    fi

  } > "$dir/post.md"

  # Progress indicator every 50 posts
  if (( i % 50 == 0 )); then
    echo "  ... $i / $NUM_POSTS"
  fi
done

echo "Done. Generated $NUM_POSTS posts in $CONTENT_DIR/bench-*/"
