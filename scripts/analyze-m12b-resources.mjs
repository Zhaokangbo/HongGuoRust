#!/usr/bin/env node
/** Read-only offline analysis of one M12-B sampler CSV, Node.js 22+/24+. */
import { readFile, stat } from "node:fs/promises";
import { parseM12bCsv, summarizeM12b } from "./m12b-trend.mjs";

function usage() {
  return "Usage: node scripts/analyze-m12b-resources.mjs --csv <file> [--warmup-seconds 120] [--json]";
}

async function main(args) {
  let file = "";
  let warmupSeconds = 120;
  let json = false;
  for (let i = 0; i < args.length; i++) {
    const flag = args[i];
    if (flag === "--csv" && i + 1 < args.length && !file) {
      file = args[++i];
    } else if (flag === "--warmup-seconds" && i + 1 < args.length) {
      const value = args[++i];
      if (!/^\d+$/.test(value) || Number(value) > 7200) throw new Error("Invalid warmup time");
      warmupSeconds = Number(value);
    } else if (flag === "--json") {
      json = true;
    } else {
      throw new Error("Unsupported argument: " + flag);
    }
  }
  if (!file) throw new Error(usage());
  const size = (await stat(file)).size;
  if (size <= 0 || size > 8 * 1024 * 1024) throw new Error("CSV must be under 8 MiB");
  const rows = parseM12bCsv(await readFile(file, "utf8"));
  const report = summarizeM12b(rows, warmupSeconds);
  if (json) {
    process.stdout.write(JSON.stringify(report, null, 2) + "\n");
    return;
  }
  process.stdout.write(
    "M12-B resource trend (one opt-in PID; warmup excluded)\n" +
    "Stage=" + report.stage + " PID=" + report.pid + " Samples=" + report.sampleCount +
    "/" + report.totalSamples + " MeasuredMinutes=" + report.measuredMinutes + "\n" +
    "Metric                 first median    last median   delta    OLS slope/hour   max\n"
  );
  for (const [name, value] of Object.entries(report.metrics)) {
    const line = [
      name.padEnd(22),
      String(value.baselineMedian).padStart(10),
      String(value.endMedian).padStart(13),
      String(value.delta).padStart(9),
      String(value.linearSlopePerHour).padStart(16),
      String(value.max).padStart(9),
    ].join(" ");
    process.stdout.write(line + "\n");
  }
  process.stdout.write(
    "Host-normalized CPU avg: " + report.cpuPercentMachineAverage + "% (sample-based)\n" +
    "No automatic leak verdict: compare >=2 repeat runs under the same workload.\n" +
    "Duration, warmup, workload, memory allocator, caches and external process measurements affect the trend.\n"
  );
}

if (process.argv[1] && /(?:^|[/\\])analyze-m12b-resources\.mjs$/.test(process.argv[1])) {
  main(process.argv.slice(2)).catch(error => {
    process.stderr.write("M12-B analyzer: " + error.message + "\n");
    process.exitCode = 1;
  });
}
