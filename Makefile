.DEFAULT_GOAL := help

.PHONY: help build test test-db lint fmt run docker-up docker-down migrate clean

help: ## Show available targets
	@grep -E '^[a-zA-Z_-]+:.*##' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*##"}; {printf "  \033[36m%-14s\033[0m %s\n", $$1, $$2}'

build: ## Compile the project
	cargo build

test: ## Run the full test suite (requires DATABASE_URL)
	cargo test

test-db: ## Start a test Postgres container and run the full test suite
	docker compose -f docker-compose.test.yml up -d --wait
	DATABASE_URL=postgres://postgres:postgres@localhost/soroban_pulse_test cargo test; \
	  EXIT=$$?; \
	  docker compose -f docker-compose.test.yml down; \
	  exit $$EXIT

lint: ## Run clippy with warnings as errors
	cargo clippy -- -D warnings

fmt: ## Format source code
	cargo fmt

run: ## Start the development server
	cargo run

docker-up: ## Start the full stack via Docker Compose
	docker-compose up --build -d

docker-down: ## Tear down the Docker Compose stack
	docker-compose down

migrate: ## Run pending database migrations
	cargo sqlx migrate run

migrate-down: ## Rollback the most recent migration
	cargo sqlx migrate revert

clean: ## Remove build artifacts
	cargo clean
