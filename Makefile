.DEFAULT_GOAL := help

.PHONY: help build test lint fmt run docker-up docker-down migrate clean

help: ## Show available targets
	@grep -E '^[a-zA-Z_-]+:.*##' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*##"}; {printf "  \033[36m%-14s\033[0m %s\n", $$1, $$2}'

build: ## Compile the project
	cargo build

test: ## Run the full test suite (requires DATABASE_URL)
	cargo test

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

clean: ## Remove build artifacts
	cargo clean
