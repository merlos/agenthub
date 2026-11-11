import { useEffect, useRef, useState } from "react";
import { ArrowLeft, BarChart3 } from "lucide-react";
import { Link, useParams } from "react-router-dom";
import { api } from "./api";
import type { ProviderStats, TokenBucket, TokenSummary } from "./types";

const number = (value: number) => new Intl.NumberFormat().format(value);
const decimal = (value: number) => new Intl.NumberFormat(undefined, { maximumFractionDigits: 1 }).format(value);

function Summary({ title, subtitle, usage }: { title: string; subtitle: string; usage: TokenSummary }) {
  const metrics = [
    ["Input tokens", number(usage.tokensIn)],
    ["Output tokens", number(usage.tokensOut)],
    ["Total tokens", number(usage.totalTokens)],
    ["Avg. tokens/request", decimal(usage.averageTokens)],
    ["Avg. output tok/s", decimal(usage.averageTokensPerSecond)],
  ];
  return <section className="usage-summary" aria-label={title}>
    <h2>{title}</h2><p className="usage-help">{subtitle}</p>
    <div className="usage-metrics">{metrics.map(([label, value]) => <article className="metric" key={label}><div><span>{label}</span><strong>{value}</strong></div><BarChart3 className="usage-icon" aria-hidden="true"/></article>)}</div>
  </section>;
}

export function TokenChart({ buckets, daily = false }: { buckets: TokenBucket[]; daily?: boolean }) {
  const container = useRef<HTMLDivElement>(null);
  const [days, setDays] = useState(30);
  const [active, setActive] = useState<string>();
  useEffect(() => {
    if (!daily || !container.current) return;
    const resize = (width: number) => setDays(Math.max(7, Math.min(30, Math.floor(width / 32))));
    resize(container.current.getBoundingClientRect().width);
    const observer = new ResizeObserver(entries => resize(entries[0].contentRect.width));
    observer.observe(container.current);
    return () => observer.disconnect();
  }, [daily]);
  const visible = daily ? buckets.slice(-days) : buckets;
  const peak = Math.max(1, ...visible.map(bucket => bucket.totalTokens));
  const selected = visible.find(bucket => bucket.date === active);
  return <section className="panel token-chart">
    <div className="panel-title"><div><h2>{daily ? `Last ${visible.length} days` : "Last 12 months"}</h2><p>{daily ? "Daily consumption · UTC" : "Calendar-month consumption · current month is partial"}</p></div></div>
    <div className="token-legend"><span><i className="tokens-in"/>Input tokens</span><span><i className="tokens-out"/>Output tokens</span></div>
    <div className="token-bars" ref={container} role="group" aria-label={daily ? "Daily token consumption" : "Monthly token consumption"}>
      {visible.map(bucket => <button className="token-bar" key={bucket.date}
        aria-label={`${bucket.date}: ${number(bucket.tokensIn)} input tokens, ${number(bucket.tokensOut)} output tokens, ${number(bucket.totalTokens)} total tokens${daily ? `, ${decimal(bucket.averageTokensPerSecond)} average output tokens/s` : ""}`}
        onMouseEnter={() => setActive(bucket.date)} onMouseLeave={() => setActive(undefined)}
        onFocus={() => setActive(bucket.date)} onBlur={() => setActive(undefined)} onClick={() => setActive(bucket.date)}>
        <span className="token-stack" aria-hidden="true"><span className="tokens-out" style={{ height: `${bucket.tokensOut / peak * 100}%` }}/><span className="tokens-in" style={{ height: `${bucket.tokensIn / peak * 100}%` }}/></span>
        <small aria-hidden="true">{daily ? bucket.date.slice(5) : bucket.date.slice(2)}</small>
      </button>)}
    </div>
    <div className="token-detail" aria-live="polite">{selected ? <><b>{selected.date}</b><span>Input: {number(selected.tokensIn)}</span><span>Output: {number(selected.tokensOut)}</span><span>Total: {number(selected.totalTokens)}</span>{daily && <span>Avg. output tok/s: {decimal(selected.averageTokensPerSecond)}</span>}</> : <span>Hover, focus, or tap a bar for exact token counts{daily ? " and average output tokens/s" : ""}.</span>}</div>
  </section>;
}

export default function ProviderStatsPage() {
  const { id = "" } = useParams();
  const [stats, setStats] = useState<ProviderStats>();
  const [error, setError] = useState("");
  const [saving, setSaving] = useState(false);
  const [message, setMessage] = useState("");
  const generation = useRef(0);
  useEffect(() => {
    const current = ++generation.current;
    setStats(undefined); setError(""); setMessage(""); setSaving(false);
    api.providerStats(id).then(value => { if (generation.current === current) setStats(value); })
      .catch(e => { if (generation.current === current) setError(String(e)); });
    return () => { generation.current++; };
  }, [id]);
  const save = async (day: number) => {
    const current = generation.current;
    setSaving(true); setError(""); setMessage("");
    try {
      await api.setProviderBillingStartDay(id, day);
      const value = await api.providerStats(id);
      if (generation.current === current) { setStats(value); setMessage("Billing start day saved."); }
    } catch (e) { if (generation.current === current) setError(String(e)); }
    finally { if (generation.current === current) setSaving(false); }
  };
  return <>
    <Link className="secondary stats-back" to="/providers"><ArrowLeft aria-hidden="true"/>Providers</Link>
    <header><div><h1>{stats ? `${stats.providerName} statistics` : "Provider statistics"}</h1><p>Token consumption recorded through agenthub</p></div></header>
    {error && <div className="error" role="alert">{error}</div>}
    {!stats && !error && <p role="status">Loading statistics…</p>}
    {stats && <>
      <section className="panel billing-settings"><label htmlFor="billing-start">Billing period starts on day</label>
        <select id="billing-start" value={stats.billingStartDay} disabled={saving} onChange={event => void save(Number(event.target.value))}>
          {Array.from({ length: 31 }, (_, index) => index + 1).map(day => <option key={day} value={day}>{day}</option>)}
        </select><p>Saved per provider. Shorter months use their last day. All dates use UTC.</p>
        <span role="status">{saving ? "Saving…" : message}</span>
      </section>
      <Summary title="Current billing period" subtitle={`${stats.periodStart} – ${stats.periodEnd} · ${number(stats.current.requests)} requests`} usage={stats.current}/>
      <Summary title="Historical usage" subtitle={`All retained request history · ${number(stats.historical.requests)} requests`} usage={stats.historical}/>
      <p className="usage-help">Only traffic through agenthub is counted. Pruned history is unavailable; providers that omit usage report zero tokens. Average tokens/request includes all recorded requests. Output tok/s averages per-request output tokens ÷ total duration, excluding zero-duration requests.</p>
      <TokenChart buckets={stats.daily} daily/>
      <TokenChart buckets={stats.monthly}/>
    </>}
  </>;
}
