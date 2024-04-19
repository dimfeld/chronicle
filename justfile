_list:
  @just --list

run:
  cd api/web && bun run build
  cd api && cargo run --release serve

filigree:
  cd api && ../../filigree/target/debug/filigree write

prepare:
  cd api/web && bun install && bun run build

dev-api:
  cd api && cargo watch -d 0.1 -x 'lrun serve --dev'

dev-web:
  cd api/web && bun run dev
