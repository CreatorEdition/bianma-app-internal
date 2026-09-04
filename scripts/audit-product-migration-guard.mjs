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
const requestedModes = ["--staged", "--worktree", "--committed"].filter(
  (flag) => args.includes(flag),
);
if (requestedModes.length > 1) {
  console.error("迁移 denylist 审计模式不能同时指定多个模式。");
  process.exit(2);
}
const mode = args.includes("--committed")
  ? "committed"
  : args.includes("--worktree")
    ? "worktree"
    : "staged";
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

const runGitRaw = (gitArgs) =>
  execFileSync("git", ["-c", "core.quotePath=false", ...gitArgs], {
    cwd: repoRoot,
    encoding: "utf8",
  });

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

// Committed-tree mode is intentionally limited to the collaboration directory.
// The remaining entries describe migration content and are not promoted to
// permanent public-repository path policy by this change.
const committedPathBlockers = pathBlockers.filter(
  ([label]) => label === "内部协作目录",
);

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

const getArgValue = (flag) => {
  const index = args.indexOf(flag);
  if (index < 0) {
    return undefined;
  }
  return args[index + 1];
};

const isZeroObjectId = (value) => /^0+$/.test(value ?? "");

const verifyCommit = (value, label) => {
  if (!value || isZeroObjectId(value)) {
    failures.push(`${label} 缺失或为全零对象，拒绝降级审计。`);
    return null;
  }

  try {
    return runGit([
      "rev-parse",
      "--verify",
      "--end-of-options",
      `${value}^{commit}`,
    ]);
  } catch {
    failures.push(`${label} 无法解析为提交对象，拒绝降级审计。`);
    return null;
  }
};

const verifyTree = (value) => {
  if (!value || isZeroObjectId(value)) {
    failures.push("最终 tree 缺失或为全零对象，拒绝降级审计。");
    return null;
  }

  for (const suffix of ["^{commit}", "^{tree}"]) {
    try {
      runGit([
        "rev-parse",
        "--verify",
        "--end-of-options",
        `${value}${suffix}`,
      ]);
      return value;
    } catch {
      // Try the other supported object type before failing closed.
    }
  }

  failures.push("最终 tree 无法解析为提交或 tree 对象，拒绝降级审计。");
  return null;
};

const listTreePaths = (treeish) =>
  runGitRaw(["ls-tree", "-r", "-z", "--name-only", treeish])
    .split("\0")
    .filter(Boolean)
    .map(normalizePath);

const shortObjectId = (value) => value.slice(0, 12);

const checkCommittedTree = (treeish, sourceLabel, seenFailures) => {
  let paths;
  try {
    paths = listTreePaths(treeish);
  } catch {
    failures.push(
      `${sourceLabel} ${shortObjectId(treeish)} 无法读取 Git tree，拒绝降级审计。`,
    );
    return;
  }

  for (const relativePath of paths) {
    for (const [label, pattern] of committedPathBlockers) {
      if (!pattern.test(relativePath)) {
        continue;
      }

      const key = `${label}:${sourceLabel}:${shortObjectId(treeish)}`;
      if (!seenFailures.has(key)) {
        seenFailures.add(key);
        failures.push(
          `提交树包含禁止路径（${label}，${sourceLabel} ${shortObjectId(treeish)}）。`,
        );
      }
    }
  }
};

const auditCommittedObjects = () => {
  const tree = verifyTree(getArgValue("--tree"));
  if (!tree) {
    return;
  }

  let shallowRepository = false;
  try {
    shallowRepository =
      runGit(["rev-parse", "--is-shallow-repository"]) === "true";
  } catch {
    failures.push("无法确定 Git 历史是否完整，拒绝降级审计。");
  }
  if (shallowRepository) {
    failures.push(
      "Git 仓库是 shallow clone，拒绝在不完整历史上执行提交树审计。",
    );
  }

  const baseValue = getArgValue("--base");
  const headValue = getArgValue("--head");
  if ((baseValue && !headValue) || (!baseValue && headValue)) {
    failures.push("--base 和 --head 必须同时提供，拒绝部分范围审计。");
  }

  const seenFailures = new Set();
  checkCommittedTree(tree, "tree", seenFailures);

  if (!baseValue && !headValue) {
    return;
  }

  const base = verifyCommit(baseValue, "base 提交");
  const head = verifyCommit(headValue, "head 提交");
  if (!base || !head || shallowRepository) {
    return;
  }

  let commits;
  try {
    commits = runGit(["rev-list", "--reverse", `${base}..${head}`])
      .split(/\r?\n/)
      .filter(Boolean);
  } catch {
    failures.push("无法枚举 base..head 提交范围，拒绝降级审计。");
    return;
  }

  for (const commit of commits) {
    checkCommittedTree(commit, "commit", seenFailures);
  }
};

let candidatePathCount = null;
if (mode === "committed") {
  auditCommittedObjects();
} else {
  const candidatePaths = getCandidatePaths();
  candidatePathCount = candidatePaths.length;
  for (const relativePath of candidatePaths) {
    checkPathBlockers(relativePath);
  }
  checkDiffContent(candidatePaths);
}
checkBoundaryDocs();

if (failures.length > 0) {
  console.error("product 迁移 denylist 审计失败：");
  for (const failure of failures) {
    console.error(`- ${failure}`);
  }
  process.exit(1);
}

console.log(
  `product 迁移 denylist 审计通过：mode=${mode}${candidatePathCount === null ? "" : `，候选文件=${candidatePathCount}`}，禁迁边界文档仍完整。`,
);
