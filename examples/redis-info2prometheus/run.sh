#!/bin/sh

export OTEL_SERVICE_NAME=test-redis-info2prom-service

cat ./sample.toml |
  ./target/release/redis-info2prometheus
