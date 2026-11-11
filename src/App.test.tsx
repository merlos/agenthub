import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

const provider = { id: "p1", name: "OpenAI", kind: "openai", baseUrl: "https://api.openai.com/v1", defaultModel: "gpt", isDefault: true, hasSecret: true };
const application = { id: "a1", name: "OpenCode", port: 43100, apiKey: "ah_1234567890", providerId: "p1", enabled: true };
const stat = { id: 1, application: "OpenCode", provider: "OpenAI", model: "gpt", endpoint: "/chat/completions", modality: "text", status: 200, tokensIn: 2, tokensOut: 3, accumulatedTokensIn: 12, accumulatedTokensOut: 23, durationMs: 40, requestBytes: 10, responseBytes: 20, errorType: null, createdAt: "2026-01-01T00:00:00Z" };

function responses(command: string) {
  if (command === "list_providers") return Promise.resolve([provider]);
  if (command === "list_provider_usage") return Promise.resolve([{ providerId: "p1", totalTokens: 5, monthTokens: 3, periodStart: "2026-01-01", periodEnd: "2026-01-31" }]);
  if (command === "list_applications") return Promise.resolve([application]);
  if (command === "dashboard_stats") return Promise.resolve({ requests: 1, successRate: 100, tokensIn: 2, tokensOut: 3, averageDurationMs: 40, p95DurationMs: 55, providerTokens: [{ provider: "OpenAI", tokensIn: 2, tokensOut: 3, averageTokensPerSecond: 75 }], daily: [{ day: "2025-12-31", requests: 0 }, { day: "2026-01-01", requests: 2 }], recent: [stat] });
  if (command === "get_settings") return Promise.resolve({ logConversations: false, configPath: "/home/a/config.toml", conversationLogPath: "/home/a/conversations", conversationRetentionDays: 30, requestMetadataDays: 0 });
  return Promise.resolve(undefined);
}
function page(path: string) { return render(<MemoryRouter initialEntries={[path]}><App/></MemoryRouter>); }

describe("App", () => {
  beforeEach(() => { invoke.mockReset().mockImplementation(responses); });
  it("shows dashboard metrics and request rows", async () => { page("/"); expect(await screen.findByText("100.0%")).toBeInTheDocument(); expect(screen.getByText("55ms")).toBeInTheDocument(); expect(screen.queryByText("Token usage by provider")).not.toBeInTheDocument(); expect(screen.getByText("30-day traffic")).toBeInTheDocument(); expect(screen.getByText("/chat/completions")).toBeInTheDocument(); expect(screen.getByText("2026-01-01")).toBeInTheDocument(); expect(screen.getAllByText("OpenCode").length).toBeGreaterThan(0); });
  it("shows and edits applications", async () => {
    const user=userEvent.setup(); page("/applications"); expect((await screen.findAllByText("OpenCode")).length).toBeGreaterThan(0); await user.click(screen.getByText("Edit connection")); await user.clear(screen.getByLabelText("Application name")); await user.type(screen.getByLabelText("Application name"),"Editor"); await user.click(screen.getByText("Save changes")); await waitFor(()=>expect(invoke).toHaveBeenCalledWith("save_application",expect.objectContaining({application:expect.objectContaining({name:"Editor"})})));
  });
  it("creates and deletes an application", async () => {
    const user=userEvent.setup(); page("/applications"); await screen.findAllByText("OpenCode"); await user.click(screen.getByText("Add application")); await user.type(screen.getByLabelText("Application name"),"New App"); await user.selectOptions(screen.getByLabelText("Provider"),"p1"); await user.click(screen.getByText("Create application")); await waitFor(()=>expect(invoke).toHaveBeenCalledWith("save_application",expect.anything())); await user.click(screen.getByLabelText("Delete")); await waitFor(()=>expect(invoke).toHaveBeenCalledWith("delete_application",{id:"a1"}));
  });
  it("stops an application and regenerates its key", async () => {
    const user=userEvent.setup(); page("/applications"); await screen.findAllByText("OpenCode"); await user.click(screen.getByLabelText("Stop")); await waitFor(()=>expect(invoke).toHaveBeenCalledWith("save_application",expect.objectContaining({application:expect.objectContaining({enabled:false})}))); await user.click(screen.getByLabelText("Regenerate key")); await waitFor(()=>expect(invoke).toHaveBeenCalledWith("regenerate_application_key",{id:"a1"}));
  });
  it("creates, defaults, edits, and deletes providers", async () => {
    const user=userEvent.setup(); page("/providers"); expect((await screen.findAllByText("OpenAI")).length).toBeGreaterThan(0); expect(screen.queryByText("Token usage by provider")).not.toBeInTheDocument(); expect(screen.getByText("Total tokens")).toBeInTheDocument(); expect(screen.getByText("Month tokens")).toBeInTheDocument(); expect(screen.queryByText("Input tokens")).not.toBeInTheDocument(); expect(screen.queryByText("Output tokens")).not.toBeInTheDocument(); expect(screen.queryByText("Avg. tok/s")).not.toBeInTheDocument(); expect(screen.getByText("5")).toBeInTheDocument(); expect(screen.getByText("3")).toBeInTheDocument(); expect(screen.getByRole("link", {name:"Statistics for OpenAI"})).toHaveAttribute("href","/providers/p1/stats"); await user.click(screen.getByText("Add provider")); expect(screen.getByText("Load models")).toBeDisabled(); await user.selectOptions(screen.getByLabelText("Provider type"),"deepseek"); expect(screen.getByDisplayValue("https://api.deepseek.com/v1")).toBeInTheDocument(); await user.type(screen.getByLabelText("API key"),"secret"); expect(screen.getByText("Load models")).toBeEnabled(); await user.click(screen.getByText("Save provider")); await waitFor(()=>expect(invoke).toHaveBeenCalledWith("save_provider",expect.anything()));
    const buttons=screen.getAllByLabelText("Delete"); await user.click(buttons[0]); await waitFor(()=>expect(invoke).toHaveBeenCalledWith("delete_provider",{id:"p1"}));
  });
  it("configures Ollama without an API key", async () => {
    const ollama={...provider,id:"ollama",name:"Ollama",kind:"ollama",baseUrl:"http://localhost:11434/v1",defaultModel:"llama3.2",hasSecret:false}; invoke.mockImplementation((command:string)=>command==="list_providers"?Promise.resolve([ollama]):command==="list_provider_models"?Promise.resolve(["gemma3","llama3.2"]):responses(command)); const user=userEvent.setup(); page("/providers"); expect(await screen.findByText("Local")).toBeInTheDocument(); await user.click(screen.getByText("Add provider")); await user.selectOptions(screen.getByLabelText("Provider type"),"ollama"); expect(screen.getByLabelText("Display name")).toHaveValue("Ollama"); expect(screen.getByDisplayValue("http://localhost:11434/v1")).toBeInTheDocument(); expect(screen.getByDisplayValue("llama3.2")).toBeInTheDocument(); expect(screen.getByLabelText("API key")).not.toBeRequired(); expect(screen.getByLabelText("API key")).toHaveAttribute("placeholder","Optional"); await user.click(screen.getByText("Load models")); await waitFor(()=>expect(document.querySelector('datalist option[value="gemma3"]')).toBeInTheDocument()); await user.click(screen.getByText("Save provider")); await waitFor(()=>expect(invoke).toHaveBeenCalledWith("save_provider",expect.objectContaining({provider:expect.objectContaining({kind:"ollama",name:"Ollama",defaultModel:"llama3.2"})})));
  });
  it("shows and filters logs with friendly durations", async () => {
    const rows=[stat,{...stat,id:2,application:"Terminal",durationMs:30_000},{...stat,id:3,application:"Long task",durationMs:393_000},{...stat,id:4,application:"Minute task",durationMs:60_000}];
    invoke.mockImplementation((command:string)=>command==="dashboard_stats"?Promise.resolve({...responses,requests:4,daily:[],recent:rows}):responses(command));
    const user=userEvent.setup(); const view=page("/logs"); expect(await screen.findByText("40ms")).toBeInTheDocument(); expect(screen.getByText("Tokens (in/out)")).toBeInTheDocument(); expect(screen.getByText("Accumulated (in/out)")).toBeInTheDocument(); expect(screen.getAllByText("2 / 3").length).toBeGreaterThan(0); expect(screen.getAllByText("12 / 23").length).toBeGreaterThan(0); expect(screen.getByText("30s")).toBeInTheDocument(); expect(screen.getByText("6m 33s")).toBeInTheDocument(); expect(screen.getByText("1m")).toBeInTheDocument(); const filter=screen.getByLabelText("Filter requests"); await user.type(filter,"terminal"); expect(screen.getByText("Terminal")).toBeInTheDocument(); expect(screen.queryByText("Long task")).not.toBeInTheDocument(); await user.clear(filter); await user.type(filter,"no match"); expect(screen.getByText("No matching requests")).toBeInTheDocument(); view.unmount(); page("/settings"); expect(await screen.findByText("Disabled by default")).toBeInTheDocument(); expect(screen.getByText("/home/a/config.toml")).toBeInTheDocument();
  });
  it("auto-refreshes logs, preserves filters, and pauses and resumes polling", async () => {
    vi.useFakeTimers();
    const view = page("/logs");
    try {
      await act(async () => {});
      expect(screen.getByRole("button", { name: "Disable auto-refresh" })).toHaveAttribute("aria-pressed", "true");
      fireEvent.change(screen.getByLabelText("Filter requests"), { target: { value: "terminal" } });
      expect(screen.getByText("No matching requests")).toBeInTheDocument();
      invoke.mockImplementation(command => command === "dashboard_stats" ? Promise.resolve({ recent: [{ ...stat, application: "Terminal" }] }) : responses(command));
      await act(async () => { await vi.advanceTimersByTimeAsync(5_000); });
      expect(screen.getByText("Terminal")).toBeInTheDocument();
      expect(screen.getByLabelText("Filter requests")).toHaveValue("terminal");
      fireEvent.click(screen.getByRole("button", { name: "Disable auto-refresh" }));
      const count = invoke.mock.calls.length;
      await act(async () => { await vi.advanceTimersByTimeAsync(10_000); });
      expect(invoke).toHaveBeenCalledTimes(count);
      fireEvent.click(screen.getByRole("button", { name: "Enable auto-refresh" }));
      await act(async () => {});
      expect(invoke).toHaveBeenCalledTimes(count + 1);
      view.unmount();
      await vi.advanceTimersByTimeAsync(10_000);
      expect(invoke).toHaveBeenCalledTimes(count + 1);
    } finally { view.unmount(); vi.useRealTimers(); }
  });
  it("reports refresh errors and recovers on the next poll", async () => {
    vi.useFakeTimers();
    invoke.mockRejectedValueOnce(new Error("logs offline"));
    const view = page("/logs");
    try {
      await act(async () => {});
      expect(screen.getByText(/logs offline/)).toBeInTheDocument();
      await act(async () => { await vi.advanceTimersByTimeAsync(5_000); });
      expect(screen.queryByText(/logs offline/)).not.toBeInTheDocument();
      expect(screen.getByText("OpenCode")).toBeInTheDocument();
    } finally { view.unmount(); vi.useRealTimers(); }
  });
  it.each([false, true])("ignores in-flight refresh results after leaving the page (failure: %s)", async failure => {
    vi.useFakeTimers();
    let resolve!: (value: unknown) => void;
    let reject!: (error: Error) => void;
    invoke.mockImplementation(() => new Promise((ok, fail) => { resolve = ok; reject = fail; }));
    const view = page("/logs");
    try {
      await act(async () => { await vi.advanceTimersByTimeAsync(10_000); });
      expect(invoke).toHaveBeenCalledTimes(1);
      view.unmount();
      await act(async () => { if (failure) reject(new Error("late error")); else resolve({ recent: [stat] }); });
      await vi.advanceTimersByTimeAsync(10_000);
      expect(invoke).toHaveBeenCalledTimes(1);
    } finally { view.unmount(); vi.useRealTimers(); }
  });
  it("closes dialogs with cancel, close, and backdrop", async () => { const user=userEvent.setup(); page("/applications"); await screen.findAllByText("OpenCode"); await user.click(screen.getByText("Add application")); await user.click(screen.getByText("Cancel")); expect(screen.queryByText("Create application")).not.toBeInTheDocument(); await user.click(screen.getByText("Add application")); await user.click(screen.getByLabelText("Close")); await user.click(screen.getByText("Add application")); fireEvent.mouseDown(document.querySelector(".modal-bg")!); expect(screen.queryByText("Create application")).not.toBeInTheDocument(); });
  it("renders stopped apps, fallback providers, and copy feedback", async () => {
    invoke.mockImplementation((command: string) => command === "list_applications" ? Promise.resolve([{ ...application, enabled: false, providerId: null }]) : command === "list_providers" ? Promise.resolve([]) : responses(command));
    const user=userEvent.setup(); page("/applications"); expect(await screen.findByText("Stopped")).toBeInTheDocument(); expect(screen.getByText("Default provider")).toBeInTheDocument(); await user.click(screen.getAllByLabelText("Copy")[0]); await new Promise(resolve => setTimeout(resolve, 1250)); vi.mocked(window.confirm).mockReturnValueOnce(false); await user.click(screen.getByLabelText("Regenerate key")); expect(invoke).not.toHaveBeenCalledWith("regenerate_application_key",expect.anything()); vi.mocked(window.confirm).mockReturnValueOnce(false); await user.click(screen.getByLabelText("Delete")); expect(invoke).not.toHaveBeenCalledWith("delete_application",expect.anything());
  });
  it("handles non-default providers and provider editing", async () => {
    const alternate={...provider,isDefault:false,hasSecret:false}; invoke.mockImplementation((command:string)=>command==="list_providers"?Promise.resolve([alternate]):responses(command));
    const user=userEvent.setup(); page("/providers"); expect(await screen.findByText("No key")).toBeInTheDocument(); await user.click(screen.getByText("Make default")); await waitFor(()=>expect(invoke).toHaveBeenCalledWith("set_default_provider",{id:"p1"})); vi.mocked(window.confirm).mockReturnValueOnce(false); await user.click(screen.getByLabelText("Delete")); expect(invoke).not.toHaveBeenCalledWith("delete_provider",expect.anything()); await user.click(screen.getByLabelText("Edit")); expect(screen.getByLabelText("API key")).toHaveAttribute("placeholder","Paste API key"); await user.type(screen.getByLabelText("API key"),"key"); await user.clear(screen.getByLabelText("Display name")); await user.type(screen.getByLabelText("Display name"),"Updated"); await user.clear(screen.getByLabelText("Base URL")); await user.type(screen.getByLabelText("Base URL"),"https://example.com/v1/"); await user.clear(screen.getByLabelText("Default model")); await user.type(screen.getByLabelText("Default model"),"new-model"); await user.click(screen.getByText("Save provider")); await waitFor(()=>expect(invoke).toHaveBeenCalledWith("save_provider",expect.objectContaining({provider:expect.objectContaining({name:"Updated",baseUrl:"https://example.com/v1"})})));
  });
  it("renders empty states, failures, enabled logging, and error statuses", async () => {
    invoke.mockImplementation((command:string)=>{if(command==="list_applications"||command==="list_providers")return Promise.resolve([]);if(command==="dashboard_stats")return Promise.resolve({requests:1,successRate:0,tokensIn:0,tokensOut:0,averageDurationMs:1,p95DurationMs:2,daily:[],recent:[{...stat,status:500,errorType:"upstream_error"}]});if(command==="get_settings")return Promise.resolve({logConversations:true,configPath:"/tmp/config",conversationLogPath:"/tmp/logs",conversationRetentionDays:7,requestMetadataDays:14});return responses(command)});
    let view=page("/applications"); expect(await screen.findByText("Connect your first application")).toBeInTheDocument(); view.unmount(); view=page("/providers"); expect(await screen.findByText("Add your first provider")).toBeInTheDocument(); const user=userEvent.setup(); await user.click(screen.getByText("Add provider")); await user.selectOptions(screen.getByLabelText("Provider type"),"compatible"); expect(screen.getByDisplayValue("Custom provider")).toBeInTheDocument(); await user.click(screen.getByText("Cancel")); view.unmount(); view=page("/logs"); expect(await screen.findByText("500")).toHaveClass("warn"); view.unmount(); page("/settings"); expect(await screen.findByText("Enabled")).toBeInTheDocument(); expect(screen.getByText("14 days")).toBeInTheDocument();
  });
  it("renders backend loading errors", async () => {
    invoke.mockImplementation((command:string)=>command==="dashboard_stats"?Promise.reject(new Error("stats offline")):responses(command)); page("/"); expect(await screen.findByText(/stats offline/)).toBeInTheDocument();
  });
  it("handles application loading and mutation errors", async () => {
    invoke.mockImplementation((command:string)=>command==="list_applications"?Promise.reject(new Error("apps offline")):responses(command)); let view=page("/applications"); expect(await screen.findByText(/apps offline/)).toBeInTheDocument(); view.unmount();
    invoke.mockImplementation((command:string)=>command==="save_application"?Promise.reject(new Error("save failed")):responses(command)); const user=userEvent.setup(); view=page("/applications"); await screen.findAllByText("OpenCode"); await user.click(screen.getByText("Edit connection")); await user.click(screen.getByText("Save changes")); expect(await screen.findByText(/save failed/)).toBeInTheDocument(); await user.click(screen.getByLabelText("Close")); await user.click(screen.getByLabelText("Stop")); expect(await screen.findByText(/save failed/)).toBeInTheDocument();
    invoke.mockImplementation((command:string)=>command==="regenerate_application_key"?Promise.reject(new Error("key failed")):responses(command)); await user.click(screen.getByLabelText("Regenerate key")); expect(await screen.findByText(/key failed/)).toBeInTheDocument();
  });
  it("handles provider loading and saving errors", async () => {
    invoke.mockImplementation((command:string)=>command==="list_providers"?Promise.reject(new Error("providers offline")):responses(command)); let view=page("/providers"); expect(await screen.findByText(/providers offline/)).toBeInTheDocument(); view.unmount();
    invoke.mockImplementation((command:string)=>command==="save_provider"?Promise.reject(new Error("provider failed")):command==="list_provider_models"?Promise.reject(new Error("models failed")):responses(command)); const user=userEvent.setup(); page("/providers"); await screen.findAllByText("OpenAI"); await user.click(screen.getByLabelText("Edit")); expect(screen.getByLabelText("API key")).toHaveAttribute("placeholder","Leave blank to keep existing key"); await user.click(screen.getByText("Load models")); expect(await screen.findByText(/models failed/)).toBeInTheDocument(); await user.click(screen.getByText("Save provider")); expect(await screen.findByText(/provider failed/)).toBeInTheDocument();
  });
  it("uses default provider, cancellation, fallbacks, and unlimited retention", async () => {
    const missingFields={...stat,endpoint:"",modality:"",errorType:null}; invoke.mockImplementation((command:string)=>{if(command==="dashboard_stats")return Promise.resolve({requests:1,successRate:100,tokensIn:0,tokensOut:0,averageDurationMs:0,p95DurationMs:0,daily:[],recent:[missingFields]});if(command==="get_settings")return Promise.resolve({logConversations:true,configPath:"/config",conversationLogPath:"/logs",conversationRetentionDays:0,requestMetadataDays:0});return responses(command)});
    let view=page("/"); expect(await screen.findByText("unknown")).toBeInTheDocument(); expect(screen.getByText("—")).toBeInTheDocument(); view.unmount(); const user=userEvent.setup(); view=page("/applications"); await screen.findAllByText("OpenCode"); await user.click(screen.getByText("Add application")); await user.type(screen.getByLabelText("Application name"),"Default App"); await user.click(screen.getByText("Create application")); await waitFor(()=>expect(invoke).toHaveBeenCalledWith("save_application",expect.objectContaining({application:expect.objectContaining({providerId:null})}))); view.unmount(); page("/settings"); expect(await screen.findByText(/unlimited days/)).toBeInTheDocument(); expect(screen.getByText("Unlimited")).toBeInTheDocument();
  });
});
