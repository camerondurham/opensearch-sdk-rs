#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
CRATE_DIR=$(cd -- "$SCRIPT_DIR/.." && pwd)
OPENSEARCH_DIR=${OPENSEARCH_DIR:-"$CRATE_DIR/../OpenSearch"}
WORK_DIR=${WORK_DIR:-"$CRATE_DIR/.tmp/live-hello"}
LOG_DIR="$WORK_DIR/logs"
RUNTIME_DIR="$WORK_DIR/runtime"
OPENSEARCH_HOME=""
EXTENSION_PID=""
OPENSEARCH_PID=""
PORT_BASE=${PORT_BASE:-$((14000 + RANDOM % 1000))}

EXTENSION_HOST=${EXTENSION_HOST:-127.0.0.1}
EXTENSION_PORT=${EXTENSION_PORT:-$PORT_BASE}
OPENSEARCH_HTTP_PORT=${OPENSEARCH_HTTP_PORT:-$((PORT_BASE + 1))}
OPENSEARCH_TRANSPORT_PORT=${OPENSEARCH_TRANSPORT_PORT:-$((PORT_BASE + 2))}
OPENSEARCH_URL=${OPENSEARCH_URL:-"http://127.0.0.1:${OPENSEARCH_HTTP_PORT}"}
OPENSEARCH_ARCHIVE_DIR=${OPENSEARCH_ARCHIVE_DIR:-"$OPENSEARCH_DIR/distribution/archives/no-jdk-linux-tar/build/distributions"}
OPENSEARCH_GRADLE_TASK=${OPENSEARCH_GRADLE_TASK:-:distribution:archives:no-jdk-linux-tar:assemble}
OPENSEARCH_ADMIN_USER=${OPENSEARCH_ADMIN_USER:-}
OPENSEARCH_ADMIN_PASSWORD=${OPENSEARCH_ADMIN_PASSWORD:-}
OPENSEARCH_CURL_INSECURE=${OPENSEARCH_CURL_INSECURE:-0}
OPENSEARCH_SKIP_BUILD=${OPENSEARCH_SKIP_BUILD:-0}
OPENSEARCH_SKIP_EXTENSION_BUILD=${OPENSEARCH_SKIP_EXTENSION_BUILD:-0}
KEEP_WORKDIR=${KEEP_WORKDIR:-0}
OPENSEARCH_SDK_RS_TRACE=${OPENSEARCH_SDK_RS_TRACE:-1}

export OPENSEARCH_SDK_RS_TRACE

run_with_jdk() {
  if command -v java >/dev/null 2>&1; then
    "$@"
  else
    nix shell nixpkgs#jdk21 --command "$@"
  fi
}

resolve_jdk_home() {
  if [[ -n "${JAVA_HOME:-}" ]]; then
    printf '%s\n' "$JAVA_HOME"
    return 0
  fi

  if command -v java >/dev/null 2>&1; then
    java -XshowSettings:properties -version 2>&1 | sed -n 's/^[[:space:]]*java.home = //p' | head -n 1
    return 0
  fi

  run_with_jdk bash -lc 'java -XshowSettings:properties -version 2>&1 | sed -n "s/^[[:space:]]*java.home = //p" | head -n 1'
}

curl_args() {
  local fail_on_http=${1:-1}
  local args=(-sS --max-time 5)
  if [[ "$fail_on_http" == "1" ]]; then
    args=(-f "${args[@]}")
  fi
  if [[ "$OPENSEARCH_CURL_INSECURE" == "1" ]]; then
    args+=(-k)
  fi
  if [[ -n "$OPENSEARCH_ADMIN_USER" ]]; then
    args+=(-u "${OPENSEARCH_ADMIN_USER}:${OPENSEARCH_ADMIN_PASSWORD}")
  fi
  printf '%s\n' "${args[@]}"
}

dump_logs() {
  if [[ -f "$LOG_DIR/extension.log" ]]; then
    echo "--- extension.log (tail) ---"
    tail -n 80 "$LOG_DIR/extension.log" || true
  fi
  if [[ -f "$LOG_DIR/opensearch.log" ]]; then
    echo "--- opensearch.log (tail) ---"
    tail -n 120 "$LOG_DIR/opensearch.log" || true
  fi
}

cleanup() {
  local status=$?

  if [[ -n "$OPENSEARCH_PID" ]]; then
    kill "$OPENSEARCH_PID" 2>/dev/null || true
    wait "$OPENSEARCH_PID" 2>/dev/null || true
  fi
  if [[ -n "$EXTENSION_PID" ]]; then
    kill "$EXTENSION_PID" 2>/dev/null || true
    wait "$EXTENSION_PID" 2>/dev/null || true
  fi

  if [[ $status -ne 0 ]]; then
    dump_logs
    echo "Live hello harness failed. Logs are under $LOG_DIR" >&2
  elif [[ "$KEEP_WORKDIR" != "1" ]]; then
    rm -rf "$WORK_DIR"
  fi

  exit "$status"
}

trap cleanup EXIT

wait_for_tcp() {
  local host=$1
  local port=$2
  local label=$3

  for _ in $(seq 1 60); do
    if bash -c "exec 3<>/dev/tcp/${host}/${port}" 2>/dev/null; then
      return 0
    fi
    sleep 1
  done

  echo "Timed out waiting for ${label} on ${host}:${port}" >&2
  return 1
}

wait_for_http() {
  local url=$1
  mapfile -t args < <(curl_args 1)

  for _ in $(seq 1 120); do
    if curl "${args[@]}" "$url" >/dev/null 2>&1; then
      return 0
    fi
    sleep 1
  done

  echo "Timed out waiting for OpenSearch HTTP endpoint at $url" >&2
  return 1
}

wait_for_hello() {
  local url=$1
  local body_file=$2
  mapfile -t args < <(curl_args 0)

  for _ in $(seq 1 30); do
    local status
    status=$(curl -sS "${args[@]}" -o "$body_file" -w '%{http_code}' "$url" || true)
    if [[ "$status" == "200" ]] && grep -q 'Hello from Rust!' "$body_file"; then
      printf '%s\n' "$status"
      return 0
    fi
    sleep 1
  done

  status=$(curl -sS "${args[@]}" -o "$body_file" -w '%{http_code}' "$url" || true)
  printf '%s\n' "$status"
  return 1
}

mkdir -p "$LOG_DIR"
rm -rf "$RUNTIME_DIR"
mkdir -p "$RUNTIME_DIR"
: >"$LOG_DIR/extension.log"
: >"$LOG_DIR/opensearch.log"

if [[ "$OPENSEARCH_SKIP_EXTENSION_BUILD" != "1" ]]; then
  cargo build --bin server --manifest-path "$CRATE_DIR/Cargo.toml" >/dev/null
fi

if [[ "$OPENSEARCH_SKIP_BUILD" != "1" ]]; then
  run_with_jdk bash -lc "cd '$OPENSEARCH_DIR' && ./gradlew '$OPENSEARCH_GRADLE_TASK' -x test"
fi

OPENSEARCH_JAVA_HOME=$(resolve_jdk_home)
if [[ -z "$OPENSEARCH_JAVA_HOME" ]]; then
  echo "Unable to resolve a JDK for OpenSearch startup" >&2
  exit 1
fi

OPENSEARCH_ARCHIVE=$(find "$OPENSEARCH_ARCHIVE_DIR" -maxdepth 1 -type f -name 'opensearch-*.tar.gz' | sort | tail -n 1)
if [[ -z "$OPENSEARCH_ARCHIVE" ]]; then
  echo "No OpenSearch archive found in $OPENSEARCH_ARCHIVE_DIR" >&2
  exit 1
fi

tar -xzf "$OPENSEARCH_ARCHIVE" -C "$RUNTIME_DIR"
OPENSEARCH_HOME=$(find "$RUNTIME_DIR" -mindepth 1 -maxdepth 1 -type d | head -n 1)

cat >>"$OPENSEARCH_HOME/config/opensearch.yml" <<EOF
cluster.name: opensearch-sdk-rs-live
node.name: opensearch-sdk-rs-live
network.host: 127.0.0.1
http.port: ${OPENSEARCH_HTTP_PORT}
transport.port: ${OPENSEARCH_TRANSPORT_PORT}
discovery.type: single-node
opensearch.experimental.feature.extensions.enabled: true
EOF

INIT_REQUEST="$WORK_DIR/initialize.json"
cat >"$INIT_REQUEST" <<EOF
{
  "name": "Hello World",
  "uniqueId": "hello-world-rs",
  "hostAddress": "${EXTENSION_HOST}",
  "port": "${EXTENSION_PORT}",
  "version": "0.1.0",
  "opensearchVersion": "3.6.0",
  "minimumCompatibleVersion": "2.19.0"
}
EOF

(
  cd "$CRATE_DIR"
  OPENSEARCH_SDK_RS_HOST="$EXTENSION_HOST" \
  OPENSEARCH_SDK_RS_PORT="$EXTENSION_PORT" \
  "$CRATE_DIR/target/debug/server"
) >"$LOG_DIR/extension.log" 2>&1 &
EXTENSION_PID=$!

wait_for_tcp "$EXTENSION_HOST" "$EXTENSION_PORT" "extension listener"

(
  cd "$OPENSEARCH_HOME"
  OPENSEARCH_JAVA_HOME="$OPENSEARCH_JAVA_HOME" ./bin/opensearch
) >"$LOG_DIR/opensearch.log" 2>&1 &
OPENSEARCH_PID=$!

wait_for_http "$OPENSEARCH_URL"

mapfile -t args < <(curl_args 0)
INIT_STATUS=$(
  curl -sS "${args[@]}" \
    -o "$WORK_DIR/initialize-response.json" \
    -w '%{http_code}' \
    -XPOST \
    "$OPENSEARCH_URL/_extensions/initialize" \
    -H "Content-Type: application/json" \
    --data "@$INIT_REQUEST"
)

if [[ "$INIT_STATUS" != "202" ]]; then
  echo "Unexpected initialize status: $INIT_STATUS" >&2
  cat "$WORK_DIR/initialize-response.json" >&2
  exit 1
fi

HELLO_STATUS=$(wait_for_hello "$OPENSEARCH_URL/_extensions/_hello-world-rs/hello" "$WORK_DIR/hello-response.txt" || true)
HELLO_RESPONSE=$(cat "$WORK_DIR/hello-response.txt")

if [[ "$HELLO_STATUS" != "200" ]] || [[ "$HELLO_RESPONSE" != *"Hello from Rust!"* ]]; then
  echo "Unexpected hello status: $HELLO_STATUS" >&2
  cat "$WORK_DIR/hello-response.txt" >&2
  exit 1
fi

echo "Live hello harness succeeded."
echo "Initialize response saved to $WORK_DIR/initialize-response.json"
echo "Hello response: $HELLO_RESPONSE"
