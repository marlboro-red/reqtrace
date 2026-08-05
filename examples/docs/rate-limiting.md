# Rate limiting

Sliding-window counters per account and per source IP. Window state lives
in Redis with a 10-minute TTL; on Redis loss we fail open and alert.

Covers: req~login-throttling~1

## Retry queue

Failed lockout notifications are retried with exponential backoff. This is
a design decision with no parent requirement in the HLD, so it declares
itself instead of covering something:

Derived: dsn~retry-queue~1
