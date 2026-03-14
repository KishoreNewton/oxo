#!/usr/bin/env bash
# Pushes realistic microservice logs to Loki's push API.
# Simulates 8 services across prod/staging with varied log patterns.
set -euo pipefail

LOKI_URL="${LOKI_URL:-http://loki:3100}"
PUSH_URL="${LOKI_URL}/loki/api/v1/push"

services=("api-gateway" "auth-service" "order-service" "payment-service"
          "user-service" "notification-svc" "inventory-svc" "search-service")
namespaces=("prod" "staging")
levels=("debug" "info" "info" "info" "info" "warn" "warn" "error" "error" "fatal")

info_msgs=(
  'GET /api/v1/users 200 12ms'
  'POST /api/v1/orders 201 45ms'
  'GET /api/v1/health 200 2ms'
  'GET /api/v1/products?page=3 200 89ms'
  'POST /api/v1/auth/login 200 120ms'
  'DELETE /api/v1/sessions/abc123 204 8ms'
  'Cache hit for key=user:1234'
  'Connection pool: 8/20 active'
  'Scheduled task completed in 230ms'
  'Request processed in 14ms'
  'Background worker picked up job batch-export-42'
  'gRPC stream opened for service discovery'
)

warn_msgs=(
  'Slow query: SELECT * FROM orders took 2340ms'
  'Connection pool nearing capacity: 18/20'
  'Retry attempt 2/3 for upstream auth-service'
  'Response time exceeding SLA: 1200ms > 1000ms'
  'Certificate expires in 7 days'
  'Memory usage at 82% of limit'
  'Request queue depth: 142 (threshold: 100)'
  'Upstream latency spike detected: p99=2400ms'
)

error_msgs=(
  'Failed to connect to database: connection refused'
  'NullPointerException in OrderService.processPayment()'
  'HTTP 503 from auth-service: timeout after 30s'
  'Disk space critically low: 2.1GB remaining'
  'OOM killed container payment-worker-3'
  'TLS handshake failed: certificate verify failed'
  'Panic: index out of bounds in inventory handler'
  'Circuit breaker OPEN for payment-gateway (5 failures in 60s)'
)

json_msgs=(
  '{"msg":"Request completed","method":"GET","path":"/api/v1/users","status":200,"duration_ms":12,"trace_id":"abc-123-def"}'
  '{"msg":"Payment processed","amount":99.95,"currency":"USD","order_id":"ord-77231","provider":"stripe"}'
  '{"msg":"Cache miss","key":"product:555","backend":"redis","fallback":"postgres","latency_ms":340}'
  '{"msg":"Rate limit triggered","ip":"203.0.113.77","limit":100,"window_s":60,"remaining":0}'
  '{"msg":"Deployment started","version":"v2.4.1","strategy":"rolling","replicas":3}'
  '{"msg":"Health check","status":"ok","cpu_pct":23,"mem_pct":67,"disk_pct":45}'
  '{"msg":"Batch job finished","job":"invoice-export","records":1500,"elapsed_s":2.3}'
  '{"msg":"Slow query","query":"SELECT * FROM events WHERE ts > now() - 1h","duration_ms":4200,"rows":84000}'
)

pick_random() {
  local -n arr=$1
  echo "${arr[$((RANDOM % ${#arr[@]}))]}"
}

generate_entry() {
  local svc="${services[$((RANDOM % ${#services[@]}))]}"
  local ns="${namespaces[$((RANDOM % ${#namespaces[@]}))]}"
  local lvl="${levels[$((RANDOM % ${#levels[@]}))]}"

  local msg
  # ~25% JSON, rest plain text
  if (( RANDOM % 4 == 0 )); then
    msg=$(pick_random json_msgs)
  else
    case "$lvl" in
      debug|info) msg=$(pick_random info_msgs) ;;
      warn)       msg=$(pick_random warn_msgs) ;;
      error|fatal) msg=$(pick_random error_msgs) ;;
    esac
  fi

  local ts
  ts=$(date -u +%s)000000000

  cat <<ENTRY
{
  "streams": [{
    "stream": {
      "job": "microservices",
      "service": "$svc",
      "namespace": "$ns",
      "level": "$lvl"
    },
    "values": [["$ts", "$msg"]]
  }]
}
ENTRY
}

echo "Log generator starting — pushing to $PUSH_URL"

# Wait briefly for Loki to be fully ready
sleep 2

while true; do
  # Push 2-6 entries per batch
  batch_size=$(( (RANDOM % 5) + 2 ))
  for _ in $(seq 1 "$batch_size"); do
    payload=$(generate_entry)
    curl -s -X POST "$PUSH_URL" \
      -H "Content-Type: application/json" \
      -d "$payload" > /dev/null 2>&1 || true
  done

  # Random delay 100-400ms
  sleep "0.$(( (RANDOM % 3) + 1 ))"
done
