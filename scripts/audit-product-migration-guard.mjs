import { execFileSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "..");

const normalizePath = (value) => value.replace(/\\/g, "/").replace(/^\.\//, "");
const resolveRepoPath = (relativePath) => path.join(repoRoot, relativePath);
const readText = (relativePath) =>
  readFileSync(resolveRepoPath(relativePath), "utf8");

const failures = [];
const args = process.argv.slice(2);
const mode = args.includes("--worktree") ? "worktree" : "staged";

// Node 子进程在 Windows 沙箱中可能不会继承宿主 shell 注入的
// `safe.directory` 配置；仅对本次 git 子进程信任其工作目录，避免审计退化为 `--no-index`。
const runGit = (gitArgs) =>
  execFileSync("git", ["-c", "safe.directory=*", ...gitArgs], {
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

const boundaryFiles = new Set(
  [
    "scripts/audit-product-migration-guard.mjs",
    "scripts/audit-public-release-preflight.mjs",
    "docs/open-source-migration/product-diff-and-release-audit-2026-07-07.md",
    "docs/open-source-migration/private-material-audit-2026-07-07.md",
    "docs/open-source-migration/public-release-gate-runbook-2026-07-07.md",
    "docs/open-source-migration/public-release-approval-checklist-2026-07-08.md",
    "task.md",
  ].map(normalizePath),
);

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

const contentBlockers = [
  ["私有域名 data.bianma.ai", /data\.bianma\.ai/i],
  ["provider rule center", /providerRule(?:Registry|Center)/i],
  ["多 key 池", /apiKeyPool/i],
  ["Session Cloud", /SessionCloud|session_cloud/i],
  ["Risk Guard", /Risk Guard|RiskGuard/i],
  ["Local Policy", /Local Policy|LocalPolicy|local_policy/i],
  ["Keyword Guard", /Keyword Guard|KeywordGuard|keyword_guard/i],
  ["私有发布私钥变量", /TAURI_SIGNING_PRIVATE_KEY/],
  [
    "Apple 发布证书变量",
    /APPLE_CERTIFICATE|APPLE_PASSWORD|APPLE_ID|APPLE_TEAM_ID|KEYCHAIN_PASSWORD/,
  ],
  ["GitHub 发布 token", /\b(GITHUB_TOKEN|GH_TOKEN)\b/],
  ["GitHub release 命令", /\bgh\s+release\s+(create|edit|upload)\b/i],
  [
    "GitHub release action",
    /softprops\/action-gh-release|actions\/create-release/i,
  ],
  ["Tauri 发布 action", /tauri-apps\/tauri-action/i],
  ["Artifact 上传 action", /actions\/upload-artifact/i],
  ["contents 写权限", /\bcontents:\s*write\b/i],
  ["id-token 写权限", /\bid-token:\s*write\b/i],
  [
    "latest.json 生成或上传",
    /(upload|generate|generated).*latest\.json|latest\.json.*(upload|gh release)/i,
  ],
  ["release-assets 上传", /release-assets/i],
  ["notarization 私有链路", /\bnotarytool\b|notarization|notarize/i],
  ["私有 release 仓变量", /\bRELEASE_REPO\b/],
  ["合作方推广材料", /partnerPromotion|affiliate|referral|sponsor/i],
  ["内部规格引用", /docs\/internal-spec|docs\\internal-spec/i],
  ["私钥块", /BEGIN [A-Z ]*PRIVATE KEY/, { scanBoundary: true }],
  ["OpenAI token-like 示例", /sk-[A-Za-z0-9]{8,}/, { scanBoundary: true }],
  ["Google token-like 示例", /AIza[0-9A-Za-z_-]{10,}/, { scanBoundary: true }],
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

const shouldScanContent = (relativePath) =>
  !boundaryFiles.has(normalizePath(relativePath));

const checkPathBlockers = (relativePath) => {
  for (const [label, pattern] of pathBlockers) {
    assertCondition(
      !pattern.test(relativePath),
      `禁止迁入 ${label}：${relativePath}`,
    );
  }
};

const checkContentBlockers = (relativePath, text) => {
  for (const [label, pattern, options = {}] of contentBlockers) {
    if (!options.scanBoundary && !shouldScanContent(relativePath)) {
      continue;
    }

    assertCondition(!pattern.test(text), `禁止迁入 ${label}：${relativePath}`);
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

const checkDiffContent = (candidatePaths) => {
  if (mode === "worktree") {
    for (const relativePath of candidatePaths) {
      checkFileContent(relativePath);
    }
    return;
  }

  const diff = runGit(["diff", "--cached", "--unified=0", "--no-ext-diff"]);
  let currentPath = "";

  for (const line of diff.split(/\r?\n/)) {
    if (line.startsWith("+++ b/")) {
      currentPath = normalizePath(line.slice("+++ b/".length));
      continue;
    }

    if (!line.startsWith("+") || line.startsWith("+++")) {
      continue;
    }

    checkContentBlockers(currentPath, line.slice(1));
  }
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
