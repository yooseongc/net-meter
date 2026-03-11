# CODEX Test Record

## Loopback Suite

- Date: 2026-03-11
- Runtime mode: `loopback`
- Server config: `config/loopback.runtime.yaml`
- Control CLI: `target/debug/net-meter-cli`
- Test duration per case: `2s`
- Connections per case: `6`
- Result source: `/tmp/net-meter-loopback-suite/results.jsonl`

## Scope

Loopback mode에서 실제로 돌린 조합:

- TCP: `cps`, `cc`, `bw`
- HTTP/1: `cps`, `cc`, `bw`
- HTTP/1 + TLS: `cps`, `cc`, `bw`
- HTTP/2: `cps`, `cc`, `bw`
- HTTP/2 + TLS: `cps`, `cc`, `bw`

총 `15`개 케이스를 `net-meter` 서버와 `net-meter-cli run`으로 end-to-end 실행했다.

## Result Summary

- Total: `15`
- Passed: `15`
- Failed: `0`
- Max CPS: `16797.46`
- Max RPS: `63684.81`
- p99 range: `0.189ms` to `49.855ms`

| Case | Result | CPS | RPS | Conn Attempted | Responses | p99 ms |
|---|---:|---:|---:|---:|---:|---:|
| loopback-http1-bw | PASS | 205 | 675 | 382 | 1244 | 47.359 |
| loopback-http1-bw-tls | PASS | 102 | 475 | 200 | 838 | 47.743 |
| loopback-http1-cc | PASS | 0 | 6 | 6 | 12 | 0.308 |
| loopback-http1-cc-tls | PASS | 0 | 5 | 6 | 12 | 0.46 |
| loopback-http1-cps | PASS | 16797 | 16798 | 32110 | 32106 | 0.543 |
| loopback-http1-cps-tls | PASS | 2943 | 2943 | 5219 | 5217 | 2.791 |
| loopback-http2-bw | PASS | 0 | 25788 | 6 | 39659 | 3.275 |
| loopback-http2-bw-tls | PASS | 0 | 23178 | 6 | 31083 | 3.743 |
| loopback-http2-cc | PASS | 0 | 5 | 6 | 12 | 44.383 |
| loopback-http2-cc-tls | PASS | 0 | 6 | 6 | 12 | 43.231 |
| loopback-http2-cps | PASS | 169 | 169 | 280 | 274 | 48.127 |
| loopback-http2-cps-tls | PASS | 181 | 181 | 267 | 261 | 49.855 |
| loopback-tcp-bw | PASS | 0 | 63684 | 6 | 126870 | 0.189 |
| loopback-tcp-cc | PASS | 0 | 0 | 6 | 12 | 0.213 |
| loopback-tcp-cps | PASS | 14576 | 14574 | 35471 | 35465 | 2.765 |

## Notes

- `net-meter-cli monitor/run`는 이번 수정 이후 정상 동작했다. 이전 `latency_histogram`의 `+Inf` 버킷 직렬화 문제는 재현되지 않았다.
- HTTP/1 `cps`가 loopback 기준 가장 높은 CPS를 보였다.
- TCP `bw`가 가장 높은 RPS를 보였다.
- HTTP/1 + TLS는 plain HTTP/1 대비 CPS가 크게 낮아졌다.
- HTTP/2 `cps`와 `cc`는 기능적으로는 정상 완료됐지만 p99가 약 `43ms`~`50ms`로 다른 조합보다 높았다.
- `namespace`와 `external_port` 검증 결과는 아래 섹션에 추가 기록했다.

## Namespace Suite

- Date: `2026-03-11`
- Runtime mode: `namespace`
- Runtime CLI: `--mode namespace --upper-iface veth-c0 --lower-iface veth-s0`
- Topology: `veth-c0` ↔ `br-nm` ↔ `veth-s0` bridge, client/server namespace 분리
- Control CLI: `engine/target/debug/net-meter-cli`
- Test duration per case: `2s`
- Connections per case: `6`
- Result source: `/tmp/net-meter-modes/namespace/results.jsonl`

### Scope

- TCP: `cps`, `cc`, `bw`
- HTTP/1: `cps`, `cc`, `bw`
- HTTP/1 + TLS: `cps`, `cc`, `bw`
- HTTP/2: `cps`, `cc`, `bw`
- HTTP/2 + TLS: `cps`, `cc`, `bw`

총 `15`개 케이스를 root 권한으로 end-to-end 실행했다.

### Result Summary

- Total: `15`
- Functional pass: `9`
- Functional fail: `6`
- Max CPS: `20249.89`
- Max RPS: `44130.78`
- p99 range: `0.0ms` to `47.999ms`

### Notes

- TCP, HTTP/1, HTTP/1 + TLS 조합은 모두 정상 완료됐다.
- HTTP/2, HTTP/2 + TLS `6`개 조합은 모두 `responses_total=0`, `connections_failed>0`로 실패했다.
- 서버 로그에는 HTTP/2 listener가 namespace 안에서 정상 기동된 기록이 있으므로, 실패 지점은 namespace 경로의 HTTP/2 연결/트래픽 처리 쪽으로 보인다.
- 이번 실행에서 namespace HTTP/2 조합은 성능 수치가 아니라 회귀 후보로 봐야 한다.

## External Port Suite

- Date: `2026-03-11`
- Runtime mode: `external_port`
- Runtime CLI: `--mode external_port --upper-iface veth-c0 --lower-iface veth-s0`
- Topology: `veth-c0` ↔ `veth-c1` ↔ `br-dut` ↔ `veth-s1` ↔ `veth-s0`
- Control CLI: `engine/target/debug/net-meter-cli`
- Test duration per case: `2s`
- Connections per case: `6`
- Result source: `/tmp/net-meter-modes/external_port/results.jsonl`

### Scope

- TCP: `cps`, `cc`, `bw`
- HTTP/1: `cps`, `cc`, `bw`
- HTTP/1 + TLS: `cps`, `cc`, `bw`
- HTTP/2: `cps`, `cc`, `bw`
- HTTP/2 + TLS: `cps`, `cc`, `bw`

총 `15`개 케이스를 root 권한으로 end-to-end 실행했다.

### Result Summary

- Total: `15`
- Passed: `15`
- Failed: `0`
- Max CPS: `19661.07`
- Max RPS: `58133.95`
- p99 range: `0.212ms` to `310.271ms`

### Notes

- `veth-dut` 토폴로지에서 TCP, HTTP/1, HTTP/2, TLS 조합이 모두 기능적으로 정상 완료됐다.
- HTTP/2 `bw`와 HTTP/2 + TLS `bw`는 각각 p99가 약 `310ms`, `222ms`로 다른 조합보다 높았다.
- `external_port`에서는 namespace와 달리 HTTP/2 계열 연결 실패가 재현되지 않았다.
