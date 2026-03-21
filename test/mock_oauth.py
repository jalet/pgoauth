#!/usr/bin/env python3
"""
Minimal mock OAuth 2.0 device authorization server for pg_oauth integration tests.

Endpoints:
  GET  /.well-known/openid-configuration  — OIDC discovery
  POST /device                            — device authorization (instant approval)
  POST /token                             — token endpoint (returns JWT immediately)

client_id=expired-test  →  returns an already-expired JWT for negative testing.
"""
import json
import time
import urllib.parse
from http.server import BaseHTTPRequestHandler, HTTPServer
from pathlib import Path

import jwt

ISSUER = "http://oauth-server"
FIXTURES = Path("/fixtures")
PRIVATE_KEY = (FIXTURES / "rsa_private.pem").read_text()

# device_code → client_id
_pending: dict[str, str] = {}


def _make_token(expired: bool = False) -> str:
    now = int(time.time())
    claims = {
        "iss":   ISSUER,
        "aud":   "postgresql",
        "sub":   "testuser",
        "scope": "db:read",
        "iat":   now - 3610 if expired else now,
        "exp":   now - 10  if expired else now + 3600,
        "kid":   "test-rsa-key",
    }
    return jwt.encode(claims, PRIVATE_KEY, algorithm="RS256", headers={"kid": "test-rsa-key"})


def _parse_form(body: bytes) -> dict:
    return dict(urllib.parse.parse_qsl(body.decode()))


class Handler(BaseHTTPRequestHandler):
    def _send(self, data: dict, status: int = 200) -> None:
        body = json.dumps(data).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def _read_body(self) -> bytes:
        length = int(self.headers.get("Content-Length", 0))
        return self.rfile.read(length) if length else b""

    def do_GET(self):
        if self.path == "/.well-known/openid-configuration":
            self._send({
                "issuer": ISSUER,
                "device_authorization_endpoint": f"{ISSUER}/device",
                "token_endpoint": f"{ISSUER}/token",
                "response_types_supported": ["token"],
                "grant_types_supported": [
                    "urn:ietf:params:oauth:grant-type:device_code",
                ],
            })
        else:
            self.send_response(404)
            self.end_headers()

    def do_POST(self):
        body = self._read_body()
        params = _parse_form(body)

        if self.path == "/device":
            client_id = params.get("client_id", "test")
            device_code = f"devcode-{client_id}-{int(time.time())}"
            _pending[device_code] = client_id
            self._send({
                "device_code":      device_code,
                "user_code":        "TEST-CODE",
                "verification_uri": f"{ISSUER}/activate",
                "expires_in":       300,
                "interval":         0,
            })

        elif self.path == "/token":
            device_code = params.get("device_code", "")
            client_id = _pending.pop(device_code, params.get("client_id", "test"))
            expired = client_id == "expired-test"
            self._send({
                "access_token": _make_token(expired=expired),
                "token_type":   "bearer",
                "expires_in":   0 if expired else 3600,
            })

        else:
            self.send_response(404)
            self.end_headers()

    def log_message(self, fmt, *args):
        pass  # suppress access logs


if __name__ == "__main__":
    server = HTTPServer(("", 80), Handler)
    print(f"Mock OAuth server listening on :80 (issuer={ISSUER})", flush=True)
    server.serve_forever()
