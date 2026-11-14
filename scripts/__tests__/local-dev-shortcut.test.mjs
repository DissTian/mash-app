import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import test from "node:test";

const shortcut = fileURLToPath(new URL("../../dev/开发运行.command", import.meta.url));
const onMac = { skip: process.platform !== "darwin" };

function fixture(t) {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "mash dev shortcut test "));
  const root = fs.realpathSync(temp);
  const parent = fs.realpathSync(os.tmpdir());
  assert.equal(path.dirname(root), parent);
  assert.ok(path.basename(root).startsWith("mash dev shortcut test "));
  t.after(() => {
    assert.equal(path.dirname(fs.realpathSync(root)), parent);
    fs.rmSync(root, { recursive: true, force: true });
  });
  const repo = path.join(root, "repository with spaces");
  const home = path.join(root, "home with spaces");
  const tools = path.join(root, "tools");
  const entry = path.join(repo, "dev", path.basename(shortcut));
  for (const dir of [
    path.dirname(entry),
    path.join(repo, "src-tauri"),
    path.join(repo, "node_modules/@tauri-apps/cli"),
    path.join(repo, "node_modules/vite/bin"),
    tools,
  ]) {
    fs.mkdirSync(dir, { recursive: true });
  }
  fs.copyFileSync(shortcut, entry);
  fs.writeFileSync(path.join(repo, "package.json"), "{}");
  fs.writeFileSync(path.join(repo, "src-tauri/tauri.conf.json"), JSON.stringify({ version: "0.29.0", productName: "Mash" }));
  const cli = path.join(repo, "node_modules/@tauri-apps/cli/tauri.js");
  fs.writeFileSync(cli, "// stub tauri cli\n");
  const vite = path.join(repo, "node_modules/vite/bin/vite.js");
  fs.writeFileSync(vite, "// stub vite\n");

  const stub = (name, code) => {
    const file = path.join(tools, name);
    fs.writeFileSync(file, `#!${process.execPath}\n${code}\n`, { mode: 0o755 });
    return file;
  };
  // The preflight (`-e`) is short-circuited; every other call is recorded and
  // never starts a real dev session.
  const node = stub("node", `
    const fs = require('node:fs');
    const args = process.argv.slice(2);
    if (args[0] === '-e') {
      if (process.env.FIXTURE_NATIVE_FAIL && !process.argv[1].includes('mash-dev-node.')) process.exit(1);
      process.exit(0);
    }
    fs.appendFileSync(process.env.FIXTURE_TRACE, JSON.stringify({args, script: process.argv[1]}) + '\\n');
    process.exit(process.env.FIXTURE_DEV_FAIL ? 3 : 0);
  `);
  const calls = path.join(root, "calls.jsonl");
  const codesignCalls = path.join(root, "codesign.jsonl");
  stub("cargo", "process.exit(0)");
  stub("codesign", `
    const fs = require('node:fs');
    fs.appendFileSync(process.env.FIXTURE_CODESIGN_TRACE, JSON.stringify(process.argv.slice(2)) + '\\n');
    process.exit(0);
  `);
  stub("lsof", "if (process.env.FIXTURE_PORT_BUSY) { console.log('4242'); process.exit(0); } process.exit(1)");
  stub("pgrep", "process.exit(1)");

  const run = (extra = {}) => spawnSync("/bin/bash", [entry, ...(extra.args || [])], {
    cwd: root, encoding: "utf8", timeout: 20000,
    env: {
      ...process.env, HOME: home, TMPDIR: parent,
      PATH: `${tools}:/usr/bin:/bin:/usr/sbin:/sbin`,
      MASH_DEV_NODE: node, FIXTURE_TRACE: calls, FIXTURE_CODESIGN_TRACE: codesignCalls,
      MASH_FLOW_TRACE: "", ...extra,
    },
  });
  const trace = () => (fs.existsSync(calls)
    ? fs.readFileSync(calls, "utf8").trim().split("\n").map(JSON.parse)
    : []);
  const codesigns = () => (fs.existsSync(codesignCalls)
    ? fs.readFileSync(codesignCalls, "utf8").trim().split("\n").map(JSON.parse)
    : []);
  const logs = () => fs.readdirSync(path.join(repo, "dev/logs")).filter(name => name.startsWith("dev-") && name.endsWith(".log"));
  return { root, repo, home, tools, entry, cli, vite, node, calls, run, trace, codesigns, logs };
}

function configOf(args) {
  const index = args.indexOf("--config");
  assert.notEqual(index, -1, "dev invocation must pass --config");
  return JSON.parse(args[index + 1]);
}

test("dev shortcut starts tauri dev with flow tracing and a pnpm-free vite command", onMac, t => {
  const f = fixture(t);
  const result = f.run();
  assert.equal(result.status, 0, result.stdout + result.stderr);
  const calls = f.trace();
  assert.equal(calls.length, 1);
  const [call] = calls;
  assert.equal(call.args[0], f.cli);
  assert.deepEqual(call.args.slice(1, 3), ["dev", "--features"]);
  assert.equal(call.args[3], "flow-trace");
  const config = configOf(call.args);
  assert.ok(config.build.beforeDevCommand.startsWith(`cd "${f.repo}" && `), config.build.beforeDevCommand);
  assert.ok(config.build.beforeDevCommand.includes(f.vite), config.build.beforeDevCommand);
  assert.ok(!config.build.beforeDevCommand.includes("pnpm"), "must not depend on a global pnpm");
  assert.match(result.stdout, /开发模式/);
  assert.equal(f.logs().length, 1);
});

test("dev shortcut leaves flow tracing off when MASH_FLOW_TRACE=0", onMac, t => {
  const f = fixture(t);
  const result = f.run({ MASH_FLOW_TRACE: "0" });
  assert.equal(result.status, 0, result.stdout + result.stderr);
  const [call] = f.trace();
  assert.deepEqual(call.args.slice(1, 3), ["dev", "--config"]);
  assert.ok(!call.args.includes("--features"));
  assert.match(result.stdout, /流程追踪：关闭/);
});

test("dev shortcut fails before starting when Vite's strict port is busy", onMac, t => {
  const f = fixture(t);
  const result = f.run({ FIXTURE_PORT_BUSY: "1" });
  assert.notEqual(result.status, 0);
  assert.match(result.stdout + result.stderr, /1420/);
  assert.deepEqual(f.trace(), []);
});

test("dev shortcut reports a missing Node instead of starting", onMac, t => {
  const f = fixture(t);
  fs.rmSync(path.join(f.tools, "node"));
  const result = f.run({ MASH_DEV_NODE: "" });
  assert.notEqual(result.status, 0);
  assert.match(result.stdout + result.stderr, /找不到 Node/);
  assert.deepEqual(f.trace(), []);
});

test("dev shortcut reports a missing Cargo toolchain", onMac, t => {
  const f = fixture(t);
  fs.rmSync(path.join(f.tools, "cargo"));
  const result = f.run();
  assert.notEqual(result.status, 0);
  assert.match(result.stdout + result.stderr, /找不到 Cargo/);
  assert.deepEqual(f.trace(), []);
});

test("dev shortcut re-signs a disposable Node copy when the native addon cannot load", onMac, t => {
  const f = fixture(t);
  const result = f.run({ FIXTURE_NATIVE_FAIL: "1" });
  assert.equal(result.status, 0, result.stdout + result.stderr);
  const codesigns = f.codesigns();
  assert.equal(codesigns.length, 1);
  assert.deepEqual(codesigns[0].slice(0, 2), ["--force", "--sign"]);
  assert.ok(codesigns[0].at(-1).includes("mash-dev-node."));
  const [call] = f.trace();
  assert.ok(call.script.includes("mash-dev-node."), `expected the temporary Node copy, got ${call.script}`);
  assert.ok(configOf(call.args).build.beforeDevCommand.includes("mash-dev-node."));
  assert.match(result.stdout, /临时 Node 副本/);
});

test("dev shortcut --dry-run prints the command without starting tauri", onMac, t => {
  const f = fixture(t);
  const result = f.run({ args: ["--dry-run"] });
  assert.equal(result.status, 0, result.stdout + result.stderr);
  assert.match(result.stdout, /将执行：/);
  assert.match(result.stdout, /\bdev\b/);
  assert.deepEqual(f.trace(), []);
  assert.deepEqual(f.logs(), []);
});

test("dev shortcut --help documents the shortcuts and exits cleanly", onMac, t => {
  const f = fixture(t);
  const result = f.run({ args: ["--help"] });
  assert.equal(result.status, 0, result.stdout + result.stderr);
  assert.match(result.stdout, /tauri dev/);
  assert.deepEqual(f.trace(), []);
});
