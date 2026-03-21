#!/usr/bin/env bash
# Install libpq-oauth (provides OAUTHBEARER SASL support) then exec psql with all args.
apt-get update -qq >/dev/null 2>&1
apt-get install -y -qq libpq-oauth >/dev/null 2>&1
exec psql "$@"
