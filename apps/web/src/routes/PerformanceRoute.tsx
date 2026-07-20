import { useMemo } from "react";
import { useQuery } from "@tanstack/react-query";

import { ApiClientError, apiClient } from "../api/client";
import type { InstagramInsightSnapshotResponse } from "../api/types";
import { useAuth } from "../auth/useAuth";
import { LoginRoute } from "./LoginRoute";

const snapshotLimit = 30;
const weeklyFollowerGoal = 35;
const weeklyReachGoal = 3500;
const weeklySaveShareGoal = 120;

export function PerformanceRoute() {
  const auth = useAuth();
  const insightsQuery = useQuery({
    queryKey: ["instagram-insights", snapshotLimit],
    queryFn: () => apiClient.listInstagramInsights(snapshotLimit),
    enabled: auth.isAuthenticated,
  });

  const snapshots = useMemo(
    () => sortSnapshotsAscending(insightsQuery.data?.snapshots ?? []),
    [insightsQuery.data?.snapshots],
  );
  const summary = useMemo(() => performanceSummary(snapshots), [snapshots]);

  if (auth.status === "unknown") {
    return (
      <div className="border border-slate-200 bg-white p-6 shadow-sm">
        <p className="text-sm font-medium text-slate-600">Checking session...</p>
      </div>
    );
  }

  if (auth.status === "anonymous") {
    return <LoginRoute />;
  }

  return (
    <div className="space-y-6">
      <section className="border border-slate-200 bg-white p-6 shadow-sm">
        <div className="flex flex-col gap-5 xl:flex-row xl:items-start xl:justify-between">
          <div>
            <p className="text-pine text-sm font-semibold uppercase">Performance</p>
            <h2 className="mt-2 text-2xl font-semibold text-slate-950">Growth dashboard</h2>
            <p className="mt-2 max-w-3xl text-sm leading-6 text-slate-600">
              Track whether the Vancouver content cadence is building audience, reach, and saved
              value.
            </p>
          </div>

          <div className="grid grid-cols-3 gap-3 text-sm">
            <PerformanceMetric label="Snapshots" value={snapshots.length.toString()} />
            <PerformanceMetric label="Window" value={summary.windowLabel} />
            <PerformanceMetric label="Latest" value={summary.latestDateLabel} />
          </div>
        </div>
      </section>

      <section className="border border-slate-200 bg-white shadow-sm">
        <div className="flex flex-col gap-3 border-b border-slate-200 px-5 py-4 sm:flex-row sm:items-center sm:justify-between">
          <div>
            <h3 className="text-base font-semibold text-slate-950">Instagram growth signals</h3>
            <p className="mt-1 text-sm text-slate-500">
              Followers, reach, saves, and shares from the latest stored snapshots.
            </p>
          </div>
          <button
            type="button"
            onClick={() => void insightsQuery.refetch()}
            disabled={insightsQuery.isFetching}
            className="border border-slate-300 px-3 py-2 text-sm font-semibold text-slate-700 hover:bg-slate-50 disabled:cursor-wait disabled:opacity-60"
          >
            {insightsQuery.isFetching ? "Refreshing..." : "Refresh"}
          </button>
        </div>

        {insightsQuery.isLoading ? (
          <p className="p-6 text-sm text-slate-600">Loading performance snapshots...</p>
        ) : null}

        {insightsQuery.isError ? (
          <p className="p-6 text-sm font-medium text-red-700">
            {errorMessage(insightsQuery.error)}
          </p>
        ) : null}

        {insightsQuery.isSuccess && snapshots.length === 0 ? <EmptyPerformanceState /> : null}

        {insightsQuery.isSuccess && snapshots.length > 0 ? (
          <div className="grid gap-5 p-5">
            <div className="grid gap-4 lg:grid-cols-4">
              <KpiPanel
                label="Followers"
                value={formatNumber(summary.latest.followers_count)}
                detail={`${signedNumber(summary.followerDelta)} over this window`}
                tone={summary.followerDelta >= 0 ? "emerald" : "rose"}
              />
              <KpiPanel
                label="Reach"
                value={formatNumber(summary.latest.reach)}
                detail={`${formatNumber(summary.averageReach)} avg per snapshot`}
                tone="sky"
              />
              <KpiPanel
                label="Saves"
                value={formatNumber(summary.totalSaves)}
                detail={`${formatNumber(summary.latest.saves)} latest snapshot`}
                tone="amber"
              />
              <KpiPanel
                label="Shares"
                value={formatNumber(summary.totalShares)}
                detail={`${formatNumber(summary.latest.shares)} latest snapshot`}
                tone="coral"
              />
            </div>

            <div className="grid gap-5 xl:grid-cols-[minmax(0,1fr)_360px]">
              <section className="border border-slate-200 bg-white">
                <div className="border-b border-slate-200 px-5 py-4">
                  <h3 className="text-base font-semibold text-slate-950">Snapshot trend</h3>
                  <p className="mt-1 text-sm text-slate-500">
                    Reach bars with follower line across the stored window.
                  </p>
                </div>
                <div className="p-5">
                  <PerformanceChart snapshots={snapshots} />
                </div>
              </section>

              <aside className="space-y-4">
                <GoalPanel
                  label="Weekly follower lift"
                  value={summary.followerDelta}
                  goal={weeklyFollowerGoal}
                  suffix="followers"
                />
                <GoalPanel
                  label="Weekly reach"
                  value={summary.totalReach}
                  goal={weeklyReachGoal}
                  suffix="reach"
                />
                <GoalPanel
                  label="Saves + shares"
                  value={summary.totalSaves + summary.totalShares}
                  goal={weeklySaveShareGoal}
                  suffix="actions"
                />
              </aside>
            </div>

            <section className="overflow-hidden border border-slate-200 bg-white">
              <div className="border-b border-slate-200 px-5 py-4">
                <h3 className="text-base font-semibold text-slate-950">Recent snapshots</h3>
              </div>
              <div className="overflow-x-auto">
                <table className="min-w-full divide-y divide-slate-200 text-left text-sm">
                  <thead className="bg-slate-50 text-xs font-semibold uppercase text-slate-500">
                    <tr>
                      <th className="px-5 py-3">Captured</th>
                      <th className="px-5 py-3">Followers</th>
                      <th className="px-5 py-3">Reach</th>
                      <th className="px-5 py-3">Saves</th>
                      <th className="px-5 py-3">Shares</th>
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-slate-100">
                    {[...snapshots]
                      .reverse()
                      .slice(0, 8)
                      .map((snapshot) => (
                        <tr key={snapshot.id}>
                          <td className="px-5 py-3 font-medium text-slate-800">
                            {formatDateTime(snapshot.captured_at)}
                          </td>
                          <td className="px-5 py-3 text-slate-700">
                            {formatNumber(snapshot.followers_count)}
                          </td>
                          <td className="px-5 py-3 text-slate-700">
                            {formatNumber(snapshot.reach)}
                          </td>
                          <td className="px-5 py-3 text-slate-700">
                            {formatNumber(snapshot.saves)}
                          </td>
                          <td className="px-5 py-3 text-slate-700">
                            {formatNumber(snapshot.shares)}
                          </td>
                        </tr>
                      ))}
                  </tbody>
                </table>
              </div>
            </section>
          </div>
        ) : null}
      </section>
    </div>
  );
}

interface PerformanceMetricProps {
  label: string;
  value: string;
}

function PerformanceMetric({ label, value }: PerformanceMetricProps) {
  return (
    <div className="border border-slate-200 bg-slate-50 px-4 py-3">
      <p className="text-xs font-semibold uppercase text-slate-500">{label}</p>
      <p className="mt-1 whitespace-nowrap text-2xl font-semibold text-slate-950">{value}</p>
    </div>
  );
}

interface KpiPanelProps {
  label: string;
  value: string;
  detail: string;
  tone: "amber" | "coral" | "emerald" | "rose" | "sky";
}

function KpiPanel({ label, value, detail, tone }: KpiPanelProps) {
  return (
    <article className={`border-l-4 bg-white px-5 py-4 shadow-sm ${kpiToneClassName(tone)}`}>
      <p className="text-sm font-semibold uppercase text-slate-500">{label}</p>
      <p className="mt-2 text-3xl font-semibold text-slate-950">{value}</p>
      <p className="mt-2 text-sm text-slate-600">{detail}</p>
    </article>
  );
}

interface GoalPanelProps {
  label: string;
  value: number;
  goal: number;
  suffix: string;
}

function GoalPanel({ label, value, goal, suffix }: GoalPanelProps) {
  const progress = goal > 0 ? Math.min(100, Math.max(0, (value / goal) * 100)) : 0;

  return (
    <article className="border border-slate-200 bg-slate-50 p-4">
      <div className="flex items-start justify-between gap-3">
        <div>
          <p className="text-sm font-semibold text-slate-950">{label}</p>
          <p className="mt-1 text-sm text-slate-600">
            {formatNumber(value)} / {formatNumber(goal)} {suffix}
          </p>
        </div>
        <span className="rounded-full bg-white px-2.5 py-1 text-xs font-semibold uppercase text-slate-700">
          {Math.round(progress)}%
        </span>
      </div>
      <div className="mt-4 h-2 bg-white">
        <div className="bg-coral h-full" style={{ width: `${progress}%` }} />
      </div>
    </article>
  );
}

function PerformanceChart({ snapshots }: { snapshots: InstagramInsightSnapshotResponse[] }) {
  const width = 720;
  const height = 260;
  const padding = 32;
  const reachMax = Math.max(...snapshots.map((snapshot) => snapshot.reach), 1);
  const followerMin = Math.min(...snapshots.map((snapshot) => snapshot.followers_count));
  const followerMax = Math.max(...snapshots.map((snapshot) => snapshot.followers_count));
  const followerRange = Math.max(followerMax - followerMin, 1);
  const xStep = snapshots.length > 1 ? (width - padding * 2) / (snapshots.length - 1) : 0;
  const followerPoints = snapshots
    .map((snapshot, index) => {
      const x = padding + index * xStep;
      const y =
        height -
        padding -
        ((snapshot.followers_count - followerMin) / followerRange) * (height - padding * 2);
      return `${x},${y}`;
    })
    .join(" ");
  const barWidth = Math.max(10, Math.min(34, (width - padding * 2) / snapshots.length - 6));

  return (
    <svg
      role="img"
      aria-label="Instagram reach and follower trend"
      viewBox={`0 0 ${width} ${height}`}
      className="h-auto w-full"
    >
      <rect x="0" y="0" width={width} height={height} fill="#f8fafc" />
      <line
        x1={padding}
        y1={height - padding}
        x2={width - padding}
        y2={height - padding}
        stroke="#cbd5e1"
      />
      {snapshots.map((snapshot, index) => {
        const x = padding + index * xStep - barWidth / 2;
        const barHeight = (snapshot.reach / reachMax) * (height - padding * 2);
        const y = height - padding - barHeight;

        return (
          <rect
            key={snapshot.id}
            x={x}
            y={y}
            width={barWidth}
            height={Math.max(2, barHeight)}
            fill="#38bdf8"
            opacity="0.78"
          />
        );
      })}
      <polyline points={followerPoints} fill="none" stroke="#f97316" strokeWidth="4" />
      {snapshots.map((snapshot, index) => {
        const [x, y] = followerPoints.split(" ")[index].split(",").map(Number);

        return <circle key={`followers-${snapshot.id}`} cx={x} cy={y} r="4" fill="#f97316" />;
      })}
      <text x={padding} y="22" fill="#475569" fontSize="13" fontWeight="600">
        Reach bars
      </text>
      <text x={width - padding - 92} y="22" fill="#c2410c" fontSize="13" fontWeight="600">
        Followers
      </text>
    </svg>
  );
}

function EmptyPerformanceState() {
  return (
    <div className="p-6">
      <p className="text-base font-semibold text-slate-950">No insight snapshots yet</p>
      <p className="mt-2 max-w-2xl text-sm leading-6 text-slate-600">
        Connect Instagram and let the insights job collect the first snapshot before this dashboard
        fills in.
      </p>
    </div>
  );
}

interface PerformanceSummary {
  latest: InstagramInsightSnapshotResponse;
  followerDelta: number;
  totalReach: number;
  averageReach: number;
  totalSaves: number;
  totalShares: number;
  latestDateLabel: string;
  windowLabel: string;
}

function performanceSummary(snapshots: InstagramInsightSnapshotResponse[]): PerformanceSummary {
  const fallback = emptySnapshot();
  const latest = snapshots.length > 0 ? snapshots[snapshots.length - 1] : fallback;
  const first = snapshots.length > 0 ? snapshots[0] : fallback;
  const totalReach = snapshots.reduce((total, snapshot) => total + snapshot.reach, 0);
  const totalSaves = snapshots.reduce((total, snapshot) => total + snapshot.saves, 0);
  const totalShares = snapshots.reduce((total, snapshot) => total + snapshot.shares, 0);

  return {
    latest,
    followerDelta: latest.followers_count - first.followers_count,
    totalReach,
    averageReach: snapshots.length > 0 ? Math.round(totalReach / snapshots.length) : 0,
    totalSaves,
    totalShares,
    latestDateLabel: snapshots.length > 0 ? formatShortDate(latest.captured_at) : "none",
    windowLabel: snapshots.length > 1 ? `${snapshots.length} pts` : `${snapshots.length} pt`,
  };
}

function sortSnapshotsAscending(
  snapshots: InstagramInsightSnapshotResponse[],
): InstagramInsightSnapshotResponse[] {
  return [...snapshots].sort(
    (a, b) => new Date(a.captured_at).getTime() - new Date(b.captured_at).getTime(),
  );
}

function emptySnapshot(): InstagramInsightSnapshotResponse {
  return {
    id: 0,
    instagram_account_id: "",
    followers_count: 0,
    reach: 0,
    saves: 0,
    shares: 0,
    captured_at: new Date(0).toISOString(),
    created_at: new Date(0).toISOString(),
  };
}

function formatNumber(value: number): string {
  return new Intl.NumberFormat("en-CA").format(value);
}

function signedNumber(value: number): string {
  if (value > 0) {
    return `+${formatNumber(value)}`;
  }

  return formatNumber(value);
}

function formatShortDate(value: string): string {
  return new Intl.DateTimeFormat("en-CA", {
    month: "short",
    day: "numeric",
  }).format(new Date(value));
}

function formatDateTime(value: string): string {
  return new Intl.DateTimeFormat("en-CA", {
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  }).format(new Date(value));
}

function kpiToneClassName(tone: KpiPanelProps["tone"]): string {
  switch (tone) {
    case "amber":
      return "border-amber-400";
    case "coral":
      return "border-coral";
    case "emerald":
      return "border-emerald-400";
    case "rose":
      return "border-rose-400";
    case "sky":
      return "border-sky-400";
  }
}

function errorMessage(error: Error): string {
  if (error instanceof ApiClientError) {
    return error.message;
  }

  return "Performance data could not be loaded.";
}
