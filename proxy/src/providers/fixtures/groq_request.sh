#!/bin/bash
source ../../../../.env

cat ./$1.json | \
curl https://api.groq.com/openai/v1/chat/completions \
  -v -X POST \
  --header "Authorization: Bearer ${GROQ_API_KEY}" \
  --header "Content-Type: application/json" \
  --data-binary @- > $1_response_nonstreaming.json

cat ./$1.json | \
jq '. += { stream: true }' | \
curl https://api.groq.com/openai/v1/chat/completions \
  -v -X POST \
  --header "Authorization: Bearer ${GROQ_API_KEY}" \
  --header "Content-Type: application/json" \
  --data-binary @- > $1_response_streaming.txt


