"""Mint hub tokens for the e2e fixture: prints one JSON object on stdout.

Usage: python mint.py <tokens-db-path> <slug>
"""

import json
import sys

from tidetable.tokens import mint_token

db, slug = sys.argv[1], sys.argv[2]
read_token = mint_token(db, [slug], ["read"], expires_days=1)[1]
host_token = mint_token(db, [slug], ["read", "write"], expires_days=1)[1]
json.dump(
    {
        "pool_read": read_token,
        "host": host_token,
    },
    sys.stdout,
)
