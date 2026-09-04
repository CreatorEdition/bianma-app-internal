import { execFileSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import {
  findContentBlockerLabels,
  normalizePath,
  parseAddedDiff,
} from "./product-migration-guard-utils.mjs";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const args = process.argv.slice(2);
const mode = args.includes("--worktree") ? "worktree" : "staged";
const repoRootArgIndex = args.indexOf("--repo-root");
const repoRoot = path.resolve(
  repoRootArgIndex >= 0
    ? (args[repoRootArgIndex + 1] ?? path.resolve(__dirname, ".."))
    : path.resolve(__dirname, ".."),
);

const resolveRepoPath = (relativePath) => path.join(repoRoot, relativePath);
const readText = (relativePath) =>
  readFileSync(resolveRepoPath(relativePath), "utf8");

const failures = [];

const runGit = (gitArgs) =>
  execFileSync("git", ["-c", "core.quotePath=false", ...gitArgs], {
    cwd: repoRoot,
    encoding: "utf8",
  }).trim();

const listGit = (gitArgs) =>
  runGit(gitArgs).split(/\r?\n/).filter(Boolean).map(normalizePath);

const assertCondition = (condition, message) => {
  if (!condition) {
    failures.push(message);
  }
};

const pathBlockers = [
  ["内部协作目录", /(^|\/)\.teamwork(\/|$)/i],
  ["内部规格目录", /(^|\/)docs\/internal-spec(\/|$)/i],
  ["测试运行残留", /(^|\/)\.codex-vitest[^/]*\.json$/i],
  ["规则中心文件", /providerRule(?:Registry|Center)/i],
  ["Session Cloud 文件", /SessionCloud|session_cloud/i],
  ["Risk Guard 文件", /(^|\/)risk(\/|\.)|RiskGuard/i],
  ["Local Policy 文件", /LocalPolicy|local_policy/i],
  ["Keyword Guard 文件", /KeywordGuard|keyword_guard/i],
  ["多 key 池文件", /apiKeyPool|ApiKeyPool/i],
  ["私有策略链文件", /(^|\/)strategy(\/|\.)|load-balanc/i],
];

const requiredDocAssertions = [
  [
    "docs/open-source-migration/product-diff-and-release-audit-2026-07-07.md",
    [
      /\.teamwork\/\*\*/,
      /docs\/internal-spec\/\*\*/,
      /providerRuleCenter/,
      /Session Cloud/,
      /Risk Guard/,
      /Local Policy/,
      /Keyword Guard/,
      /apiKeyPool/,
      /partnerPromotionKey|affiliate\/referral\/sponsor/,
      /product `\.github\/workflows\/release\.yml`/,
    ],
  ],
  [
    "docs/open-source-migration/private-material-audit-2026-07-07.md",
    [
      /bianma-app-product\/\.teamwork\/\*\*/,
      /bianma-app-product\/docs\/internal-spec\/\*\*/,
      /TAURI_SIGNING_PRIVATE_KEY/,
      /APPLE_CERTIFICATE/,
      /data\.bianma\.ai/,
      /providerRuleCenter/,
      /Session Cloud/,
      /Risk Guard/,
      /Local Policy/,
      /Keyword Guard/,
      /apiKeyPool/,
      /partnerPromotion|affiliate|referral|sponsor/,
      /token-like 示例/,
      /GEMINI_OAUTH_CLIENT_SECRET/,
    ],
  ],
];

const getCandidatePaths = () => {
  if (mode === "worktree") {
    return [
      ...new Set([
        ...listGit(["diff", "--cached", "--name-only", "--diff-filter=ACMRT"]),
        ...listGit(["diff", "--name-only", "--diff-filter=ACMRT"]),
        ...listGit(["ls-files", "--others", "--exclude-standard"]),
      ]),
    ];
  }

  return listGit(["diff", "--cached", "--name-only", "--diff-filter=ACMRT"]);
};

const checkPathBlockers = (relativePath) => {
  for (const [label, pattern] of pathBlockers) {
    assertCondition(
      !pattern.test(relativePath),
      `禁止迁入 ${label}：${relativePath}`,
    );
  }
};

const checkContentBlockers = (relativePath, text) => {
  for (const label of findContentBlockerLabels(relativePath, text)) {
    failures.push(`禁止迁入 ${label}：${relativePath}`);
  }
};

const checkFileContent = (relativePath) => {
  const absolutePath = resolveRepoPath(relativePath);
  if (!existsSync(absolutePath)) {
    return;
  }

  try {
    checkContentBlockers(relativePath, readFileSync(absolutePath, "utf8"));
  } catch {
    // 二进制文件不参与文本 denylist 扫描；路径级 blocker 仍然生效。
  }
};

const checkAddedDiffContent = (diff) => {
  for (const { path: relativePath, text } of parseAddedDiff(diff)) {
    checkContentBlockers(relativePath, text);
  }
};

const checkDiffContent = (candidatePaths) => {
  if (mode === "worktree") {
    const trackedDiff = [
      runGit(["diff", "--cached", "--unified=0", "--no-ext-diff"]),
      runGit(["diff", "--unified=0", "--no-ext-diff"]),
    ].join("\n");
    checkAddedDiffContent(trackedDiff);

    const untrackedPaths = new Set(
      listGit(["ls-files", "--others", "--exclude-standard"]),
    );
    for (const relativePath of untrackedPaths) {
      checkFileContent(relativePath);
    }
    return;
  }

  checkAddedDiffContent(
    runGit(["diff", "--cached", "--unified=0", "--no-ext-diff"]),
  );
};

const checkBoundaryDocs = () => {
  for (const [relativePath, patterns] of requiredDocAssertions) {
    assertCondition(
      existsSync(resolveRepoPath(relativePath)),
      `迁移边界文档必须存在：${relativePath}`,
    );

    if (!existsSync(resolveRepoPath(relativePath))) {
      continue;
    }

    const text = readText(relativePath);
    for (const pattern of patterns) {
      assertCondition(
        pattern.test(text),
        `迁移边界文档缺少关键 denylist 说明：${relativePath} -> ${pattern}`,
      );
    }
  }
};

const candidatePaths = getCandidatePaths();
for (const relativePath of candidatePaths) {
  checkPathBlockers(relativePath);
}
checkDiffContent(candidatePaths);
checkBoundaryDocs();

if (failures.length > 0) {
  console.error("product 迁移 denylist 审计失败：");
  for (const failure of failures) {
    console.error(`- ${failure}`);
  }
  process.exit(1);
}

console.log(
  `product 迁移 denylist 审计通过：mode=${mode}，候选文件=${candidatePaths.length}，禁迁边界文档仍完整。`,
);
