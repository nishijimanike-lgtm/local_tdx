# Patches applied on top of rustdx 0.4.4

## 1. TCP timeout (src/tcp/mod.rs)

- **Upstream:** `TIMEOUT = 100ms`
- **Here:** `TIMEOUT = 5s`

The three-packet TDX handshake commonly needs 1–4 seconds on real networks.
With 100ms, `Tcp::new` / `Tcp::new_with_ip` always fail with Windows error 10060
even when TCP port 7709 is reachable.

## 2. K-line parse resilience (src/tcp/stock/kline.rs)

- Accept wire `count` when it differs from the requested count
- Avoid panics on short/empty bodies (some modern HQ hosts handshake but
  return empty historical bar payloads)
