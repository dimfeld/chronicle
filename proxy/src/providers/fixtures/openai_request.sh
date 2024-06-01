#!/bin/bash
source ../../../../.env

cat ./$1.json | \
curl https://api.openai.com/v1/chat/completions \
  -v -X POST \
  --header "Authorization: Bearer ${OPENAI_API_KEY}" \
  --header "Content-Type: application/json" \
  --data-binary @- > $1_response_nonstreaming.json

cat ./$1.json | \
jq '. += { stream: true, stream_options: { include_usage: true } }' | \
curl https://api.openai.com/v1/chat/completions  \
  -v -X POST \
  --header "Authorization: Bearer ${OPENAI_API_KEY}" \
  --header "Content-Type: application/json" \
  --data-binary @- > $1_response_streaming.txt

