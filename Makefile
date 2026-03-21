.DEFAULT_GOAL := help
.PHONY: help build unit-test up down token connect test-oauth test-symbol clean

PG_VERSION  ?= 18.0
COMPOSE      = docker compose -f test/docker-compose.yml
PSQL         = docker run --rm --user root --network test_default \
               -e PGOAUTHDEBUG=UNSAFE \
               -v $(CURDIR)/test/run-psql.sh:/run-psql.sh:ro \
               local bash /run-psql.sh

# ── Help ──────────────────────────────────────────────────────────────────────

help:
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) \
		| awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-18s\033[0m %s\n", $$1, $$2}'

# ── Build ─────────────────────────────────────────────────────────────────────

build: ## Build the local Docker image (PG_VERSION=18.0)
	docker buildx bake \
		-f docker-bake.hcl \
		-f src/docker-bake.hcl \
		src-local \
		--set "*.args.BASE_IMAGE=ghcr.io/cloudnative-pg/postgresql:$(PG_VERSION)-system-trixie"

# ── Unit tests ────────────────────────────────────────────────────────────────

unit-test: ## Run Rust unit tests (requires postgresql-server-dev-18 + clang)
	cd lib/src && PGRX_PG_CONFIG_PATH=$$(which pg_config) cargo test --features pg18

# ── Integration test environment ─────────────────────────────────────────────

up: ## Start postgres + jwks containers
	$(COMPOSE) up -d --wait
	@echo "Waiting for postgres..."
	@$(COMPOSE) exec postgres \
		psql -U postgres -c "CREATE USER testuser LOGIN;" 2>/dev/null || true
	@echo "Ready."

down: ## Stop and remove test containers
	$(COMPOSE) down -v

token: ## Print a fresh test JWT (pip install pyjwt cryptography if missing)
	@python3 test/gen_token.py

# ── Tests ─────────────────────────────────────────────────────────────────────

test-symbol: ## Verify _PG_oauth_validator_module_init is exported from the .so
	@echo "==> Checking exported symbol..."
	docker run --rm local \
		nm -D /usr/lib/postgresql/18/lib/pg_oauth.so \
		| grep -q _PG_oauth_validator_module_init \
		&& echo "PASS: symbol present" || (echo "FAIL: symbol missing"; exit 1)

test-oauth: up ## Full OAuth login test (valid token → connect succeeds)
	@echo "==> Connecting with valid token..."
	$(PSQL) \
		"host=postgres port=5432 user=testuser dbname=postgres \
		 sslmode=disable oauth_issuer=http://oauth-server oauth_client_id=test" \
		-c "SELECT current_user, now();" \
		&& echo "PASS: OAuth login succeeded" || (echo "FAIL: OAuth login failed"; exit 1)

test-oauth-expired: up ## Verify an expired token is rejected
	@echo "==> Connecting with expired token (should be rejected)..."
	$(PSQL) \
		"host=postgres port=5432 user=testuser dbname=postgres \
		 sslmode=disable oauth_issuer=http://oauth-server oauth_client_id=expired-test" \
		-c "SELECT 1;" 2>&1 \
		| grep -q "authentication failed\|FATAL" \
		&& echo "PASS: expired token correctly rejected" || (echo "FAIL: expired token was accepted"; exit 1)

# ── Combined ──────────────────────────────────────────────────────────────────

test: build test-symbol unit-test test-oauth test-oauth-expired ## Run all tests
	@echo ""
	@echo "All tests passed."

# ── Cleanup ───────────────────────────────────────────────────────────────────

clean: down ## Remove containers and the local Docker image
	docker image rm local 2>/dev/null || true
