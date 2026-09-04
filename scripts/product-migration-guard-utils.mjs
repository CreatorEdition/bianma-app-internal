export const normalizePath = (value) =>
  value.replace(/\\/g, "/").replace(/^\.\//, "");

export const boundaryFiles = new Set(
  [
    "scripts/audit-product-migration-guard.mjs",
    "scripts/product-migration-guard-utils.mjs",
    "scripts/audit-public-release-preflight.mjs",
    "tests/scripts/audit-product-migration-guard.test.mjs",
    "docs/open-source-migration/product-diff-and-release-audit-2026-07-07.md",
    "docs/open-source-migration/private-material-audit-2026-07-07.md",
    "docs/open-source-migration/public-release-gate-runbook-2026-07-07.md",
    "docs/open-source-migration/public-release-approval-checklist-2026-07-08.md",
    "docs/open-source-migration/public-updater-latest-json-gate-2026-07-08.md",
    "task.md",
  ].map(normalizePath),
);

export const contentBlockers = [
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

export const findContentBlockerLabels = (relativePath, text) => {
  const normalizedPath = normalizePath(relativePath);
  return contentBlockers
    .filter(([, pattern, options = {}]) => {
      return (
        (options.scanBoundary || !boundaryFiles.has(normalizedPath)) &&
        pattern.test(text)
      );
    })
    .map(([label]) => label);
};

/** Return only added lines, retaining the repository-relative path. */
export const parseAddedDiff = (diff) => {
  const entries = [];
  let currentPath = "";

  for (const line of diff.split(/\r?\n/)) {
    if (line.startsWith("+++ b/")) {
      currentPath = normalizePath(line.slice("+++ b/".length));
      continue;
    }

    if (currentPath && line.startsWith("+") && !line.startsWith("+++")) {
      entries.push({ path: currentPath, text: line.slice(1) });
    }
  }

  return entries;
};
