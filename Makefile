.PHONY: build dev test clean docker

# Build frontend + backend
build:
	cd crates/tdx-web && npm install && npm run build
	cargo build --release

# Dev mode: frontend HMR + backend
dev:
	cd crates/tdx-web && npm run dev &
	cargo run -p tdx-maintain-server

# Run tests
test:
	cargo test --workspace

# Check compilation
check:
	cargo check --workspace

# Clean build artifacts
clean:
	cargo clean
	rm -rf crates/tdx-web/dist crates/tdx-web/node_modules

# Docker
docker:
	docker build -t tdx-maintain .
	docker compose up -d
