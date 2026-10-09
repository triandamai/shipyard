<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import {
    Cpu,
    MemoryStick,
    HardDrive,
    Network,
    RefreshCw,
    Activity,
    Radio,
    Server,
    Copy,
    Check,
    Link,
    Box,
    AlertCircle,
  } from "@lucide/svelte";
  import { api } from "$lib/api/client";
  import { orgStore } from "$lib/stores/org.store";
  import { can, perm } from "$lib/auth/permissions";
  import {
    Badge,
    DataTable,
    Button,
    Card,
    InlineAlert,
    ProgressBar,
    SectionLabel,
    Spinner,
    StatCard,
    StatusDot,
  } from "$lib/components/ui";
  import PermissionDeniedDialog from "$lib/components/PermissionDeniedDialog.svelte";
  import type { SwarmNode, SwarmJoinTokens } from "$lib/api/types";

  let orgId = $derived($orgStore.activeOrg?.id ?? "");
  let myRole = $derived($orgStore.myMembership?.role ?? null);
  let myPerms = $derived($orgStore.myMembership?.permissions ?? []);
  let membershipLoaded = $derived($orgStore.membershipLoaded);
  let canInfraRead = $derived(
    can(myRole, myPerms, perm(orgId, "infra", "read")) ||
      can(myRole, myPerms, perm(orgId, "settings", "read")),
  );

  interface DiskInfo {
    mount: string;
    total_gb: number;
    used_gb: number;
    used_pct: number;
  }

  interface NetInfo {
    iface: string;
    rx_bytes: number;
    tx_bytes: number;
  }

  interface SystemInfo {
    cpu_usage_pct: number;
    memory_total_mb: number;
    memory_used_mb: number;
    memory_used_pct: number;
    swap_total_mb: number;
    swap_used_mb: number;
    uptime_secs: number;
    disks: DiskInfo[];
    networks: NetInfo[];
  }

  let info = $state<SystemInfo | null>(null);
  let error = $state("");
  let connected = $state(false);

  // ── Core service monitoring ───────────────────────────────────────────────
  interface ContainerSummary {
    id: string;
    names: string[];
    image: string;
    status: string;
    state: string;
    created: number;
  }

  interface ContainerResourceStats {
    cpu_pct: number;
    mem_used_mb: number;
    mem_limit_mb: number;
    mem_pct: number;
    blkio_read_bytes: number;
    blkio_write_bytes: number;
  }

  const CORE_SERVICES = [
    { key: "shipyard-traefik", label: "Traefik", desc: "Reverse proxy & TLS" },
    { key: "shipyard-backend", label: "Backend", desc: "API server" },
    { key: "shipyard-frontend", label: "Frontend", desc: "Web UI" },
    { key: "shipyard-mqtt", label: "MQTT", desc: "Broker / events" },
    { key: "shipyard-redis", label: "Redis", desc: "Cache & sessions" },
    { key: "shipyard-postgres", label: "Postgres", desc: "Primary database" },
    {
      key: "shipyard-nginx-static",
      label: "Nginx Static",
      desc: "Static file server",
    },
  ] as const;

  let coreContainers = $state<ContainerSummary[]>([]);
  let coreStats = $state<Record<string, ContainerResourceStats>>({});
  let coreLoading = $state(false);
  let coreError = $state("");
  let coreLastRefresh = $state<Date | null>(null);

  function findContainer(key: string): ContainerSummary | undefined {
    return coreContainers.find((c) =>
      c.names.some((n) => n === `/${key}` || n === key),
    );
  }

  function serviceState(
    c: ContainerSummary | undefined,
  ): "running" | "stopped" | "unknown" {
    if (!c) return "unknown";
    if (c.state === "running") return "running";
    return "stopped";
  }

  function stateColor(s: "running" | "stopped" | "unknown"): string {
    if (s === "running") return "var(--accent-green)";
    if (s === "stopped") return "var(--accent-red)";
    return "var(--accent-yellow)";
  }

  // Keeps the old three-colour state semantics (running=green, stopped=red,
  // unknown=orange→warning). toDotStatus would turn "stopped" grey.
  function coreDot(
    s: "running" | "stopped" | "unknown",
  ): "running" | "failed" | "warning" {
    if (s === "running") return "running";
    if (s === "stopped") return "failed";
    return "warning";
  }

  function fmtMemMb(mb: number): string {
    if (mb >= 1024) return `${(mb / 1024).toFixed(1)} GB`;
    return `${mb.toFixed(0)} MB`;
  }

  async function loadCoreServices() {
    if (!orgId) return;
    coreLoading = true;
    coreError = "";
    try {
      const res = await api.get<ContainerSummary[]>(
        `/admin/docker/containers?org_id=${orgId}`,
      );
      if (res.error) {
        coreError = res.error.message;
        return;
      }
      coreContainers = res.data ?? [];
      coreLastRefresh = new Date();
      // fetch resource stats for all core services in one call
      const names = CORE_SERVICES.map((s) => s.key).join(",");
      const sres = await api.get<Record<string, ContainerResourceStats>>(
        `/admin/docker/containers/resource-stats?org_id=${orgId}&names=${names}`,
      );
      if (sres.data) coreStats = sres.data;
    } catch (e) {
      coreError = String(e);
    } finally {
      coreLoading = false;
    }
  }

  let nodes = $state<SwarmNode[]>([]);
  let nodesError = $state("");
  let nodesLoading = $state(false);

  let joinTokens: SwarmJoinTokens | null = $state(null);
  let joinTokensError = $state("");
  let joinTokensLoading = $state(false);
  let copiedToken = $state<"worker" | "manager" | null>(null);

  let es: EventSource | null = null;
  // Last time resource stats were fetched — throttle to once every 30 s.
  let lastStatsFetch = 0;

  function openStream() {
    if (es) {
      es.close();
      es = null;
    }

    error = "";
    connected = false;

    const source = new EventSource("/api/admin/system/stream");

    source.onopen = () => {
      connected = true;
      error = "";
    };

    source.onmessage = (ev) => {
      try {
        info = JSON.parse(ev.data) as SystemInfo;
        connected = true;
        error = "";
        // Piggy-back resource stats refresh on the SSE tick, throttled.
        const now = Date.now();
        if (canInfraRead && orgId && now - lastStatsFetch > 30_000) {
          lastStatsFetch = now;
          loadCoreServices();
        }
      } catch {
        // ignore malformed frame
      }
    };

    source.onerror = () => {
      connected = false;
      // EventSource retries automatically; surface a warning only after
      // the first frame has never arrived.
      if (!info) error = "Unable to connect to metrics stream — retrying…";
    };

    es = source;
  }

  function formatUptime(secs: number): string {
    const d = Math.floor(secs / 86400);
    const h = Math.floor((secs % 86400) / 3600);
    const m = Math.floor((secs % 3600) / 60);
    const parts: string[] = [];
    if (d) parts.push(`${d}d`);
    if (h) parts.push(`${h}h`);
    parts.push(`${m}m`);
    return parts.join(" ");
  }

  function formatBytes(b: number): string {
    if (b >= 1_099_511_627_776)
      return `${(b / 1_099_511_627_776).toFixed(2)} TB`;
    if (b >= 1_073_741_824) return `${(b / 1_073_741_824).toFixed(2)} GB`;
    if (b >= 1_048_576) return `${(b / 1_048_576).toFixed(1)} MB`;
    if (b >= 1024) return `${(b / 1024).toFixed(1)} KB`;
    return `${b} B`;
  }

  function gaugeColor(pct: number): string {
    if (pct >= 90) return "var(--accent-red)";
    if (pct >= 70) return "var(--accent-yellow)";
    return "var(--accent)";
  }

  // Same thresholds as gaugeColor, as ProgressBar / StatCard tone names.
  function gaugeTone(pct: number): "red" | "yellow" | "blue" {
    if (pct >= 90) return "red";
    if (pct >= 70) return "yellow";
    return "blue";
  }

  // Deduplicate disk entries: multiple bind-mounted paths can share the same
  // underlying partition (same total_gb + used_gb). Keep only the shortest
  // (most generic) mount path per unique partition signature.
  function deduplicateDisks(disks: DiskInfo[]): DiskInfo[] {
    const seen = new Map<string, DiskInfo>();
    for (const d of disks) {
      const key = `${d.total_gb}:${d.used_gb}`;
      const existing = seen.get(key);
      if (!existing || d.mount.length < existing.mount.length) {
        seen.set(key, d);
      }
    }
    return [...seen.values()].sort((a, b) => a.mount.localeCompare(b.mount));
  }

  const HIDE_IFACES = new Set(["lo", "docker0", "docker_gwbridge"]);
  function visibleNets(nets: NetInfo[]): NetInfo[] {
    return nets.filter(
      (n) =>
        !HIDE_IFACES.has(n.iface) &&
        !n.iface.startsWith("br-") &&
        !n.iface.startsWith("veth"),
    );
  }

  async function loadNodes() {
    nodesLoading = true;
    nodesError = "";
    try {
      const res = await api.getSwarmNodes(orgId);
      if (res.error) nodesError = res.error.message;
      else nodes = res.data ?? [];
    } catch (e) {
      nodesError = String(e);
    } finally {
      nodesLoading = false;
    }
  }

  async function loadJoinTokens() {
    joinTokensLoading = true;
    joinTokensError = "";
    try {
      const res = await api.getSwarmJoinTokens(orgId);
      if (res.error) joinTokensError = res.error.message;
      else joinTokens = res.data ?? null;
    } catch (e) {
      joinTokensError = String(e);
    } finally {
      joinTokensLoading = false;
    }
  }

  async function copyToken(type: "worker" | "manager") {
    if (!joinTokens) return;
    const cmd = `docker swarm join --token ${type === "worker" ? joinTokens.worker : joinTokens.manager} ${joinTokens.addr}`;
    await navigator.clipboard.writeText(cmd);
    copiedToken = type;
    setTimeout(() => (copiedToken = null), 2000);
  }

  onMount(() => {
    openStream();
    if (canInfraRead) {
      loadNodes();
      loadJoinTokens();
      loadCoreServices();
    }
  });

  onDestroy(() => {
    es?.close();
    es = null;
  });

  // Page-local mapping preserving the old colours: ready=green, down=red,
  // anything else=orange (toDotStatus would turn "down" grey).
  function nodeDot(status: string): "running" | "failed" | "warning" {
    if (status === "ready") return "running";
    if (status === "down") return "failed";
    return "warning";
  }

  // Old availability colours: active=green, drain=red, else=orange.
  function availTone(avail: string): "green" | "red" | "yellow" {
    if (avail === "active") return "green";
    if (avail === "drain") return "red";
    return "yellow";
  }
</script>

<PermissionDeniedDialog
  open={membershipLoaded && !!orgId && !canInfraRead}
  message="You need the 'View infrastructure' permission to access this page."
  onDismiss={() => history.back()}
  onBack={() => history.back()}
/>

{#if canInfraRead}
  <div class="infra-page">
    <div class="page-toolbar">
      <div class="toolbar-left">
        <Activity size={15} />
        <span class="toolbar-title">Infrastructure</span>
        {#if info}
          <Badge tone="neutral">Up {formatUptime(info.uptime_secs)}</Badge>
        {/if}
        <span class={connected ? "live-wrap live" : "live-wrap"}>
          <Badge tone={connected ? "green" : "neutral"}>
            <Radio size={11} />&nbsp;{connected ? "Live" : "Reconnecting…"}
          </Badge>
        </span>
      </div>
      <Button variant="secondary" size="sm" onclick={openStream}>
        <RefreshCw size={14} />
        Reconnect
      </Button>
    </div>

    {#if error}
      <div role="alert"><InlineAlert tone="error">{error}</InlineAlert></div>
    {/if}

    {#if !info}
      <div class="waiting">
        <Spinner size={18} />
        Waiting for first metrics frame…
      </div>
    {:else}
      <!-- Top stat cards -->
      <div class="stat-grid">
        <!-- CPU -->
        <div class="metric-card">
          <StatCard
            tone={gaugeTone(info.cpu_usage_pct)}
            value="{info.cpu_usage_pct.toFixed(1)}%"
            label="CPU Usage"
          >
            {#snippet icon()}<Cpu size={16} />{/snippet}
          </StatCard>
          <div class="metric-foot">
            <ProgressBar
              value={info.cpu_usage_pct}
              tone={gaugeTone(info.cpu_usage_pct)}
            />
          </div>
        </div>

        <!-- Memory -->
        <div class="metric-card">
          <StatCard
            tone={gaugeTone(info.memory_used_pct)}
            value="{info.memory_used_pct.toFixed(1)}%"
            label="Memory"
          >
            {#snippet icon()}<MemoryStick size={16} />{/snippet}
          </StatCard>
          <div class="metric-foot">
            <ProgressBar
              value={info.memory_used_pct}
              tone={gaugeTone(info.memory_used_pct)}
            />
            <span class="metric-sub">
              {info.memory_used_mb.toLocaleString()} MB / {info.memory_total_mb.toLocaleString()}
              MB
            </span>
          </div>
        </div>

        <!-- Swap -->
        {#if info.swap_total_mb > 0}
          {@const swapPct =
            info.swap_total_mb > 0
              ? (info.swap_used_mb / info.swap_total_mb) * 100
              : 0}
          <div class="metric-card">
            <StatCard
              tone={gaugeTone(swapPct)}
              value="{swapPct.toFixed(1)}%"
              label="Swap"
            >
              {#snippet icon()}<MemoryStick size={16} />{/snippet}
            </StatCard>
            <div class="metric-foot">
              <ProgressBar value={swapPct} tone={gaugeTone(swapPct)} />
              <span class="metric-sub">
                {info.swap_used_mb.toLocaleString()} MB / {info.swap_total_mb.toLocaleString()}
                MB
              </span>
            </div>
          </div>
        {/if}
      </div>

      <!-- Core Services -->
      <div class="block">
        <div class="sec-head">
          <Box size={14} />
          <SectionLabel>Core Services</SectionLabel>
          {#if coreLastRefresh}
            <span class="section-hint"
              >Updated {coreLastRefresh.toLocaleTimeString()}</span
            >
          {/if}
          <div class="sec-action" class:sec-action-auto={!coreLastRefresh}>
            <Button
              variant="secondary"
              size="icon"
              onclick={loadCoreServices}
              disabled={coreLoading}
              title="Refresh"
              aria-label="Refresh core services"
            >
              <RefreshCw size={13} />
            </Button>
          </div>
        </div>

        <Card padding="0">
          {#if coreError}
            <div class="core-error"><AlertCircle size={13} />{coreError}</div>
          {/if}

          <div class="core-grid">
            {#each CORE_SERVICES as svc}
              {@const container = findContainer(svc.key)}
              {@const state = serviceState(container)}
              {@const stats = coreStats[svc.key]}
              <div
                class={state === "running"
                  ? "core-card running"
                  : state === "stopped"
                    ? "core-card stopped"
                    : "core-card"}
              >
                <div class="core-card-header">
                  <StatusDot status={coreDot(state)} />
                  <span class="core-label">{svc.label}</span>
                  <span
                    class="core-state-badge"
                    style="color:{stateColor(state)}">{state}</span
                  >
                </div>
                <div class="core-desc">{svc.desc}</div>
                {#if container}
                  <div class="core-image">{container.image}</div>
                {:else if !coreLoading}
                  <div class="core-status-text muted">Container not found</div>
                {/if}
                {#if stats}
                  <div class="core-metrics">
                    <!-- CPU -->
                    <div class="core-metric-row">
                      <span class="core-metric-label">CPU</span>
                      <div class="core-gauge">
                        <ProgressBar
                          value={stats.cpu_pct}
                          tone={gaugeTone(stats.cpu_pct)}
                        />
                      </div>
                      <span
                        class="core-metric-val"
                        style="color:{gaugeColor(stats.cpu_pct)}"
                        >{stats.cpu_pct.toFixed(1)}%</span
                      >
                    </div>
                    <!-- Memory -->
                    <div class="core-metric-row">
                      <span class="core-metric-label">MEM</span>
                      <div class="core-gauge">
                        <ProgressBar
                          value={stats.mem_pct}
                          tone={gaugeTone(stats.mem_pct)}
                        />
                      </div>
                      <span
                        class="core-metric-val"
                        style="color:{gaugeColor(stats.mem_pct)}"
                        >{fmtMemMb(stats.mem_used_mb)}</span
                      >
                    </div>
                    <!-- Disk I/O -->
                    <div class="core-io-row">
                      <span class="core-metric-label">I/O</span>
                      <span class="core-io-val">
                        <span class="io-read"
                          >↑ {formatBytes(stats.blkio_read_bytes)}</span
                        >
                        <span class="io-write"
                          >↓ {formatBytes(stats.blkio_write_bytes)}</span
                        >
                      </span>
                    </div>
                  </div>
                {:else if state === "running" && !coreLoading}
                  <div class="core-status-text muted">Stats unavailable</div>
                {/if}
              </div>
            {/each}
          </div>
        </Card>
      </div>

      <!-- Disk section -->
      <div class="block">
        <div class="sec-head">
          <HardDrive size={14} />
          <SectionLabel>Disk</SectionLabel>
        </div>
        <Card padding="0">
          <div class="disk-list">
            {#each deduplicateDisks(info.disks) as disk}
              <div class="disk-row">
                <div class="disk-meta">
                  <span class="disk-mount mono">{disk.mount}</span>
                  <span class="disk-size"
                    >{disk.used_gb.toFixed(1)} GB / {disk.total_gb.toFixed(1)} GB</span
                  >
                </div>
                <div class="disk-gauge-wrap">
                  <div class="disk-track">
                    <ProgressBar
                      value={disk.used_pct}
                      tone={gaugeTone(disk.used_pct)}
                    />
                  </div>
                  <span
                    class="disk-pct"
                    style="color: {gaugeColor(disk.used_pct)}"
                    >{disk.used_pct.toFixed(1)}%</span
                  >
                </div>
              </div>
            {/each}
          </div>
        </Card>
      </div>

      <!-- Network section -->
      <div class="block">
        <div class="sec-head">
          <Network size={14} />
          <SectionLabel>Network</SectionLabel>
          <span class="section-hint">cumulative since boot</span>
        </div>
        <Card padding="0">
          <div class="table-wrap">
            <table class="data-table">
              <thead>
                <tr>
                  <th>Interface</th>
                  <th>Received</th>
                  <th>Transmitted</th>
                </tr>
              </thead>
              <tbody>
                {#each visibleNets(info.networks) as net}
                  <tr>
                    <td class="mono iface">{net.iface}</td>
                    <td class="rx">{formatBytes(net.rx_bytes)}</td>
                    <td class="tx">{formatBytes(net.tx_bytes)}</td>
                  </tr>
                {/each}
              </tbody>
            </table>
          </div>
        </Card>
      </div>
    {/if}

    <!-- Swarm Nodes (always rendered, not gated by metrics stream) -->
    <div class="block">
      <div class="sec-head">
        <Server size={14} />
        <SectionLabel>Swarm Nodes</SectionLabel>
        <div class="sec-action sec-action-auto">
          <Button
            variant="secondary"
            size="icon"
            onclick={loadNodes}
            disabled={nodesLoading}
            title="Refresh"
            aria-label="Refresh swarm nodes"
          >
            <RefreshCw size={13} />
          </Button>
        </div>
      </div>

      {#if nodesLoading}
        <div class="nodes-placeholder">
          <Spinner size={18} />
          Loading nodes…
        </div>
      {:else if nodesError}
        <div role="alert"><InlineAlert tone="error">{nodesError}</InlineAlert></div>
      {:else if nodes.length === 0}
        <div class="nodes-placeholder">
          Not running in swarm mode, or no nodes visible.
        </div>
      {:else}
        <DataTable
          items={nodes}
          rowKey={(n) => n.hostname}
          searchable={false}
          columns={[
            { key: "hostname", label: "Hostname" },
            { key: "role", label: "Role" },
            { key: "status", label: "Status" },
            { key: "availability", label: "Availability" },
            { key: "addr", label: "Address" },
            { key: "engine_version", label: "Engine" },
          ]}
        >
          {#snippet row(node)}
            <tr>
              <td class="mono">{node.hostname}</td>
              <td>
                <Badge tone={node.role === "manager" ? "blue" : "neutral"}
                  >{node.role}</Badge
                >
              </td>
              <td>
                <span class="node-status">
                  <StatusDot status={nodeDot(node.status)} />
                  {node.status}
                </span>
              </td>
              <td>
                <Badge tone={availTone(node.availability)}
                  >{node.availability}</Badge
                >
              </td>
              <td class="mono muted">{node.addr ?? "—"}</td>
              <td class="mono muted">{node.engine_version ?? "—"}</td>
            </tr>
          {/snippet}
        </DataTable>
      {/if}
    </div>

    <!-- Swarm Join Tokens -->
    <div class="block">
      <div class="sec-head">
        <Link size={14} />
        <SectionLabel>Join Tokens</SectionLabel>
        <span class="section-hint">Run on a new VPS to add it to this swarm</span>
        <div class="sec-action">
          <Button
            variant="secondary"
            size="icon"
            onclick={loadJoinTokens}
            disabled={joinTokensLoading}
            title="Refresh"
            aria-label="Refresh join tokens"
          >
            <RefreshCw size={13} />
          </Button>
        </div>
      </div>

      <Card padding="0">
        <!-- Guided setup banner -->
        <div class="setup-banner">
          <div class="setup-banner-text">
            <span class="setup-banner-title">New to this?</span>
            Run the guided worker setup script on your new VPS — it installs Docker,
            configures registry credentials, and joins the swarm in one go.
          </div>
          <div class="setup-banner-cmd-wrap">
            <code class="setup-banner-cmd"
              >curl -fsSL {typeof window !== "undefined"
                ? window.location.origin
                : ""}/worker-setup.sh | sudo bash</code
            >
            <Button
              variant="secondary"
              size="icon"
              onclick={async () => {
                await navigator.clipboard.writeText(
                  `curl -fsSL ${window.location.origin}/worker-setup.sh | sudo bash`,
                );
                copiedToken = "worker";
                setTimeout(() => (copiedToken = null), 2000);
              }}
              title="Copy setup command"
              aria-label="Copy setup command"
            >
              {#if copiedToken === "worker"}
                <Check size={13} />
              {:else}
                <Copy size={13} />
              {/if}
            </Button>
          </div>
        </div>

        {#if joinTokensLoading}
          <div class="nodes-placeholder">
            <Spinner size={18} />
            Loading…
          </div>
        {:else if joinTokensError}
          <div class="nodes-placeholder">
            <div role="alert"><InlineAlert tone="error">{joinTokensError}</InlineAlert></div>
          </div>
        {:else if joinTokens}
          <div class="token-list">
            {#each [["worker", joinTokens.worker], ["manager", joinTokens.manager]] as [role, token]}
              <div class="token-row">
                <div class="token-meta">
                  <Badge tone={role === "manager" ? "blue" : "neutral"}
                    >{role}</Badge
                  >
                  <span class="token-hint">
                    {role === "worker"
                      ? "Runs workloads — no scheduling control"
                      : "Full cluster control — use sparingly"}
                  </span>
                </div>
                <div class="token-cmd-wrap">
                  <code class="token-cmd"
                    >docker swarm join --token {token} {joinTokens.addr}</code
                  >
                  <Button
                    variant="secondary"
                    size="icon"
                    onclick={() =>
                      copyToken(role === "manager" ? "manager" : "worker")}
                    title="Copy command"
                    aria-label="Copy {role} join command"
                  >
                    {#if copiedToken === role}
                      <Check size={13} />
                    {:else}
                      <Copy size={13} />
                    {/if}
                  </Button>
                </div>
              </div>
            {/each}
          </div>
        {/if}
      </Card>
    </div>
  </div>
{/if}

<style>
  .infra-page {
    display: flex;
    flex-direction: column;
    gap: 20px;
  }

  .page-toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .toolbar-left {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--text-secondary);
    font-size: 13px;
  }
  .toolbar-title {
    font-weight: 600;
    color: var(--text-primary);
  }
  .live-wrap.live :global(svg) {
    animation: pulse 2s ease-in-out infinite;
  }
  @keyframes pulse {
    0%,
    100% {
      opacity: 1;
    }
    50% {
      opacity: 0.4;
    }
  }

  .waiting {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 10px;
    padding: 60px;
    color: var(--text-muted);
    font-size: 13px;
  }

  /* Stat cards: StatCard with a ProgressBar footer */
  .stat-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
    gap: 16px;
  }
  .metric-card {
    display: flex;
    flex-direction: column;
  }
  .metric-card :global(.ui-stat-card) {
    border-bottom-left-radius: 0;
    border-bottom-right-radius: 0;
    border-bottom: none;
  }
  .metric-foot {
    background: var(--bg-surface);
    border: 1px solid var(--border);
    border-top: none;
    border-bottom-left-radius: var(--radius-lg);
    border-bottom-right-radius: var(--radius-lg);
    padding: 9px 14px 11px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .metric-sub {
    font-size: 11px;
    color: var(--text-muted);
  }

  /* Part 1 section header: icon + SectionLabel + hint */
  .sec-head {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 10px;
    color: var(--text-muted);
  }
  .sec-head :global(.ui-section-label) {
    margin-bottom: 0;
    white-space: nowrap;
  }
  .sec-action {
    margin-left: 0;
  }
  .sec-action-auto {
    margin-left: auto;
  }
  .section-hint {
    font-size: 11px;
    font-weight: 400;
    color: var(--text-muted);
    margin-left: auto;
  }
  .section-hint + .sec-action {
    margin-left: 0;
  }

  /* Disk */
  .disk-list {
    display: flex;
    flex-direction: column;
  }
  .disk-row {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 12px 16px;
    border-bottom: 1px solid var(--border);
  }
  .disk-row:last-child {
    border-bottom: none;
  }
  .disk-meta {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 160px;
  }
  .disk-mount {
    font-size: 13px;
    color: var(--text-primary);
  }
  .disk-size {
    font-size: 11px;
    color: var(--text-muted);
  }
  .disk-gauge-wrap {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .disk-track {
    flex: 1;
  }
  .disk-pct {
    font-size: 12px;
    font-weight: 600;
    min-width: 44px;
    text-align: right;
  }

  /* Network table */
  .table-wrap {
    overflow-x: auto;
  }
  .data-table {
    width: 100%;
    border-collapse: collapse;
    font-size: 13px;
  }
  .data-table thead th {
    padding: 10px 16px;
    text-align: left;
    font-size: 11px;
    font-weight: 600;
    color: var(--text-muted);
    background: var(--bg-elevated);
    border-bottom: 1px solid var(--border);
  }
  .data-table tbody tr td {
    padding: 10px 16px;
    border-bottom: 1px solid var(--border);
    color: var(--text-primary);
  }
  .data-table tbody tr:last-child td {
    border-bottom: none;
  }
  .data-table tbody tr:hover td {
    background: var(--bg-hover);
  }

  .mono {
    font-family: var(--font-mono, "JetBrains Mono", monospace);
    font-size: 12px;
  }
  .muted {
    color: var(--text-muted);
  }
  .iface {
    color: var(--text-secondary);
  }
  .rx {
    color: var(--accent-green);
  }
  .tx {
    color: var(--accent);
  }

  /* Swarm nodes */
  .nodes-placeholder {
    padding: 20px 16px;
    font-size: 13px;
    color: var(--text-muted);
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .node-status {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    font-weight: 500;
    color: var(--text-secondary);
    text-transform: capitalize;
  }

  /* Setup banner */
  .setup-banner {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px 16px;
    background: var(--accent-muted);
    border-bottom: 1px solid var(--border);
  }
  .setup-banner-text {
    font-size: 13px;
    color: var(--text-secondary);
    line-height: 1.5;
  }
  .setup-banner-title {
    font-weight: 600;
    color: var(--text-primary);
    margin-right: 6px;
  }
  .setup-banner-cmd-wrap {
    display: flex;
    align-items: center;
    gap: 10px;
    background: var(--bg-base);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    padding: 7px 12px;
  }
  .setup-banner-cmd {
    flex: 1;
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--accent);
    word-break: break-all;
    user-select: all;
  }

  /* Join tokens */
  .token-list {
    display: flex;
    flex-direction: column;
  }
  .token-row {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 14px 16px;
    border-bottom: 1px solid var(--border);
  }
  .token-row:last-child {
    border-bottom: none;
  }
  .token-meta {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .token-hint {
    font-size: 12px;
    color: var(--text-muted);
  }
  .token-cmd-wrap {
    display: flex;
    align-items: center;
    gap: 10px;
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    padding: 8px 12px;
  }
  .token-cmd {
    flex: 1;
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--text-secondary);
    word-break: break-all;
    user-select: all;
  }
  /* Core services */
  .core-error {
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 10px 16px;
    font-size: 12px;
    color: var(--accent-red);
    border-bottom: 1px solid var(--border);
  }
  .core-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
    gap: 0;
  }
  .core-card {
    padding: 14px 16px;
    border-right: 1px solid var(--border);
    border-bottom: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    gap: 4px;
    transition: background var(--transition-fast);
  }
  .core-card:hover {
    background: var(--bg-hover);
  }
  .core-card.running {
    border-left: 3px solid var(--accent-green);
  }
  .core-card.stopped {
    border-left: 3px solid var(--accent-red);
  }
  .core-card-header {
    display: flex;
    align-items: center;
    gap: 7px;
  }
  .core-label {
    font-size: 13px;
    font-weight: 600;
    color: var(--text-primary);
    flex: 1;
  }
  .core-state-badge {
    font-size: 10px;
    font-weight: 600;
  }
  .core-desc {
    font-size: 11px;
    color: var(--text-muted);
  }
  .core-image {
    font-size: 10px;
    font-family: var(--font-mono);
    color: var(--text-dim);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    margin-top: 4px;
  }
  .core-status-text {
    font-size: 11px;
    color: var(--text-muted);
  }
  .core-status-text.muted {
    color: var(--text-dim);
    font-style: italic;
  }

  /* Core card resource metrics */
  .core-metrics {
    display: flex;
    flex-direction: column;
    gap: 5px;
    margin-top: 8px;
    padding-top: 8px;
    border-top: 1px solid var(--border);
  }
  .core-metric-row {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .core-io-row {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .core-metric-label {
    font-size: 9px;
    font-weight: 700;
    color: var(--text-dim);
    width: 24px;
    flex-shrink: 0;
  }
  .core-gauge {
    flex: 1;
    min-width: 0;
  }
  .core-metric-val {
    font-size: 10px;
    font-weight: 600;
    font-family: var(--font-mono);
    min-width: 46px;
    text-align: right;
  }
  .core-io-val {
    display: flex;
    gap: 8px;
    font-size: 10px;
    font-family: var(--font-mono);
    color: var(--text-muted);
    flex-wrap: wrap;
  }
  .io-read {
    color: var(--accent-green);
  }
  .io-write {
    color: var(--accent);
  }
</style>
