#!/usr/bin/env bash
set -e

echo "Recreating local database..."
npm run db:dev:recreate

echo "Starting worker..."
npm run worker-api:dev:run > worker.log 2>&1 &
WORKER_PID=$!

cleanup() {
    echo "Stopping worker..."
    kill "$WORKER_PID" 2>/dev/null || true
    wait "$WORKER_PID" 2>/dev/null || true
}

trap cleanup EXIT
echo "Waiting for Worker..."
npx wait-on http://127.0.0.1:3001

echo "Running API tests..."
npm run api:tests:bruno