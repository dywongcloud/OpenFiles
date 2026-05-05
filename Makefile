.PHONY: fmt check test smoke minio server zip

fmt:
	cargo fmt --all

check:
	cargo check --workspace

test:
	cargo test --workspace

smoke:
	./scripts/smoke-local.sh

minio:
	docker compose -f examples/docker-compose.yml up -d minio createbucket

server:
	cargo run -p openfiles-server -- --config openfiles.toml --listen 127.0.0.1:8787

zip:
	./scripts/package.sh
