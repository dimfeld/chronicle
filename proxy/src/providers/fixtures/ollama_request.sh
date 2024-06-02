#!/bin/bash

cat ./$1.json | \
jq '. += { stream: false }' | \
curl http://localhost:11434/api/chat \
  -v -X POST \
  --header "Content-Type: application/json" \
  --data-binary @- > $1_response_nonstreaming.json

cat ./$1.json | \
jq '. += { stream: true }' | \
curl http://localhost:11434/api/chat  \
  -v -X POST \
  --header "Content-Type: application/json" \
  --data-binary @- > $1_response_streaming.txt


