#!/usr/bin/env python3
"""Mint a test JWT signed with the RSA test key."""
import sys
import time
from pathlib import Path

try:
    import jwt
except ImportError:
    sys.exit("Missing dependency: pip install pyjwt cryptography")

FIXTURES = Path(__file__).parent.parent / "lib" / "src" / "tests" / "fixtures"

key = (FIXTURES / "rsa_private.pem").read_text()
now = int(time.time())

claims = {
    "iss":   "http://oauth-server",
    "aud":   "postgresql",
    "sub":   "testuser",
    "scope": "db:read",
    "realm_access": {"roles": ["app_reader"]},
    "exp":   now + 3600,
    "iat":   now,
    "kid":   "test-rsa-key",
}

print(jwt.encode(claims, key, algorithm="RS256"))
