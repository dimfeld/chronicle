#!/bin/bash
set -e
source ../../../../.env

cat ./$1.json | \
curl https://api.anthropic.com/v1/messages \
  -v -X POST \
  --header "x-api-key: ${ANTHROPIC_API_KEY}" \
  --header "Content-Type: application/json" \
  --header "anthropic-version: 2023-06-01" \
  --data-binary @- > $1_response_nonstreaming.json

cat ./$1.json | \
jq '. += { stream: true }' | \
curl https://api.anthropic.com/v1/messages \
  -v -X POST \
  --header "x-api-key: ${ANTHROPIC_API_KEY}" \
  --header "Content-Type: application/json" \
  --header "anthropic-version: 2023-06-01" \
  --data-binary @- > $1_response_streaming.txt
