# 14. Base infrastructure in Docker Desktop's Kubernetes; the app runs natively in development

- **Status:** Accepted, with an open question on keeping the infra local
- **Date:** 2026-10-31. Accepted 2026-10-31.

## Context

[ADR 13](0013-opentelemetry-and-lgtm.md) added a Grafana LGTM backend. It first ran from a
Compose file that `scripts/calcifer observe` started, waited on, and pointed the server at.
That tied the run script to Docker and to one piece of infrastructure, and each new
service the server needs beside it (a collector today, perhaps more later) would add to
it.

The server's own development loop is native: `cargo run` and Vite's hot reload. Building
an image for every change would slow it for nothing.

## Decision

**Long-running infrastructure the server depends on runs in Docker Desktop's
Kubernetes, declared as a kustomization in `deploy/k8s/infra/` and reachable on the
host's localhost only. During development the server runs natively and reaches it
through ordinary configuration.**

1. **Declared, not scripted.** `kubectl --context docker-desktop apply -k
   deploy/k8s/infra` is the whole setup. The run script knows nothing about it.
2. **Published with a `LoadBalancer` Service, on every interface for now.** Docker Desktop
   publishes a `LoadBalancer` Service's ports on the host, so the server and the browser
   reach them on `localhost`. Kubernetes has no field for a host bind address, and Docker
   Desktop's load balancer (`cloud-provider-kind`, an Envoy container) always publishes on
   `0.0.0.0`, so the ports are also reachable from the network the machine is on. Only
   ports something on the host uses are published (Grafana 3000, OTLP/gRPC 4317). This
   falls short of ADR 5's localhost-only rule, and Grafana is `admin`/`admin`; it's
   accepted for now on a personal network, and making it local is an open question (see
   below).
3. **The server finds it through configuration.** `OTEL_EXPORTER_OTLP_ENDPOINT` in
   `server/.env` (an example line is in `.env.example`), read at boot (ADR 13, decision
   3). The server doesn't know where the collector runs.
4. **State on volumes.** LGTM's data is on a PersistentVolumeClaim, so it survives a
   restart or a redeploy; `kubectl delete -k` removes it.
5. **`--context docker-desktop` in every documented command,** so an apply never lands
   on another cluster that happens to be current.

## Alternatives considered

- **Compose with a run-script wrapper** (what this replaces). Fewer moving parts, but it
  put infrastructure in the run script and isn't how the infra would be described for a
  real cluster.
- **kind.** The cluster itself would be declared in the repository and recreated by one
  command, but it needs installing and explicit port mappings, while Docker Desktop is
  already here and publishes `LoadBalancer` services on localhost by itself.
- **Docker Desktop's "Port binding behavior: Localhost only".** Makes every Docker port
  bind loopback, but Docker Desktop's load balancer asks for `0.0.0.0` explicitly, so
  under that setting it fails to start (`docker run` exits 125) and no `LoadBalancer`
  Service gets an address.
- **`kubectl port-forward` to `127.0.0.1`, kept alive by launchd.** Loopback for certain,
  but a per-machine process outside the repository that drops when the pod restarts.
- **An Ingress.** It needs a controller, which Docker Desktop's publishes the same way;
  it adds routing by hostname, which one service doesn't need.
- **The server in the cluster too.** Possible later as its own overlay (an image, one
  replica, volumes for the SQLite database and the embedding model cache), for a
  production-like run. Not for development.

## Open question: keeping it on this machine

Two ways to meet ADR 5, to choose between later:

- **The macOS firewall blocking incoming connections to Docker.** Keeps Docker Desktop's
  Kubernetes; other devices can't connect, while `localhost` and containers' outbound
  traffic are unaffected. Applies to everything Docker publishes, and the ports still
  listen on every interface.
- **Standalone kind** with `extraPortMappings` on `listenAddress: 127.0.0.1` and a
  `NodePort` Service. Binds loopback per port, works with "Localhost only", and declares
  the cluster in the repository; it needs kind installed instead of Docker Desktop's
  Kubernetes.

## Consequences

- Kubernetes must be enabled in Docker Desktop, once, on each machine.
- Until the open question is settled, Grafana and the OTLP port are reachable from the
  local network.
- Grafana and the OTLP ports stay up while the cluster runs, whether or not the server
  does.
- A dashboard edit takes a re-apply: the dashboard ConfigMap is generated with a content
  hash, so the change rolls the pod.
