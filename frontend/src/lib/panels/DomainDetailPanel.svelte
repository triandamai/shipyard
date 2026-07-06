<script lang="ts">
  import { onMount } from "svelte";
  import {
    Globe,
    Trash2,
    AlertTriangle,
    Check,
    X,
    RefreshCw,
    ExternalLink,
    Shield,
    ShieldOff,
  } from "@lucide/svelte";
  import { api } from "$lib/api/client";
  import { orgStore } from "$lib/stores/org.store";
  import { can, permProject } from "$lib/auth/permissions";
  import type { Domain, DnsCheckResult } from "$lib/api/types";

  interface Props {
    domainId: string;
    serviceId: string;
    projectId: string;
    onDeleted?: () => void;
  }

  let { domainId, serviceId, projectId, onDeleted }: Props = $props();

  let orgId = $derived($orgStore.activeOrg?.id ?? "");
  let canDomainWrite = $derived(
    can(
      $orgStore.myMembership?.role ?? null,
      $orgStore.myMembership?.permissions ?? [],
      permProject(orgId, projectId, "domain", "write"),
    ),
  );

  let domain = $state<Domain | null>(null);
  let loading = $state(true);
  let loadError = $state("");

  let dnsStatus = $state<"idle" | "checking" | "ok" | "fail">("idle");
  let dnsResult = $state<DnsCheckResult | null>(null);

  let showDelete = $state(false);
  let deleteInput = $state("");
  let deleting = $state(false);
  let deleteError = $state("");

  let canDelete = $derived(deleteInput.trim() === (domain?.hostname ?? ""));

  onMount(async () => {
    const res = await api.get<Domain[]>(`/services/${serviceId}/domains`);
    if (res.error) {
      loadError = res.error.message;
    } else if (res.data) {
      domain = res.data.find((d) => d.id === domainId) ?? null;
      if (!domain) loadError = "Domain not found.";
    }
    loading = false;
  });

  async function checkDns() {
    if (!domain) return;
    dnsStatus = "checking";
    dnsResult = null;
    const res = await api.checkDomainDns(serviceId, domainId);
    if (res.data) {
      dnsResult = res.data;
      dnsStatus = res.data.resolves ? "ok" : "fail";
    } else {
      dnsStatus = "fail";
    }
  }

  async function deleteDomain() {
    if (!canDelete) return;
    deleting = true;
    deleteError = "";
    const res = await api.deleteDomain(serviceId, domainId);
    deleting = false;
    if (res.error) {
      deleteError = res.error.message;
    } else {
      onDeleted?.();
    }
  }

  function openExternal() {
    if (!domain) return;
    const protocol = domain.tls_enabled ? "https" : "http";
    const portSuffix =
      domain.port && domain.port !== (domain.tls_enabled ? 443 : 80)
        ? `:${domain.port}`
        : "";
    window.open(`${protocol}://${domain.hostname}`, "_blank", "noopener");
  }
</script>

<div class="panel-body">
  {#if loading}
    <div class="loading-row">
      <div class="spinner-sm"></div>
       Loading…
    </div>
  {:else if loadError}
    <div class="error-msg">{loadError}</div>
  {:else if domain}
    <!-- ── Overview ── -->
    <section class="section">
      <div class="domain-hero">
        <div class="hero-icon" class:tls={domain.tls_enabled}>
          <Globe size={18} />
        </div>
        <div class="hero-info">
          <span class="hero-hostname">{domain.hostname}</span>
          <div class="hero-badges">
            {#if domain.tls_enabled}
              <span class="badge badge-green"
                ><Shield size={10} /> TLS enabled</span
              >
            {:else}
              <span class="badge badge-muted"
                ><ShieldOff size={10} /> No TLS</span
              >
            {/if}
            {#if domain.port}
              <span class="badge badge-blue">→ :{domain.port}</span>
            {/if}
          </div>
        </div>
        <button class="icon-btn" title="Open in browser" onclick={openExternal}>
          <ExternalLink size={14} />
        </button>
      </div>
    </section>

    <!-- ── Details ── -->
    <section class="section">
      <h3 class="section-title">Details</h3>
      <dl class="detail-list">
        <div class="detail-row">
          <dt>Protocol</dt>
          <dd>{domain.tls_enabled ? "HTTPS" : "HTTP"}</dd>
        </div>
        {#if domain.port}
          <div class="detail-row">
            <dt>Target port</dt>
            <dd class="mono">:{domain.port}</dd>
          </div>
        {/if}
        <div class="detail-row">
          <dt>TLS provider</dt>
          <dd>{domain.cert_provider || "—"}</dd>
        </div>
        <div class="detail-row">
          <dt>Router name</dt>
          <dd class="mono">{domain.traefik_router_name || "—"}</dd>
        </div>
        <div class="detail-row">
          <dt>Created</dt>
          <dd>{new Date(domain.created_at).toLocaleString()}</dd>
        </div>
      </dl>
    </section>

    <!-- ── DNS check ── -->
    <section class="section">
      <div class="section-head">
        <h3 class="section-title">DNS</h3>
        <button
          class="btn btn-secondary btn-sm"
          disabled={dnsStatus === "checking"}
          onclick={checkDns}
        >
          <RefreshCw size={12} class={dnsStatus === "checking" ? "spin" : ""} />
          {dnsStatus === "checking" ? "Checking…" : "Check DNS"}
        </button>
      </div>

      {#if dnsStatus === "ok" && dnsResult}
        <div class="dns-result dns-ok">
          <Check size={13} />
          <span>Resolves — {dnsResult.addresses.join(", ")}</span>
        </div>
      {:else if dnsStatus === "fail"}
        <div class="dns-result dns-fail">
          <X size={13} />
          <span
            >{dnsResult
              ? `Does not resolve (${domain.hostname})`
              : "Check failed"}</span
          >
        </div>
      {:else if dnsStatus === "idle"}
        <p class="hint">
          Run a DNS check to verify {domain.hostname} points to this server.
        </p>
      {/if}
    </section>

    <!-- ── Danger zone ── -->
    {#if canDomainWrite}
      <section class="section danger-section">
        <h3 class="section-title danger-title">Danger zone</h3>
        {#if !showDelete}
          <button
            class="btn btn-danger btn-sm"
            onclick={() => (showDelete = true)}
          >
            <Trash2 size={13} /> Remove domain
          </button>
        {:else}
          <p class="confirm-hint">
            Type <strong>{domain.hostname}</strong> to confirm removal.
          </p>
          <input
            class="input"
            placeholder={domain.hostname}
            bind:value={deleteInput}
          />
          {#if deleteError}
            <p class="error-msg">{deleteError}</p>
          {/if}
          <div class="confirm-actions">
            <button
              class="btn btn-secondary btn-sm"
              onclick={() => {
                showDelete = false;
                deleteInput = "";
              }}
            >
              Cancel
            </button>
            <button
              class="btn btn-danger btn-sm"
              disabled={!canDelete || deleting}
              onclick={deleteDomain}
            >
              {deleting ? "Removing…" : "Remove"}
            </button>
          </div>
        {/if}
      </section>
    {/if}
  {/if}
</div>

<style>
  .panel-body {
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 20px;
  }

  .loading-row {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--text-muted);
    font-size: 13px;
    padding: 24px 0;
    justify-content: center;
  }

  .spinner-sm {
    width: 14px;
    height: 14px;
    border: 2px solid var(--border);
    border-top-color: var(--accent);
    border-radius: 50%;
    animation: spin 0.7s linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  .error-msg {
    font-size: 12px;
    color: var(--accent-red);
    padding: 8px 0;
  }

  /* ── Domain hero ── */
  .domain-hero {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px;
    background: var(--bg-elevated);
    border-radius: var(--radius-md);
    border: 1px solid var(--border);
  }

  .hero-icon {
    width: 36px;
    height: 36px;
    border-radius: var(--radius-sm);
    background: var(--bg-surface);
    color: var(--text-muted);
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    border: 1px solid var(--border);
  }

  .hero-icon.tls {
    background: var(--accent-green-muted);
    color: var(--accent-green);
    border-color: transparent;
  }

  .hero-info {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .hero-hostname {
    font-size: 13px;
    font-weight: 600;
    color: var(--text-primary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .hero-badges {
    display: flex;
    align-items: center;
    gap: 4px;
    flex-wrap: wrap;
  }

  .badge {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    font-size: 10px;
    font-weight: 500;
    padding: 2px 6px;
    border-radius: 100px;
  }

  .badge-green {
    background: var(--accent-green-muted);
    color: var(--accent-green);
  }

  .badge-muted {
    background: var(--bg-elevated);
    color: var(--text-muted);
    border: 1px solid var(--border);
  }

  .badge-blue {
    background: var(--accent-blue-muted);
    color: var(--accent-blue);
    font-family: var(--font-mono);
  }

  /* ── Sections ── */
  .section {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .section-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .section-title {
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-muted);
    margin: 0;
  }

  /* ── Detail list ── */
  .detail-list {
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 0;
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    overflow: hidden;
  }

  .detail-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 12px;
    font-size: 12px;
    border-bottom: 1px solid var(--border);
  }

  .detail-row:last-child {
    border-bottom: none;
  }

  .detail-row dt {
    color: var(--text-muted);
    font-weight: 500;
  }

  .detail-row dd {
    color: var(--text-primary);
    margin: 0;
  }

  .mono {
    font-family: var(--font-mono);
    font-size: 11px;
  }

  /* ── DNS ── */
  .dns-result {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    padding: 8px 10px;
    border-radius: var(--radius-sm);
  }

  .dns-ok {
    background: var(--accent-green-muted);
    color: var(--accent-green);
  }
  .dns-fail {
    background: color-mix(in srgb, var(--accent-red) 10%, transparent);
    color: var(--accent-red);
  }

  .hint {
    font-size: 12px;
    color: var(--text-muted);
    margin: 0;
  }

  /* ── Danger zone ── */
  .danger-section {
    border: 1px solid color-mix(in srgb, var(--accent-red) 30%, transparent);
    border-radius: var(--radius-md);
    padding: 14px;
    background: color-mix(in srgb, var(--accent-red) 4%, transparent);
  }

  .danger-title {
    color: var(--accent-red);
  }

  .confirm-hint {
    font-size: 12px;
    color: var(--text-muted);
    margin: 0;
  }

  .confirm-hint strong {
    color: var(--text-primary);
  }

  .input {
    width: 100%;
    padding: 7px 10px;
    font-size: 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--bg-surface);
    color: var(--text-primary);
    font-family: var(--font-mono);
    box-sizing: border-box;
  }

  .input:focus {
    outline: none;
    border-color: var(--accent);
  }

  .confirm-actions {
    display: flex;
    gap: 8px;
    justify-content: flex-end;
  }

  .icon-btn {
    background: none;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--text-muted);
    cursor: pointer;
    padding: 5px;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: all var(--transition-fast);
  }

  .icon-btn:hover {
    background: var(--bg-elevated);
    color: var(--text-primary);
  }

  :global(.spin) {
    animation: spin 0.7s linear infinite;
  }
</style>
