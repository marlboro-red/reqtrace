# Session handling

## Idle timeout

Sessions carry a rolling `last_seen` timestamp; a sweeper invalidates
anything idle past 30 minutes.

Covers: `req~session-timeout~1`

## Sticky sessions

The load balancer pins sessions to a pod for websocket continuity.

Covers: `req~session-idle~1`

## Login flow

Throttled logins reuse the session bootstrap path.

Covers: `req~Login-Throttling~2`
