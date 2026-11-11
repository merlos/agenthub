import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";
import ProviderStatsPage, { TokenChart } from "./ProviderStatsPage";
import type { ProviderStats, TokenBucket } from "./types";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
const usage = { requests: 2, tokensIn: 100, tokensOut: 200, totalTokens: 300, averageTokens: 150, averageTokensPerSecond: 12.5 };
const daily: TokenBucket[] = Array.from({ length: 30 }, (_, index) => ({ ...usage, date: `2026-10-${String(index + 1).padStart(2, "0")}` }));
daily[29] = { ...daily[29], requests: 0, tokensIn: 0, tokensOut: 0, totalTokens: 0, averageTokens: 0, averageTokensPerSecond: 0 };
const stats: ProviderStats = {
  providerId: "p1", providerName: "Azure", billingStartDay: 15, periodStart: "2026-10-15", periodEnd: "2026-11-14",
  current: usage, historical: { ...usage, totalTokens: 600 }, daily,
  monthly: Array.from({ length: 12 }, (_, index) => ({ ...usage, date: `2026-${String(index + 1).padStart(2, "0")}` })),
};
let resize: ResizeObserverCallback;
const disconnect = vi.fn();
function page() { return render(<MemoryRouter initialEntries={["/providers/p1/stats"]}><App/></MemoryRouter>); }
function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: Error) => void;
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}

describe("provider statistics", () => {
  beforeEach(() => {
    invoke.mockReset().mockImplementation((command: string) => Promise.resolve(command === "provider_stats" ? stats : []));
    disconnect.mockReset();
    vi.stubGlobal("ResizeObserver", class {
      constructor(callback: ResizeObserverCallback) { resize = callback; }
      observe() {}
      disconnect = disconnect;
    });
  });

  it("shows period and historical summaries, chart details, and navigation", async () => {
    const user = userEvent.setup();
    page();
    expect(screen.getByRole("status")).toHaveTextContent("Loading statistics");
    expect(await screen.findByText("Azure statistics")).toBeInTheDocument();
    await screen.findByText("Last 7 days");
    const current = screen.getByRole("region", { name: "Current billing period" });
    expect(within(current).getByText("300")).toBeInTheDocument();
    expect(within(current).getByText("150")).toBeInTheDocument();
    expect(within(current).getByText("12.5")).toBeInTheDocument();
    expect(screen.getByText("2026-10-15 – 2026-11-14 · 2 requests")).toBeInTheDocument();
    expect(within(screen.getByRole("region", { name: "Historical usage" })).getByText("600")).toBeInTheDocument();
    expect(screen.getByLabelText("Billing period starts on day")).toHaveValue("15");
    expect(screen.getAllByRole("option")).toHaveLength(31);
    const dailyGroup = screen.getByRole("group", { name: "Daily token consumption" });
    const bar = within(dailyGroup).getAllByRole("button")[0];
    fireEvent.mouseOver(bar);
    expect(screen.getByText("Avg. output tok/s: 12.5")).toBeInTheDocument();
    expect(screen.getByText("Input: 100")).toBeInTheDocument();
    expect(screen.getByText("Output: 200")).toBeInTheDocument();
    fireEvent.mouseOut(bar);
    expect(screen.queryByText("Avg. output tok/s: 12.5")).not.toBeInTheDocument();
    fireEvent.focus(bar);
    expect(screen.getByText("Avg. output tok/s: 12.5")).toBeInTheDocument();
    fireEvent.blur(bar);
    await user.click(within(dailyGroup).getAllByRole("button").at(-1)!);
    expect(screen.getByText("Avg. output tok/s: 0")).toBeInTheDocument();
    await user.click(within(screen.getByRole("group", { name: "Monthly token consumption" })).getAllByRole("button")[0]);
    expect(screen.getAllByText("Total: 300")).toHaveLength(1);
    await user.click(within(screen.getByRole("main")).getByRole("link", { name: "Providers" }));
    expect(await screen.findByText("Manage credentials and model endpoints")).toBeInTheDocument();
    expect(disconnect).toHaveBeenCalled();
  });

  it("fits the daily chart to its width while keeping the newest dates", () => {
    const view = render(<TokenChart buckets={daily} daily/>);
    act(() => resize([{ contentRect: { width: 960 } } as ResizeObserverEntry], {} as ResizeObserver));
    expect(screen.getAllByRole("button")).toHaveLength(30);
    expect(screen.getByText("Last 30 days")).toBeInTheDocument();
    act(() => resize([{ contentRect: { width: 320 } } as ResizeObserverEntry], {} as ResizeObserver));
    expect(screen.getAllByRole("button")).toHaveLength(10);
    expect(screen.getAllByRole("button")[0]).toHaveAccessibleName(/2026-10-21/);
    act(() => resize([{ contentRect: { width: 10 } } as ResizeObserverEntry], {} as ResizeObserver));
    expect(screen.getAllByRole("button")).toHaveLength(7);
    view.unmount();
    expect(disconnect).toHaveBeenCalled();
    render(<TokenChart buckets={[daily[29]]}/>);
    expect(screen.getByRole("button")).toHaveAccessibleName(/0 input tokens, 0 output tokens/);
  });

  it("saves the billing start and reloads the period", async () => {
    const user = userEvent.setup();
    const save = deferred<void>();
    invoke.mockImplementation((command: string) => command === "set_provider_billing_start_day" ? save.promise : Promise.resolve(stats));
    page();
    await screen.findByText("Azure statistics");
    await user.selectOptions(screen.getByLabelText("Billing period starts on day"), "31");
    expect(screen.getByLabelText("Billing period starts on day")).toBeDisabled();
    expect(screen.getByRole("status")).toHaveTextContent("Saving…");
    expect(invoke).toHaveBeenCalledWith("set_provider_billing_start_day", { id: "p1", day: 31 });
    invoke.mockImplementation(() => Promise.resolve({ ...stats, billingStartDay: 31, periodStart: "2026-10-31", periodEnd: "2026-11-29" }));
    await act(async () => save.resolve());
    expect(screen.getByLabelText("Billing period starts on day")).toHaveValue("31");
    expect(screen.getByRole("status")).toHaveTextContent("Billing start day saved.");
    expect(screen.getByText("2026-10-31 – 2026-11-29 · 2 requests")).toBeInTheDocument();
  });

  it("shows loading and save failures without losing the existing summary", async () => {
    invoke.mockRejectedValueOnce(new Error("Provider not found"));
    const view = page();
    expect(await screen.findByRole("alert")).toHaveTextContent("Provider not found");
    expect(screen.queryByText("Loading statistics…")).not.toBeInTheDocument();
    view.unmount();
    page();
    await screen.findByText("Azure statistics");
    invoke.mockRejectedValueOnce(new Error("Could not save"));
    const user = userEvent.setup();
    await user.selectOptions(screen.getByLabelText("Billing period starts on day"), "1");
    expect(await screen.findByRole("alert")).toHaveTextContent("Could not save");
    expect(screen.getByLabelText("Billing period starts on day")).toBeEnabled();
    expect(screen.getByLabelText("Billing period starts on day")).toHaveValue("15");
    invoke.mockResolvedValueOnce(undefined).mockRejectedValueOnce(new Error("Could not reload"));
    await user.selectOptions(screen.getByLabelText("Billing period starts on day"), "2");
    expect(await screen.findByRole("alert")).toHaveTextContent("Could not reload");
  });

  it("ignores load results after navigating away", async () => {
    const load = deferred<ProviderStats>();
    invoke.mockReturnValueOnce(load.promise);
    const view = page();
    view.unmount();
    await act(async () => load.resolve(stats));
    const failure = deferred<ProviderStats>();
    invoke.mockReturnValueOnce(failure.promise);
    const next = page();
    next.unmount();
    await act(async () => failure.reject(new Error("stale")));
  });

  it("ignores save completion and failure after navigating away", async () => {
    const user = userEvent.setup();
    const save = deferred<void>();
    invoke.mockImplementation((command: string) => command === "set_provider_billing_start_day" ? save.promise : Promise.resolve(stats));
    const view = page();
    await screen.findByText("Azure statistics");
    await user.selectOptions(screen.getByLabelText("Billing period starts on day"), "1");
    view.unmount();
    await act(async () => save.resolve());
    const failure = deferred<void>();
    invoke.mockImplementation((command: string) => command === "set_provider_billing_start_day" ? failure.promise : Promise.resolve(stats));
    const next = page();
    await screen.findByText("Azure statistics");
    await user.selectOptions(screen.getByLabelText("Billing period starts on day"), "1");
    next.unmount();
    await act(async () => failure.reject(new Error("stale")));
  });

  it("handles a missing route parameter", async () => {
    render(<MemoryRouter><Routes><Route path="/" element={<ProviderStatsPage/>}/></Routes></MemoryRouter>);
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("provider_stats", { id: "" }));
    await screen.findByText("Azure statistics");
  });
});
