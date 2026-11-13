import { useCallback, useEffect, useState } from "react";
import { Activity, AppWindow, BarChart3, Box, Check, Copy, KeyRound, Plus, Power, RefreshCw, Search, Server, Settings as SettingsIcon, ShieldCheck, Trash2, X } from "lucide-react";
import { Link, NavLink, Route, Routes } from "react-router-dom";
import { api } from "./api";
import type { Application, DashboardStats, ModelAccess, Provider, ProviderKind, ProviderUsage, Settings } from "./types";
import ProviderStatsPage from "./ProviderStatsPage";

const providerDefaults: Record<ProviderKind, { url: string; model: string; hint: string }> = {
  openai: { url: "https://api.openai.com/v1", model: "gpt-4o-mini", hint: "Create a secret key in platform.openai.com/api-keys." },
  anthropic: { url: "https://api.anthropic.com/v1", model: "claude-3-5-sonnet-latest", hint: "Create a key in the Anthropic Console under Settings → API Keys." },
  google: { url: "https://generativelanguage.googleapis.com/v1beta", model: "gemini-2.0-flash", hint: "Get a Gemini API key from Google AI Studio." },
  azure: { url: "https://YOUR-RESOURCE.openai.azure.com", model: "deployment-name", hint: "Use the endpoint and key from Azure AI Foundry. Model must be your deployment name." },
  aws: { url: "https://bedrock-runtime.us-east-1.amazonaws.com/openai/v1", model: "openai.gpt-oss-20b-1:0", hint: "Create a long-term Amazon Bedrock API key in the AWS console and choose the endpoint region you use." },
  openrouter: { url: "https://openrouter.ai/api/v1", model: "openai/gpt-4o-mini", hint: "Create a key at openrouter.ai/settings/keys." },
  deepseek: { url: "https://api.deepseek.com/v1", model: "deepseek-chat", hint: "Create a key in platform.deepseek.com/api_keys." },
  ollama: { url: "http://localhost:11434/v1", model: "llama3.2", hint: "Ollama runs locally and does not require an API key." },
  compatible: { url: "http://localhost:11434/v1", model: "", hint: "Enter an OpenAI-compatible /v1 base URL. The key may be empty for local services." },
};

const providerNeedsKey = (kind: ProviderKind) => kind !== "ollama" && kind !== "compatible";

function Layout() {
  return <div className="shell">
    <aside>
      <div className="brand"><div className="brand-mark"><Box size={20}/></div><div><b>agenthub</b><small>AI gateway</small></div></div>
      <nav>
        <NavLink to="/"><BarChart3/>Overview</NavLink>
        <NavLink to="/applications"><AppWindow/>Applications</NavLink>
        <NavLink to="/providers"><Server/>Providers</NavLink>
        <NavLink to="/logs"><Activity/>Request logs</NavLink>
        <NavLink to="/settings"><SettingsIcon/>Settings</NavLink>
      </nav>
      <div className="secure"><ShieldCheck/><div><b>Local & secure</b><small>Secrets stay on this device</small></div></div>
    </aside>
    <main><Routes><Route path="/" element={<Dashboard/>}/><Route path="/applications" element={<Applications/>}/><Route path="/providers" element={<Providers/>}/><Route path="/providers/:id/stats" element={<ProviderStatsPage/>}/><Route path="/logs" element={<Logs/>}/><Route path="/settings" element={<SettingsPage/>}/></Routes></main>
  </div>;
}

const Header = ({ title, subtitle, action }: { title: string; subtitle: string; action?: React.ReactNode }) => <header><div><h1>{title}</h1><p>{subtitle}</p></div>{action}</header>;
const Empty = ({ icon: Icon, title, text }: { icon: typeof Server; title: string; text: string }) => <div className="empty"><Icon/><h3>{title}</h3><p>{text}</p></div>;
const ErrorBox = ({ message }: { message: string }) => message ? <div className="error">{message}</div> : null;
const number = (value: number) => new Intl.NumberFormat().format(value);
const formatDate = (value: string) => value.slice(0, 10);
const formatDuration = (milliseconds: number) => {
  if (milliseconds < 1_000) return `${Math.round(milliseconds)}ms`;
  const seconds = Math.round(milliseconds / 1_000);
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.floor(seconds / 60), remainder = seconds % 60;
  return remainder ? `${minutes}m ${remainder}s` : `${minutes}m`;
};

function Dashboard() {
  const [stats, setStats] = useState<DashboardStats>();
  const [error, setError] = useState("");
  useEffect(() => { api.stats().then(setStats).catch(e => setError(String(e))); }, []);
  const cards = [
    ["Requests", number(stats?.requests ?? 0), Activity],
    ["Success rate", `${(stats?.successRate ?? 0).toFixed(1)}%`, Check],
    ["Input tokens", number(stats?.tokensIn ?? 0), BarChart3],
    ["Output tokens", number(stats?.tokensOut ?? 0), BarChart3],
    ["Avg. duration", formatDuration(stats?.averageDurationMs ?? 0), Server],
    ["P95 duration", formatDuration(stats?.p95DurationMs ?? 0), Activity],
  ] as const;
  const peak = Math.max(1, ...stats?.daily.map(item => item.requests) ?? []);
  return <><Header title="Overview" subtitle="Your AI gateway at a glance"/><ErrorBox message={error}/><section className="metrics">{cards.map(([label,value,Icon]) => <article className="metric" key={label}><div><span>{label}</span><strong>{value}</strong></div><Icon/></article>)}</section>{stats?.daily.length ? <section className="panel trend"><div className="panel-title"><div><h2>30-day traffic</h2><p>Requests per day</p></div></div><div className="bars">{stats.daily.map(day => <div key={day.day} title={`${day.day}: ${day.requests}`}><span style={{height: day.requests ? `${Math.max(8, day.requests / peak * 100)}%` : 0}}/><small>{day.day.slice(5)}</small></div>)}</div></section> : null}<section className="panel"><div className="panel-title"><div><h2>Recent requests</h2><p>Latest traffic across all applications</p></div></div>{stats?.recent.length ? <RequestTable rows={stats.recent}/> : <Empty icon={Activity} title="No requests yet" text="Requests made through an application will appear here."/>}</section></>;
}

function Applications() {
  const [items, setItems] = useState<Application[]>([]), [providers, setProviders] = useState<Provider[]>([]), [editing, setEditing] = useState<Partial<Application>>(), [error, setError] = useState("");
  const reload = useCallback(() => Promise.all([api.applications(), api.providers()]).then(([a,p]) => { setItems(a); setProviders(p); }).catch(e => setError(String(e))), []);
  useEffect(() => { void reload(); }, [reload]);
  const save = async (connection: Pick<Application, "name" | "providerId" | "modelAccess" | "allowedModels">) => { try { await api.saveApplication({ ...editing, ...connection }); setEditing(undefined); await reload(); } catch(e) { setError(String(e)); } };
  const remove = async (id: string) => { if (confirm("Delete this application and stop its local endpoint?")) { await api.deleteApplication(id); await reload(); } };
  const update = async (application: Application) => { try { await api.saveApplication(application); await reload(); } catch(e) { setError(String(e)); } };
  const regenerate = async (id: string) => { if (confirm("Regenerate this key? Existing client configuration will stop working.")) { try { await api.regenerateKey(id); await reload(); } catch(e) { setError(String(e)); } } };
  return <><Header title="Applications" subtitle="Dedicated, authenticated endpoints for your tools" action={<button className="primary" onClick={() => setEditing({})}><Plus/>Add application</button>}/><ErrorBox message={error}/><div className="cards">{items.map(app => <article className="app-card" key={app.id}><div className="row"><div className="app-icon"><AppWindow/></div><div className="grow"><h3>{app.name}</h3><span className={app.enabled ? "status good" : "status"}>{app.enabled ? "Running" : "Stopped"}</span></div><button className="icon" aria-label={app.enabled ? "Stop" : "Start"} title={app.enabled ? "Stop endpoint" : "Start endpoint"} onClick={() => void update({...app, enabled: !app.enabled})}><Power/></button><button className="icon danger" aria-label="Delete" onClick={() => void remove(app.id)}><Trash2/></button></div><dl><div><dt>Base URL</dt><dd><code>http://localhost:{app.port}/v1</code><CopyButton value={`http://localhost:${app.port}/v1`}/></dd></div><div><dt>API key</dt><dd><code>{app.apiKey.slice(0, 8)}••••••••</code><CopyButton value={app.apiKey}/><button className="copy" aria-label="Regenerate key" title="Regenerate key" onClick={() => void regenerate(app.id)}><RefreshCw/></button></dd></div><div><dt>Provider</dt><dd>{providers.find(p => p.id === app.providerId)?.name ?? "Default provider"}</dd></div></dl><button className="secondary full" onClick={() => setEditing(app)}>Edit connection</button></article>)}</div>{!items.length && <Empty icon={AppWindow} title="Connect your first application" text="Each application receives an isolated port and API key."/>}{editing && <ApplicationDialog app={editing} providers={providers} onClose={() => setEditing(undefined)} onSave={save}/>}</>;
}

function ApplicationDialog({ app, providers, onClose, onSave }: { app: Partial<Application>; providers: Provider[]; onClose: () => void; onSave: (connection: Pick<Application, "name" | "providerId" | "modelAccess" | "allowedModels">) => void }) {
  const [name, setName] = useState(app.name ?? ""), [provider, setProvider] = useState(app.providerId ?? "");
  const [modelAccess, setModelAccess] = useState<ModelAccess>(app.modelAccess ?? "defaultOnly"), [allowedModels, setAllowedModels] = useState<string[]>(app.allowedModels ?? []), [models, setModels] = useState<string[]>([]), [loadingModels, setLoadingModels] = useState(false), [modelError, setModelError] = useState("");
  const effectiveProvider = provider ? providers.find(p => p.id === provider) : providers.find(p => p.isDefault) ?? providers[0];
  const loadModels = async () => {
    setLoadingModels(true); setModelError("");
    try { setModels(await api.providerModels(effectiveProvider!)); }
    catch(e) { setModelError(String(e)); }
    finally { setLoadingModels(false); }
  };
  const choices = [...new Set([...models, ...allowedModels])].sort();
  return <Modal title={app.id ? "Edit application" : "Add application"} onClose={onClose}>
    <form onSubmit={e => { e.preventDefault(); void onSave({ name: name.trim(), providerId: provider || null, modelAccess, allowedModels: modelAccess === "selected" ? allowedModels : [] }); }}>
      <label>Application name<input autoFocus required value={name} onChange={e => setName(e.target.value)} placeholder="e.g. OpenCode"/></label>
      <label>Provider<select disabled={loadingModels} value={provider} onChange={e => { setProvider(e.target.value); setModels([]); setAllowedModels([]); setModelError(""); }}><option value="">Use default provider</option>{providers.map(p => <option key={p.id} value={p.id}>{p.name}</option>)}</select></label>
      <label>Model access<select value={modelAccess} onChange={e => setModelAccess(e.target.value as ModelAccess)}>
        <option value="defaultOnly">Default model only</option><option value="selected">Selected models</option><option value="all">All provider models</option>
      </select></label>
      {modelAccess === "defaultOnly" && <p className="help">Only the provider's default model is allowed. Other model requests are blocked.</p>}
      {modelAccess === "all" && <p className="help">Allow all models returned by the provider, including newly added models. Model discovery must succeed before requests are forwarded.</p>}
      {modelAccess === "selected" && <>
        <button type="button" className="secondary" disabled={loadingModels || !effectiveProvider} onClick={() => void loadModels()}>{loadingModels ? "Loading models…" : "Load models"}</button>
        <ErrorBox message={modelError}/>
        <fieldset className="model-choices"><legend>Enabled models</legend>{choices.map(model => <label key={model}><input type="checkbox" checked={allowedModels.includes(model)} onChange={e => setAllowedModels(e.target.checked ? [...allowedModels, model] : allowedModels.filter(item => item !== model))}/>{model}</label>)}</fieldset>
        <p className="help">Only checked models are listed and allowed. Selecting none blocks all model requests. Changing providers clears the selection.</p>
      </>}
      <p className="help">A unique port and secure API key are generated automatically.</p><DialogActions onClose={onClose} label={app.id ? "Save changes" : "Create application"}/>
    </form>
  </Modal>;
}

function Providers() {
  const [items, setItems] = useState<Provider[]>([]), [stats, setStats] = useState<ProviderUsage[]>([]), [editing, setEditing] = useState<Partial<Provider>>(), [error, setError] = useState("");
  const reload = useCallback(() => Promise.all([api.providers().then(setItems), api.providerUsage().then(setStats)]).catch(e => setError(String(e))), []);
  useEffect(() => { void reload(); }, [reload]);
  const remove = async (id: string) => { if (confirm("Delete this provider? Applications using it will fall back to the default.")) { await api.deleteProvider(id); await reload(); } };
  return <><Header title="Providers" subtitle="Manage credentials and model endpoints" action={<button className="primary" onClick={() => setEditing({})}><Plus/>Add provider</button>}/><ErrorBox message={error}/><div className="provider-list">{items.map(p => { const usage = stats.find(item => item.providerId === p.id); return <article key={p.id}><div className="provider-logo">{p.name.slice(0,2).toUpperCase()}</div><div className="provider-details grow"><div className="row"><h3>{p.name}</h3>{p.isDefault && <span className="badge">Default</span>}</div><p>{p.kind} · {p.defaultModel}</p><small>{p.baseUrl}</small></div><dl className="provider-stats"><div><dt>Total tokens</dt><dd>{number(usage?.totalTokens ?? 0)}</dd></div><div title={usage ? `Billing period: ${usage.periodStart} – ${usage.periodEnd} (UTC)` : undefined}><dt>Month tokens</dt><dd>{number(usage?.monthTokens ?? 0)}</dd></div></dl><div className="provider-actions"><span className={p.hasSecret || !providerNeedsKey(p.kind) ? "status good" : "status warn"}>{p.hasSecret ? "Connected" : providerNeedsKey(p.kind) ? "No key" : "Local"}</span>{!p.isDefault && <button className="secondary" onClick={() => void api.setDefaultProvider(p.id).then(reload)}>Make default</button>}<Link className="icon" to={`/providers/${encodeURIComponent(p.id)}/stats`} aria-label={`Statistics for ${p.name}`} title="Provider statistics"><BarChart3/></Link><button className="icon" aria-label="Edit" onClick={() => setEditing(p)}><SettingsIcon/></button><button className="icon danger" aria-label="Delete" onClick={() => void remove(p.id)}><Trash2/></button></div></article>; })}</div>{!items.length && <Empty icon={Server} title="Add your first provider" text="Connect OpenAI, Anthropic, Google, Azure, AWS, OpenRouter, DeepSeek, Ollama, or any compatible endpoint."/>}{editing && <ProviderDialog provider={editing} onClose={() => setEditing(undefined)} onSave={async p => { try { await api.saveProvider(p); setEditing(undefined); await reload(); } catch(e) { setError(String(e)); } }}/>}</>;
}

function ProviderDialog({ provider, onClose, onSave }: { provider: Partial<Provider>; onClose: () => void; onSave: (p: Omit<Provider,"hasSecret"> & { apiKey?: string }) => void }) {
  const initialKind = provider.kind ?? "openai";
  const [kind, setKind] = useState<ProviderKind>(initialKind), [name, setName] = useState(provider.name ?? "OpenAI"), [url, setUrl] = useState(provider.baseUrl ?? providerDefaults[initialKind].url), [model, setModel] = useState(provider.defaultModel ?? providerDefaults[initialKind].model), [key, setKey] = useState(""), [models, setModels] = useState<string[]>([]), [loadingModels, setLoadingModels] = useState(false), [modelError, setModelError] = useState("");
  const changeKind = (next: ProviderKind) => { setKind(next); setName(next === "compatible" ? "Custom provider" : next[0].toUpperCase() + next.slice(1)); setUrl(providerDefaults[next].url); setModel(providerDefaults[next].model); setModels([]); setModelError(""); };
  const value = () => ({ id: provider.id ?? "", name: name.trim(), kind, baseUrl: url.replace(/\/$/, ""), defaultModel: model.trim(), isDefault: provider.isDefault ?? false, apiKey: key || undefined });
  const loadModels = async () => { setLoadingModels(true); setModelError(""); try { setModels(await api.providerModels(value())); } catch(e) { setModelError(String(e)); } finally { setLoadingModels(false); } };
  return <Modal title={provider.id ? "Edit provider" : "Add provider"} onClose={onClose}><form onSubmit={e => { e.preventDefault(); onSave(value()); }}><div className="two"><label>Provider type<select value={kind} onChange={e => changeKind(e.target.value as ProviderKind)}>{Object.keys(providerDefaults).map(k => <option key={k} value={k}>{k[0].toUpperCase()+k.slice(1)}</option>)}</select></label><label>Display name<input required value={name} onChange={e => setName(e.target.value)}/></label></div><label>Base URL<input required type="url" value={url} onChange={e => setUrl(e.target.value)}/></label><label>API key<input required={!provider.hasSecret && providerNeedsKey(kind)} type="password" value={key} onChange={e => setKey(e.target.value)} placeholder={provider.hasSecret ? "Leave blank to keep existing key" : providerNeedsKey(kind) ? "Paste API key" : "Optional"}/></label><label>Default model<input required list="provider-models" value={model} onChange={e => setModel(e.target.value)}/><datalist id="provider-models">{models.map(item => <option key={item} value={item}/>)}</datalist></label><button type="button" className="secondary" onClick={() => void loadModels()} disabled={loadingModels || (providerNeedsKey(kind) && !provider.hasSecret && !key)}>{loadingModels ? "Loading models…" : "Load models"}</button><ErrorBox message={modelError}/><div className="instruction"><KeyRound/><span>{providerDefaults[kind].hint} Keys are stored in your operating system keyring.</span></div><DialogActions onClose={onClose} label="Save provider"/></form></Modal>;
}

function Logs() {
  const [stats, setStats] = useState<DashboardStats>();
  const [filter, setFilter] = useState("");
  const [autoRefresh, setAutoRefresh] = useState(true);
  const [error, setError] = useState("");
  useEffect(() => {
    if (!autoRefresh) return;
    let active = true;
    let timer: ReturnType<typeof setTimeout>;
    const refresh = async () => {
      try {
        const next = await api.stats();
        if (active) { setStats(next); setError(""); }
      } catch (e) {
        if (active) setError(String(e));
      } finally {
        if (active) timer = setTimeout(() => void refresh(), 5_000);
      }
    };
    void refresh();
    return () => { active = false; clearTimeout(timer); };
  }, [autoRefresh]);
  const query = filter.trim().toLocaleLowerCase();
  const rows = stats?.recent.filter(row => !query || [formatDate(row.createdAt), row.application, row.provider, row.model, row.endpoint, row.modality, row.status, row.tokensIn, row.tokensOut, row.accumulatedTokensIn, row.accumulatedTokensOut, row.errorType ?? ""].some(value => String(value).toLocaleLowerCase().includes(query))) ?? [];
  return <><Header title="Request logs" subtitle="Metadata for requests passing through agenthub" action={<button className="secondary" aria-pressed={autoRefresh} onClick={() => setAutoRefresh(!autoRefresh)}><RefreshCw/>{autoRefresh ? "Disable auto-refresh" : "Enable auto-refresh"}</button>}/><ErrorBox message={error}/><section className="panel"><div className="log-tools"><Search aria-hidden="true"/><label><span className="sr-only">Filter requests</span><input type="search" value={filter} onChange={event => setFilter(event.target.value)} placeholder="Filter requests…"/></label></div>{rows.length ? <RequestTable rows={rows} showAccumulated/> : stats?.recent.length ? <Empty icon={Search} title="No matching requests" text="Try a different filter."/> : <Empty icon={Activity} title="No request metadata" text="Status, latency, token usage, and model information will appear here."/>}</section></>;
}

function SettingsPage() {
  const [settings, setSettings] = useState<Settings>();
  useEffect(() => { void api.settings().then(setSettings); }, []);
  return <><Header title="Settings" subtitle="Agenthub is configured with a local TOML file"/><section className="settings-card"><h2>Configuration file</h2><p>Edit this file and restart agenthub to change advanced settings.</p><code>{settings?.configPath ?? "Loading…"}</code><h3>Conversation logging</h3><div className="notice"><ShieldCheck/><div><b>{settings?.logConversations ? "Enabled" : "Disabled by default"}</b><p>When enabled, prompts and responses may contain sensitive information and are written as local JSONL files. Set <code>log_conversations = true</code> under <code>[privacy]</code> only if you understand this risk.</p>{settings?.logConversations && <p>Location: <code>{settings.conversationLogPath}</code> · retention: {settings.conversationRetentionDays || "unlimited"} days</p>}</div></div><h3>Metadata retention</h3><p>{settings?.requestMetadataDays ? `${settings.requestMetadataDays} days` : "Unlimited"}</p></section></>;
}

function RequestTable({ rows, showAccumulated = false }: { rows: NonNullable<DashboardStats>["recent"]; showAccumulated?: boolean }) { return <div className="table-wrap"><table><thead><tr><th>Date</th><th>Application</th><th>Provider / model</th><th>Endpoint</th><th>Status</th><th>Tokens (in/out)</th>{showAccumulated && <th>Accumulated (in/out)</th>}<th>Duration</th></tr></thead><tbody>{rows.map(r => <tr key={r.id}><td>{formatDate(r.createdAt)}</td><td>{r.application}</td><td><b>{r.provider}</b><small>{r.model}</small></td><td><b>{r.modality || "unknown"}</b><small>{r.endpoint || "—"}</small></td><td><span className={r.status < 400 ? "status good" : "status warn"} title={r.errorType ?? undefined}>{r.status}</span></td><td>{number(r.tokensIn)} / {number(r.tokensOut)}</td>{showAccumulated && <td>{number(r.accumulatedTokensIn)} / {number(r.accumulatedTokensOut)}</td>}<td>{formatDuration(r.durationMs)}</td></tr>)}</tbody></table></div>; }
function Modal({ title, onClose, children }: { title: string; onClose: () => void; children: React.ReactNode }) { return <div className="modal-bg" onMouseDown={e => e.target === e.currentTarget && onClose()}><div className="modal" role="dialog" aria-modal="true" aria-label={title}><div className="modal-head"><h2>{title}</h2><button className="icon" aria-label="Close" onClick={onClose}><X/></button></div>{children}</div></div>; }
function DialogActions({ onClose, label }: { onClose: () => void; label: string }) { return <div className="actions"><button type="button" className="secondary" onClick={onClose}>Cancel</button><button className="primary" type="submit">{label}</button></div>; }
function CopyButton({ value }: { value: string }) { const [copied, setCopied] = useState(false); return <button className="copy" aria-label="Copy" onClick={() => { void navigator.clipboard.writeText(value); setCopied(true); setTimeout(() => setCopied(false), 1200); }}>{copied ? <Check/> : <Copy/>}</button>; }

export default function App() { return <Layout/>; }
