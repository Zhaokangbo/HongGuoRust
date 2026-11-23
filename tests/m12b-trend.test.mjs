import { test } from "node:test";
import assert from "node:assert/strict";
import { COLUMNS, parseM12bCsv, summarizeM12b } from "../scripts/m12b-trend.mjs";

const rows = Array.from({ length: 20 }, (_, i) => ({
  TimestampUtc: new Date(Date.UTC(2026, 10, 21, 12, i, 0)).toISOString(),
  ElapsedSec: i * 60,
  Pid: 5123,
  Stage: "local_playback",
  CpuSeconds: 3 + i,
  CpuPercentMachine: i === 0 ? 0 : 2,
  WorkingSetMiB: 160,
  PrivateMiB: 100 + i,
  Handles: 100,
  Threads: 12,
}));

function csv(values = rows) {
  return COLUMNS.join(",") + "\r\n" +
    values.map(v => COLUMNS.map(key => '"' + v[key] + '"').join(",")).join("\r\n") + "\r\n";
}

test("PowerShell Export-Csv-compatible BOM and CRLF rows parse safely", () => {
  const parsed = parseM12bCsv("\uFEFF" + csv());
  assert.deepEqual(parsed, rows);
  assert.equal(parsed[0].CpuPercentMachine, 0);
});

test("warmup-aware baseline/ending median and regression slope", () => {
  const parsed = parseM12bCsv(csv());
  const result = summarizeM12b(parsed, 120);
  assert.equal(result.sampleCount, 18);
  assert.equal(result.totalSamples, 20);
  assert.equal(result.measuredMinutes, 17);
  assert.equal(result.metrics.PrivateMiB.baselineMedian, 103.5);
  assert.equal(result.metrics.PrivateMiB.endMedian, 117.5);
  assert.equal(result.metrics.PrivateMiB.delta, 14);
  assert.equal(result.metrics.PrivateMiB.linearSlopePerHour, 60);
  assert.equal(result.metrics.Handles.delta, 0);
  assert.equal(result.metrics.WorkingSetMiB.linearSlopePerHour, 0);
  assert.equal(result.cpuPercentMachineAverage, 2);
  assert.match(result.interpretation, /not proof of a leak/);
});

test("rejects missing headers, corrupt fields, PID changes and stage injection", () => {
  assert.throws(() => parseM12bCsv("nonsense"), /schema/);
  assert.throws(() => parseM12bCsv(csv(rows.slice(0, 2))), /3 to 20,000/);
  assert.throws(() => parseM12bCsv(csv(rows.map((r,i) =>
    i === 7 ? { ...r, Pid: 999 } : r))), /PID changed/);
  assert.throws(() => parseM12bCsv(csv(rows.map((r,i) =>
    i === 7 ? { ...r, Stage: "https://secret" } : r))), /Invalid Stage/);
  assert.throws(() => parseM12bCsv(csv(rows.map((r,i) =>
    i === 7 ? { ...r, ElapsedSec: rows[6].ElapsedSec } : r))), /Non-increasing/);
  assert.throws(() => parseM12bCsv(csv(rows.map((r,i) =>
    i === 7 ? { ...r, PrivateMiB: Number.NaN } : r))), /Invalid nonnegative/);
  assert.throws(() => parseM12bCsv(csv(rows.map((r,i) =>
    i === 7 ? { ...r, Handles: -3 } : r))), /Invalid nonnegative/);
  assert.throws(() => parseM12bCsv(csv(rows.map((r,i) =>
    i === 7 ? { ...r, CpuPercentMachine: 101 } : r))), /host-normalized/);
});

test("invalid warmup or insufficient data fails instead of printing a false trend", () => {
  const parsed = parseM12bCsv(csv());
  assert.throws(() => summarizeM12b(parsed, -1), /Invalid warmup/);
  assert.throws(() => summarizeM12b(parsed, 1080), /Not enough samples/);
});

test("flat growth is zero but still not declared leak-free", () => {
  const flat = rows.map(r => ({ ...r, PrivateMiB: 100, Handles: 60 }));
  const report = summarizeM12b(parseM12bCsv(csv(flat)), 0);
  assert.equal(report.metrics.PrivateMiB.delta, 0);
  assert.equal(report.metrics.PrivateMiB.linearSlopePerHour, 0);
  assert.match(report.interpretation, /not proof of zero leaks/);
});
