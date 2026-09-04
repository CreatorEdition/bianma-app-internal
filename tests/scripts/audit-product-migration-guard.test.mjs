import {
  appendFileSync,
  cpSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { execFileSync } from "node:child_process";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { afterEach, describe, expect, it } from "vitest";
import { parseAddedDiff } from "../../scripts/product-migration-guard-utils.mjs";

const repositoryRoot = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "../..",
);
const guardScript = path.join(
  repositoryRoot,
  "scripts",
  "audit-product-migration-guard.mjs",
);
const requiredBoundaryDocs = [
  "docs/open-source-migration/product-diff-and-release-audit-2026-07-07.md",
  "docs/open-source-migration/private-material-audit-2026-07-07.md",
];
const fixtureRoots = [];

const runGit = (cwd, args) =>
  execFileSync("git", ["-c", "core.quotePath=false", ...args], {
    cwd,
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  });

const createFixture = () => {
  const root = mkdtempSync(path.join(os.tmpdir(), "bianma-migration-guard-"));
  fixtureRoots.push(root);

  for (const relativePath of requiredBoundaryDocs) {
    const source = path.join(repositoryRoot, relativePath);
    const destination = path.join(root, relativePath);
    mkdirSync(path.dirname(destination), { recursive: true });
    cpSync(source, destination);
  }

  const baselinePath = path.join(root, "src", "legacy-note.txt");
  mkdirSync(path.dirname(baselinePath), { recursive: true });
  writeFileSync(baselinePath, "historical notarization note\n", "utf8");

  runGit(root, ["init", "--quiet"]);
  runGit(root, ["config", "user.name", "Migration Guard Test"]);
  runGit(root, ["config", "user.email", "migration-guard@example.invalid"]);
  runGit(root, ["add", "."]);
  runGit(root, ["commit", "--quiet", "-m", "baseline"]);
  return root;
};

const runGuard = (root) => {
  try {
    const output = execFileSync(
      process.execPath,
      [guardScript, "--worktree", "--repo-root", root],
      { cwd: root, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] },
    );
    return { status: 0, output };
  } catch (error) {
    return {
      status: error.status ?? -1,
      output: `${error.stdout ?? ""}${error.stderr ?? ""}`,
    };
  }
};

afterEach(() => {
  for (const root of fixtureRoots.splice(0)) {
    if (existsSync(root)) {
      rmSync(root, { recursive: true, force: true });
    }
  }
});

describe("audit-product-migration-guard worktree mode", () => {
  it("ignores forbidden text that predates the current worktree diff", () => {
    const root = createFixture();
    appendFileSync(
      path.join(root, "src", "legacy-note.txt"),
      "current safe edit\n",
      "utf8",
    );

    const result = runGuard(root);

    expect(result.status).toBe(0);
    expect(result.output).toContain("审计通过");
  });

  it("blocks newly added forbidden content", () => {
    const root = createFixture();
    appendFileSync(
      path.join(root, "src", "legacy-note.txt"),
      "new affiliate content\n",
      "utf8",
    );

    const result = runGuard(root);

    expect(result.status).toBe(1);
    expect(result.output).toContain("合作方推广材料");
  });

  it("preserves non-ASCII paths when reporting a new blocker", () => {
    const root = createFixture();
    const relativePath = "docs/\u7528\u6237\u624b\u518c.md";
    const absolutePath = path.join(root, relativePath);
    mkdirSync(path.dirname(absolutePath), { recursive: true });
    writeFileSync(absolutePath, "new affiliate content\n", "utf8");

    const result = runGuard(root);

    expect(result.status).toBe(1);
    expect(result.output).toContain(relativePath);
    expect(result.output).toContain("合作方推广材料");
  });

  it("parses added lines without treating the file header as content", () => {
    const relativePath = "docs/\u7528\u6237\u624b\u518c.md";
    const entries = parseAddedDiff(
      `diff --git a/${relativePath} b/${relativePath}\n+++ b/${relativePath}\n@@ -1 +1 @@\n-old\n+new\n`,
    );

    expect(entries).toEqual([{ path: relativePath, text: "new" }]);
  });
});
