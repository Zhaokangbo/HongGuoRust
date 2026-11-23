/** M12-B: deterministic offline trend math for opt-in numeric-only Windows CSV.
 * This module never opens media, contacts a server, or classifies a leak.
 */
export const COLUMNS = [
  "TimestampUtc", "ElapsedSec", "Pid", "Stage", "CpuSeconds",
  "CpuPercentMachine", "WorkingSetMiB", "PrivateMiB", "Handles", "Threads",
];
const MEASURES = ["PrivateMiB", "WorkingSetMiB", "Handles", "Threads", "CpuPercentMachine"];
const NUMERICS = COLUMNS.filter(x => !["TimestampUtc", "Stage"].includes(x));
const MAX_ROWS = 20000;

function numeric(value, label) {
  if (value === "" || typeof value !== "string") throw new Error("Missing " + label);
  const n = Number(value);
  if (!Number.isFinite(n) || n < 0 || n > Number.MAX_SAFE_INTEGER) {
    throw new Error("Invalid nonnegative numeric field: " + label);
  }
  if (["Pid", "Handles", "Threads"].includes(label) && !Number.isSafeInteger(n)) {
    throw new Error("Expected integer field: " + label);
  }
  return n;
}

function simpleCells(line) {
  // PowerShell Export-Csv quotes each scalar; the schema contains no user text
  // capable of requiring embedded quotes, escaping or comma-separated strings.
  return line.split(",").map(part => {
    const text = part.trim();
    if (text.startsWith('"') || text.endsWith('"')) {
      if (!/^"[^"]*"$/.test(text)) throw new Error("Unsupported CSV quoted cell");
      return text.slice(1, -1);
    }
    if (text.includes('"')) throw new Error("Invalid CSV quotation");
    return text;
  });
}

export function parseM12bCsv(source) {
  if (typeof source !== "string" || source.length > 8 * 1024 * 1024) {
    throw new Error("CSV input must be text under 8 MiB");
  }
  const lines = source.replace(/^\uFEFF/, "").trim().split(/\r?\n/);
  const headings = simpleCells(lines.shift() ?? "");
  if (headings.join("|") !== COLUMNS.join("|")) throw new Error("Unexpected M12-B CSV schema");
  if (lines.length < 3 || lines.length > MAX_ROWS) throw new Error("Need 3 to 20,000 samples");
  let lastElapsed = -1;
  let pid = null;
  let stage = null;
  const records = [];
  for (const line of lines) {
    const cells = simpleCells(line);
    if (cells.length !== COLUMNS.length) throw new Error("CSV column count mismatch");
    const raw = Object.fromEntries(headings.map((key, i) => [key, cells[i]]));
    if (!/^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d/.test(raw.TimestampUtc) ||
        !Number.isFinite(Date.parse(raw.TimestampUtc))) {
      throw new Error("Invalid TimestampUtc");
    }
    if (!/^[a-zA-Z0-9_-]{1,32}$/.test(raw.Stage)) throw new Error("Invalid Stage label");
    const row = { TimestampUtc: raw.TimestampUtc, Stage: raw.Stage };
    for (const key of NUMERICS) row[key] = numeric(raw[key], key);
    if (row.CpuPercentMachine > 100) throw new Error("Invalid host-normalized CPU percent");
    if (pid !== null && row.Pid !== pid) throw new Error("Process PID changed during capture");
    if (stage !== null && row.Stage !== stage) throw new Error("Stage changed during capture");
    if (row.ElapsedSec <= lastElapsed) throw new Error("Non-increasing elapsed time");
    pid = row.Pid; stage = row.Stage; lastElapsed = row.ElapsedSec;
    records.push(row);
  }
  return records;
}

function median(values) {
  const sorted = [...values].sort((a, b) => a - b);
  const middle = Math.floor(sorted.length / 2);
  return sorted.length % 2 === 0
    ? (sorted[middle - 1] + sorted[middle]) / 2
    : sorted[middle];
}

function slopePerHour(rows, key) {
  const hours = rows.map(r => r.ElapsedSec / 3600);
  const y = rows.map(r => r[key]);
  const meanX = hours.reduce((s, v) => s + v, 0) / hours.length;
  const meanY = y.reduce((s, v) => s + v, 0) / y.length;
  let covariance = 0;
  let variance = 0;
  for (let i = 0; i < rows.length; i++) {
    covariance += (hours[i] - meanX) * (y[i] - meanY);
    variance += (hours[i] - meanX) ** 2;
  }
  return variance === 0 ? 0 : covariance / variance;
}

function rounded(n) {
  return Math.round(n * 1000) / 1000;
}

/** Interpret as a trend, never a memory-leak proof or automatic release gate. */
export function summarizeM12b(rows, warmupSeconds = 120) {
  if (!Array.isArray(rows) || rows.length < 3) throw new Error("At least 3 samples required");
  if (!Number.isFinite(warmupSeconds) || warmupSeconds < 0) {
    throw new Error("Invalid warmupSeconds");
  }
  const start = rows[0].ElapsedSec;
  const kept = rows.filter(row => row.ElapsedSec >= start + warmupSeconds);
  if (kept.length < 3) {
    throw new Error("Not enough samples after warmup; shorten warmup or capture longer");
  }
  const size = Math.max(1, Math.min(Math.floor(kept.length / 2), Math.ceil(kept.length / 5)));
  const early = kept.slice(0, size);
  const late = kept.slice(-size);
  const metrics = {};
  for (const key of MEASURES) {
    const before = median(early.map(r => r[key]));
    const after = median(late.map(r => r[key]));
    metrics[key] = {
      baselineMedian: rounded(before),
      endMedian: rounded(after),
      delta: rounded(after - before),
      linearSlopePerHour: rounded(slopePerHour(kept, key)),
      max: rounded(Math.max(...kept.map(r => r[key]))),
    };
  }
  const cpuRows = kept.slice(1); // first process sample has no CPU delta
  const cpuAvg = cpuRows.reduce((s, v) => s + v.CpuPercentMachine, 0) / cpuRows.length;
  return {
    schemaVersion: 1,
    stage: kept[0].Stage,
    pid: kept[0].Pid,
    sampleCount: kept.length,
    totalSamples: rows.length,
    warmupSeconds,
    measuredMinutes: rounded((kept[kept.length - 1].ElapsedSec - kept[0].ElapsedSec) / 60),
    cpuPercentMachineAverage: rounded(cpuAvg),
    metrics,
    interpretation: "Trend only; growth is not proof of a leak, and a flat trend is not proof of zero leaks.",
  };
}
