.DEFAULT_GOAL := help
.PHONY: help build unit-test up down token connect test-oauth test-oauth-expired test-oauth-forbidden test-symbol test clean

PG_VERSION  ?= 18.0
PG_IMAGE    ?= ghcr.io/cloudnative-pg/postgresql:$(PG_VERSION)-system-trixie
COMPOSE      = docker compose -f test/docker-compose.yml
PSQL         = docker run --rm --user root --network test_default \
               -e PGOAUTHDEBUG=UNSAFE \
               -v $(CURDIR)/test/run-psql.sh:/run-psql.sh:ro \
               $(PG_IMAGE) bash /run-psql.sh

# ── Help ──────────────────────────────────────────────────────────────────────

help:
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) \
		| awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-18s\033[0m %s\n", $$1, $$2}'

# ── Build ─────────────────────────────────────────────────────────────────────

build: ## Build the CNPG extension image; stage its /lib and /share into test/ext
	@rm -rf test/ext
	docker buildx build -f lib/Dockerfile --target final -o type=local,dest=test/ext .
	@echo "Staged extension files:" && find test/ext -type f

# ── Unit tests ────────────────────────────────────────────────────────────────

unit-test: ## Run Rust unit tests (uses the pg18 install from ~/.pgrx/config.toml)
	cd lib && \
	if command -v xcrun >/dev/null 2>&1; then \
	  export BINDGEN_EXTRA_CLANG_ARGS="-isysroot $$(xcrun --show-sdk-path)"; \
	fi; \
	cargo test --features pg18

# ── Integration test environment ─────────────────────────────────────────────

up: build ## Build the extension, then start postgres + jwks + mock IdP
	$(COMPOSE) up -d --wait
	@echo "Waiting for postgres..."
	@# app_reader is granted by the token's realm_access.roles; dba is not.
	@$(COMPOSE) exec postgres \
		psql -U postgres -c "CREATE ROLE app_reader LOGIN;" 2>/dev/null || true
	@$(COMPOSE) exec postgres \
		psql -U postgres -c "CREATE ROLE dba LOGIN;" 2>/dev/null || true
	@echo "Ready."

down: ## Stop and remove test containers
	$(COMPOSE) down -v

token: ## Print a fresh test JWT (pip install pyjwt cryptography if missing)
	@python3 test/gen_token.py

# ── Tests ─────────────────────────────────────────────────────────────────────

test-symbol: build ## Verify _PG_oauth_validator_module_init is exported from the .so
	@echo "==> Checking exported symbol..."
	@docker run --rm -v $(CURDIR)/test/ext/lib:/x:ro debian:trixie-slim \
	  sh -c 'apt-get update -qq >/dev/null 2>&1 && apt-get install -y -qq binutils >/dev/null 2>&1 && \
	         nm -D /x/pg_oauth.so | grep -q _PG_oauth_validator_module_init' \
	  && echo "PASS: symbol present" || (echo "FAIL: symbol missing"; exit 1)

test-oauth: up ## Login as a role granted by the token's roles claim (succeeds)
	@echo "==> Connecting as app_reader (granted by token realm_access.roles)..."
	$(PSQL) \
		"host=postgres port=5432 user=app_reader dbname=postgres \
		 sslmode=disable oauth_issuer=http://oauth-server oauth_client_id=test" \
		-c "SELECT current_user, now();" \
		&& echo "PASS: OAuth login succeeded" || (echo "FAIL: OAuth login failed"; exit 1)

test-oauth-forbidden: up ## Login as a role NOT in the token (must be rejected)
	@echo "==> Connecting as dba (NOT granted by token realm_access.roles)..."
	$(PSQL) \
		"host=postgres port=5432 user=dba dbname=postgres \
		 sslmode=disable oauth_issuer=http://oauth-server oauth_client_id=test" \
		-c "SELECT 1;" 2>&1 \
		| grep -q "authentication failed\|FATAL" \
		&& echo "PASS: un-granted role correctly rejected" || (echo "FAIL: un-granted role was accepted"; exit 1)

test-oauth-expired: up ## Verify an expired token is rejected
	@echo "==> Connecting with expired token (should be rejected)..."
	$(PSQL) \
		"host=postgres port=5432 user=app_reader dbname=postgres \
		 sslmode=disable oauth_issuer=http://oauth-server oauth_client_id=expired-test" \
		-c "SELECT 1;" 2>&1 \
		| grep -q "authentication failed\|FATAL" \
		&& echo "PASS: expired token correctly rejected" || (echo "FAIL: expired token was accepted"; exit 1)

# ── Combined ──────────────────────────────────────────────────────────────────

test: build test-symbol unit-test test-oauth test-oauth-forbidden test-oauth-expired ## Run all tests
	@echo ""
	@echo "All tests passed."

# ── Cleanup ───────────────────────────────────────────────────────────────────

clean: down ## Remove containers and staged extension files
	@rm -rf test/ext
