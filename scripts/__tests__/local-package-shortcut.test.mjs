import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import test from "node:test";

const shortcut = fileURLToPath(new URL("../../dev/打包-AutoEnhance.command", import.meta.url));
const onMac = { skip: process.platform !== "darwin" };

function fixture(t) {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "mash shortcut test "));
  const root = fs.realpathSync(temp);
  const parent = fs.realpathSync(os.tmpdir());
  assert.equal(path.dirname(root), parent);
  assert.ok(path.basename(root).startsWith("mash shortcut test "));
  t.after(() => {
    assert.equal(path.dirname(fs.realpathSync(root)), parent);
    fs.rmSync(root, { recursive: true, force: true });
  });
  const repo = path.join(root, "repository with spaces");
  const home = path.join(root, "home with spaces");
  const bin = path.join(root, "tools");
  const desktop = path.join(home, "Desktop");
  const entry = path.join(repo, "dev", path.basename(shortcut));
  for (const dir of [path.dirname(entry), path.join(repo, "src-tauri"), path.join(repo, "node_modules/@tauri-apps/cli"), desktop, bin]) {
    fs.mkdirSync(dir, { recursive: true });
  }
  fs.copyFileSync(shortcut, entry);
  fs.writeFileSync(path.join(repo, "package.json"), "{}");
  fs.writeFileSync(path.join(repo, "src-tauri/tauri.conf.json"), JSON.stringify({ version: "0.29.0", productName: "Mash" }));
  const destination = path.join(desktop, "Mash-0.29.0-AutoEnhance.app");
  fs.mkdirSync(path.join(destination, "Contents/MacOS"), { recursive: true });
  fs.writeFileSync(path.join(destination, "Contents/MacOS/mash"), "previous app");
  const stub = (name, code) => {
    const file = path.join(bin, name);
    fs.writeFileSync(file, `#!${process.execPath}\n${code}\n`, { mode: 0o755 });
    return file;
  };
  const node = stub("node", `
    const { spawnSync } = require('node:child_process');
    if (process.argv[2] === '-e') process.exit(0); // Native CLI preflight, no real build in this fixture.
    const result = spawnSync(${JSON.stringify(process.execPath)}, process.argv.slice(2), {stdio:'inherit'});
    process.exit(result.status ?? 1);
  `);
  const pnpm = path.join(bin, "pnpm.mjs");
  fs.writeFileSync(pnpm, `
    import fs from 'node:fs'; import path from 'node:path';
    const args = process.argv.slice(2);
    fs.appendFileSync(process.env.FIXTURE_TRACE, JSON.stringify(args)+'\\n');
    fs.appendFileSync(process.env.FIXTURE_FEATURE_TRACE, JSON.stringify({
      bench: process.env.VITE_FEATURE_BENCH,
      cvDebug: process.env.VITE_FEATURE_CV_DEBUG,
    })+'\\n');
    if (args[0] === 'build') process.exit(process.env.FAIL_BUILD ? 4 : 0);
    if (args.join(' ').includes('tauri build')) {
      const config = JSON.parse(args.at(-1));
      if (config.bundle.createUpdaterArtifacts !== false || config.build.beforeBuildCommand !== '') process.exit(8);
      const app = path.join(process.env.CARGO_TARGET_DIR, 'release/bundle/macos/Mash.app');
      fs.mkdirSync(path.join(app,'Contents/MacOS'), {recursive:true});
      fs.writeFileSync(path.join(app,'Contents/MacOS/mash'), 'new app');
      fs.writeFileSync(path.join(app,'Contents/Info.plist'), JSON.stringify({CFBundleShortVersionString:'0.29.0',CFBundleExecutable:'mash'}));
    } else process.exit(9);
  `);
  stub("cargo", "process.exit(0)");
  stub("codesign", "process.exit(process.env.FAIL_SIGNATURE ? 1 : 0)");
  stub("lsof", "if (process.env.APP_RUNNING) console.log('12345'); else process.exit(1)");
  stub("mv", `
    const {spawnSync} = require('node:child_process');
    const args = process.argv.slice(2);
    if (process.env.FAIL_MOVE && args.some(a => a.includes('.mash-autoenhance-stage.') && a.endsWith('.app'))) process.exit(1);
    const result = spawnSync('/bin/mv', args, {stdio:'inherit'});
    process.exit(result.status ?? 1);
  `);
  const trace = path.join(root, "calls.jsonl");
  const featureTrace = path.join(root, "features.jsonl");
  const run = (extra = {}) => spawnSync("/bin/bash", [entry], {
    cwd: root, encoding: "utf8", timeout: 20000,
    env: { ...process.env, HOME: home, TMPDIR: parent, PATH: `${bin}:/usr/bin:/bin:/usr/sbin:/sbin`,
      MASH_BUILD_NODE: node, MASH_BUILD_PNPM: pnpm, FIXTURE_TRACE: trace,
      FIXTURE_FEATURE_TRACE: featureTrace, VITE_FEATURE_BENCH: "", VITE_FEATURE_CV_DEBUG: "", ...extra },
  });
  const previous = () => fs.readFileSync(path.join(destination, "Contents/MacOS/mash"), "utf8");
  const backups = () => fs.readdirSync(desktop).filter(name => name.includes(".previous-"));
  return { root, repo, desktop, destination, trace, featureTrace, run, previous, backups };
}

test("shortcut builds from another directory and copies the new app with a backup", onMac, t => {
  const f = fixture(t);
  const result = f.run();
  assert.equal(result.status, 0, result.stdout + result.stderr);
  assert.equal(f.previous(), "new app");
  assert.equal(f.backups().length, 1);
  const old = path.join(f.desktop, f.backups()[0], "Contents/MacOS/mash");
  assert.equal(fs.readFileSync(old, "utf8"), "previous app");
  assert.ok(!fs.readdirSync(f.desktop).some(name => name.startsWith(".mash-autoenhance-stage.")));
  assert.ok(!fs.existsSync(path.join(f.repo, "dev/logs/package-app.lock")));
  assert.deepEqual(fs.readFileSync(f.trace, "utf8").trim().split("\n").map(JSON.parse).map(args => args.slice(0, 3)), [["build"], ["exec", "tauri", "build"]]);
  assert.deepEqual(fs.readFileSync(f.featureTrace, "utf8").trim().split("\n").map(JSON.parse), [
    { bench: "1", cvDebug: "1" },
    { bench: "1", cvDebug: "1" },
  ]);
  assert.match(result.stdout, /打包模式：测试功能版/);
});

test("test build shortcut preserves explicit per-feature opt-outs", onMac, t => {
  const f = fixture(t);
  const result = f.run({ VITE_FEATURE_BENCH: "0", VITE_FEATURE_CV_DEBUG: "0" });
  assert.equal(result.status, 0, result.stdout + result.stderr);
  assert.deepEqual(fs.readFileSync(f.featureTrace, "utf8").trim().split("\n").map(JSON.parse), [
    { bench: "0", cvDebug: "0" },
    { bench: "0", cvDebug: "0" },
  ]);
});

for (const failure of ["FAIL_BUILD", "FAIL_SIGNATURE", "APP_RUNNING"]) {
  test(`shortcut leaves the desktop app intact on ${failure}`, onMac, t => {
    const f = fixture(t);
    const result = f.run({ [failure]: "1" });
    assert.notEqual(result.status, 0);
    assert.equal(f.previous(), "previous app");
    assert.deepEqual(f.backups(), []);
    const calls = fs.readFileSync(f.trace, "utf8").trim().split("\n").map(JSON.parse);
    assert.equal(calls.length, failure === "FAIL_BUILD" ? 1 : 2);
    if (failure === "APP_RUNNING") assert.match(result.stdout, /仍在运行/);
  });
}

test("shortcut refuses a symlink destination instead of changing its target", onMac, t => {
  const f = fixture(t);
  const actual = path.join(f.root, "external app.app");
  fs.renameSync(f.destination, actual);
  fs.symlinkSync(actual, f.destination);
  const result = f.run();
  assert.notEqual(result.status, 0);
  assert.equal(fs.readlinkSync(f.destination), actual);
  assert.equal(f.previous(), "previous app");
  assert.deepEqual(f.backups(), []);
  assert.ok(!fs.existsSync(f.trace), "Unsafe paths must fail before building");
});

test("shortcut restores the previous app when the final move fails", onMac, t => {
  const f = fixture(t);
  const result = f.run({ FAIL_MOVE: "1" });
  assert.notEqual(result.status, 0);
  assert.equal(f.previous(), "previous app");
  assert.deepEqual(f.backups(), []);
});
