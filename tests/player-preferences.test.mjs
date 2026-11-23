import { test } from "node:test";
import assert from "node:assert/strict";
import {
  validPlayerPreferences, readPlayerPreferences, persistLocalAutoNext,
} from "../src/player/preferences.ts";

const defaults = {
  schemaVersion: 1, volume: 100, lastAudibleVolume: 100,
  playbackRate: 1, autoNext: false,
};

test("validated preferences use a versioned, bounded metadata contract", () => {
  assert.equal(validPlayerPreferences(defaults), true);
  assert.equal(validPlayerPreferences({ ...defaults, autoNext: true, volume: 0, lastAudibleVolume: 25 }), true);
  assert.equal(validPlayerPreferences(null), false);
  assert.equal(validPlayerPreferences(undefined), false);
  assert.equal(validPlayerPreferences({ ...defaults, schemaVersion: 2 }), false);
  for (const volume of [-1, 101, NaN, Infinity]) {
    assert.equal(validPlayerPreferences({ ...defaults, volume }), false);
  }
  for (const playbackRate of [0.49, 2.01, NaN, -Infinity]) {
    assert.equal(validPlayerPreferences({ ...defaults, playbackRate }), false);
  }
  assert.equal(validPlayerPreferences({ ...defaults, lastAudibleVolume: 0 }), false);
  assert.equal(validPlayerPreferences({ ...defaults, autoNext: "true" }), false);
});

test("reading and updating preferences only invokes the native store", async () => {
  const invoked = [];
  const oldWindow = globalThis.window;
  globalThis.window = {
    __TAURI__: {
      core: {
        async invoke(name, args) {
          invoked.push({ name, args });
          if (name === "get_player_preferences") return defaults;
          if (name === "set_player_auto_next") return { ...defaults, autoNext: args.value };
          throw new Error("unexpected invocation: " + name);
        },
      },
    },
  };
  try {
    assert.deepEqual(await readPlayerPreferences(), defaults);
    assert.equal((await persistLocalAutoNext(true)).autoNext, true);
    assert.deepEqual(invoked, [
      { name: "get_player_preferences", args: undefined },
      { name: "set_player_auto_next", args: { value: true } },
    ]);
  } finally {
    globalThis.window = oldWindow;
  }
});

test("malformed native settings responses fail closed", async () => {
  const previous = globalThis.window;
  globalThis.window = { __TAURI__: { core: { invoke: async () => ({ ...defaults, volume: -20 }) } } };
  try {
    await assert.rejects(readPlayerPreferences(), /格式或版本/);
    await assert.rejects(persistLocalAutoNext(false), /响应无效/);
  } finally {
    globalThis.window = previous;
  }
});
