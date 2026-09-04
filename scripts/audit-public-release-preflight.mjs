import { execFileSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "..");
const publicRepositoryUrl =
  "https://github.com/CreatorEdition/bianma-app-internal";
const publicRepositoryGitUrl = `git+${publicRepositoryUrl}.git`;
const publicHomepageUrl = `${publicRepositoryUrl}#readme`;
const publicIssuesUrl = `${publicRepositoryUrl}/issues`;
const publicUpdaterEndpoint = `${publicRepositoryUrl}/releases/latest/download/latest.json`;

const resolveRepoPath = (relativePath) => path.join(repoRoot, relativePath);

const fileExists = (relativePath) => existsSync(resolveRepoPath(relativePath));

const readText = (relativePath) =>
  readFileSync(resolveRepoPath(relativePath), "utf8");

const readTextIfExists = (relativePath) =>
  fileExists(relativePath) ? readText(relativePath) : "";

const listGit = (args) =>
  execFileSync("git", args, {
    cwd: repoRoot,
    encoding: "utf8",
  })
    .split("\0")
    .filter(Boolean);

const failures = [];

const assertCondition = (condition, message) => {
  if (!condition) {
    failures.push(message);
  }
};

const assertUniqueField = (content, field, expectedValue, label) => {
  const escapedField = field.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const values = [
    ...content.matchAll(new RegExp(`^${escapedField}:\\s*(.*?)\\s*$`, "gm")),
  ].map((match) => match[1]);

  assertCondition(
    values.length === 1,
    `${label} 必须且只能声明一次 ${field}。`,
  );
  if (values.length === 1) {
    assertCondition(
      values[0] === expectedValue,
      `${label} 的 ${field} 必须保持 ${expectedValue}，当前为 ${values[0]}。`,
    );
  }
};

const workflow = readText(".github/workflows/release.yml");
const tauriConfig = JSON.parse(readText("src-tauri/tauri.conf.json"));
const packageJson = JSON.parse(readText("package.json"));
const cargoToml = readText("src-tauri/Cargo.toml");
const releaseApprovalChecklist = readTextIfExists(
  "docs/open-source-migration/public-release-approval-checklist-2026-07-08.md",
);
const updaterLatestJsonGate = readTextIfExists(
  "docs/open-source-migration/public-updater-latest-json-gate-2026-07-08.md",
);
const trackedReleaseArtifactPaths = listGit(["ls-files", "-z"]).filter(
  (trackedPath) => {
    const normalizedPath = trackedPath.replace(/\\/g, "/").toLowerCase();
    return (
      path.posix.basename(normalizedPath) === "latest.json" ||
      normalizedPath.startsWith("release-assets/")
    );
  },
);
const trackedWorkflowPaths = listGit([
  "ls-files",
  "-z",
  "--",
  ".github/workflows/*.yml",
  ".github/workflows/*.yaml",
]);
const trackedWorkflows = trackedWorkflowPaths.map((workflowPath) => [
  workflowPath,
  readText(workflowPath),
]);
const requiredFlatpakPaths = [
  "flatpak/com.ccswitch.desktop.desktop",
  "flatpak/com.ccswitch.desktop.metainfo.xml",
  "flatpak/com.ccswitch.desktop.yml",
  "flatpak/README.md",
];

const flatpakDesktopEntry = readTextIfExists(
  "flatpak/com.ccswitch.desktop.desktop",
);
const flatpakMetainfo = readTextIfExists(
  "flatpak/com.ccswitch.desktop.metainfo.xml",
);
const flatpakManifest = readTextIfExists("flatpak/com.ccswitch.desktop.yml");
const flatpakReadme = readTextIfExists("flatpak/README.md");
const deepLinkSchemes =
  tauriConfig?.plugins?.["deep-link"]?.desktop?.schemes ?? [];
const flatpakDesktopMimeTypes = (
  flatpakDesktopEntry.match(/^MimeType=(.*)$/m)?.[1] ?? ""
)
  .split(";")
  .filter(Boolean);
const cargoPackageVersion = cargoToml.match(
  /^\[package\][\s\S]*?^version\s*=\s*"([^"]+)"/m,
)?.[1];

for (const requiredFlatpakPath of requiredFlatpakPaths) {
  assertCondition(
    fileExists(requiredFlatpakPath),
    `Flatpak 兼容文件必须继续存在：${requiredFlatpakPath}`,
  );
}

assertCondition(
  workflow.includes("name: Public Release Preflight"),
  "release workflow 必须保持 Public Release Preflight 命名，避免伪装成正式发布流水线。",
);
assertCondition(
  workflow.includes("workflow_dispatch:"),
  "release workflow 必须只保留手动 workflow_dispatch 入口。",
);
assertCondition(
  /^permissions:\s*\r?\n\s*contents:\s*read\s*$/m.test(workflow),
  "release workflow 必须显式声明 permissions: contents: read。",
);
assertCondition(
  !/^\s*push:\s*$/m.test(workflow),
  "release workflow 当前不得恢复 tag/push 自动发布触发。",
);
assertCondition(
  !/^\s*tags:\s*$/m.test(workflow),
  "release workflow 当前不得恢复 tag 自动发布矩阵。",
);
assertCondition(
  !/^\s*workflow_run:\s*$/m.test(workflow),
  "release workflow 当前不得通过 workflow_run 自动接续发布。",
);
assertCondition(
  !/^\s*schedule:\s*$/m.test(workflow),
  "release workflow 当前不得通过 schedule 定时发布。",
);
assertCondition(
  !/^\s*release:\s*$/m.test(workflow),
  "release workflow 当前不得通过 release 事件发布。",
);
const forbiddenWorkflowPatterns = [
  ["contents: write 发布权限", /\bcontents:\s*write\b/],
  ["id-token: write 发布权限", /\bid-token:\s*write\b/],
  ["Tauri 私钥", /TAURI_SIGNING_PRIVATE_KEY/],
  ["Apple 证书", /APPLE_CERTIFICATE/],
  ["Apple 密码", /APPLE_PASSWORD/],
  ["Apple ID", /APPLE_ID/],
  ["Apple Team ID", /APPLE_TEAM_ID/],
  ["Apple keychain 密码", /KEYCHAIN_PASSWORD/],
  ["NotaryTool 公证", /\bnotarytool\b/i],
  ["GitHub Release 命令", /\bgh\s+release\s+(create|edit|upload)\b/i],
  [
    "GitHub Release Action",
    /softprops\/action-gh-release|actions\/create-release/i,
  ],
  ["Tauri 发布 action", /tauri-apps\/tauri-action/i],
  ["Artifact 上传", /actions\/upload-artifact/i],
  [
    "latest.json 上传",
    /(upload|generate|generated).*latest\.json|latest\.json.*(upload|gh release)/i,
  ],
  ["release-assets 上传", /release-assets/i],
  ["GitHub token 发布变量", /\b(GH_TOKEN|GITHUB_TOKEN)\b/],
  ["私有 release 仓变量", /\bRELEASE_REPO\b/],
];

for (const [label, pattern] of forbiddenWorkflowPatterns) {
  for (const [workflowPath, workflowContent] of trackedWorkflows) {
    const normalizedWorkflowPath = workflowPath.replace(/\\/g, "/");
    if (normalizedWorkflowPath !== ".github/workflows/release.yml") {
      continue;
    }

    assertCondition(
      !pattern.test(workflowContent),
      `公开 workflow ${workflowPath} 当前不得包含 ${label} 相关逻辑。`,
    );
  }
}

const endpoints = tauriConfig?.plugins?.updater?.endpoints ?? [];
assertCondition(
  endpoints.length === 1 && endpoints[0] === publicUpdaterEndpoint,
  `Tauri updater endpoint 必须精确指向 ${publicUpdaterEndpoint}。`,
);
for (const endpoint of endpoints) {
  assertCondition(
    endpoint === publicUpdaterEndpoint,
    `Tauri updater endpoint 不在公开 allowlist 中：${endpoint}`,
  );
}
assertCondition(
  tauriConfig?.bundle?.createUpdaterArtifacts === true,
  "Tauri 配置应保留 createUpdaterArtifacts=true，正式发布时由人工门禁决定是否上传。",
);
assertCondition(
  deepLinkSchemes.includes("bianma"),
  "Deep link 必须继续保留 bianma 当前公开 scheme。",
);
assertCondition(
  deepLinkSchemes.includes("ccswitch"),
  "Deep link 必须继续保留 ccswitch legacy scheme，避免破坏迁移兼容。",
);
assertCondition(
  packageJson?.repository?.url === publicRepositoryGitUrl,
  `package.json repository 必须精确指向 ${publicRepositoryGitUrl}。`,
);
assertCondition(
  packageJson?.homepage === publicHomepageUrl,
  `package.json homepage 必须精确指向 ${publicHomepageUrl}。`,
);
assertCondition(
  packageJson?.bugs?.url === publicIssuesUrl,
  `package.json bugs URL 必须精确指向 ${publicIssuesUrl}。`,
);
assertCondition(
  packageJson?.version === tauriConfig?.version &&
    packageJson?.version === cargoPackageVersion,
  `公开发布版本必须保持一致：package.json=${packageJson?.version}，tauri.conf.json=${tauriConfig?.version}，Cargo.toml=${cargoPackageVersion}。`,
);
assertCondition(
  packageJson?.version === "0.0.1",
  "正式版本策略获批前，公开仓必须继续保持 0.0.1 占位版本，避免误发正式版本。",
);
assertCondition(
  fileExists(
    "docs/open-source-migration/public-release-approval-checklist-2026-07-08.md",
  ),
  "公开发布人工审批 checklist 必须存在，避免正式发布门禁只停留在口头说明。",
);
assertCondition(
  /^Status:\s*BLOCKED\s*$/m.test(releaseApprovalChecklist),
  "公开发布人工审批 checklist 必须保持 Status: BLOCKED，直到签名、notarization、latest.json、构建矩阵、artifact 和人工审批全部完成。",
);
assertCondition(
  /^release_gate_status:\s*blocked\s*$/m.test(releaseApprovalChecklist),
  "公开发布人工审批 checklist 必须声明 release_gate_status: blocked。",
);
assertCondition(
  /^allows_real_release:\s*false\s*$/m.test(releaseApprovalChecklist),
  "公开发布人工审批 checklist 必须声明 allows_real_release: false。",
);
assertCondition(
  /^product_release_workflow_import:\s*forbidden\s*$/m.test(
    releaseApprovalChecklist,
  ),
  "公开发布人工审批 checklist 必须声明 product_release_workflow_import: forbidden。",
);
assertCondition(
  !/^release_gate_status:\s*(approved|ready|unblocked)\s*$/im.test(
    releaseApprovalChecklist,
  ),
  "公开发布人工审批 checklist 不得声明 approved、ready 或 unblocked 状态。",
);
assertCondition(
  !/^\s*-\s*\[[xX]\]\s+/m.test(releaseApprovalChecklist),
  "公开发布人工审批 checklist 当前不得出现已勾选审批项，避免误判为可正式发布。",
);
for (const requiredReleaseGate of [
  "Version strategy approved",
  "Windows signing verified",
  "macOS signing and notarization verified",
  "Linux packaging verified",
  "latest.json approval verified",
  "Build matrix verified",
  "Artifact manifest verified",
  "Human approval recorded",
]) {
  assertCondition(
    releaseApprovalChecklist.includes(requiredReleaseGate),
    `公开发布人工审批 checklist 缺少门禁项：${requiredReleaseGate}`,
  );
}
assertCondition(
  fileExists(
    "docs/open-source-migration/public-updater-latest-json-gate-2026-07-08.md",
  ),
  "公开 updater/latest.json 门禁文档必须存在，避免 latest.json 上传边界只停留在口头说明。",
);
for (const [field, expectedValue] of [
  ["Status", "BLOCKED"],
  ["latest_json_upload_allowed", "false"],
  ["release_artifact_upload_allowed", "false"],
  ["updater_manifest_generation_allowed", "false"],
  ["rollback_plan_status", "pending"],
  ["signature_key_match_status", "pending"],
]) {
  assertUniqueField(
    updaterLatestJsonGate,
    field,
    expectedValue,
    "公开 updater/latest.json 门禁",
  );
}
assertCondition(
  !/^\s*-\s*\[[xX]\]\s+/m.test(updaterLatestJsonGate),
  "公开 updater/latest.json 门禁当前不得出现已勾选证据项。",
);
for (const requiredUpdaterEvidence of [
  "latest.json source verified",
  "updater signature verified",
  "public endpoint verified",
  "rollback plan verified",
  "upload approval recorded",
]) {
  assertCondition(
    updaterLatestJsonGate.includes(`- [ ] ${requiredUpdaterEvidence}`),
    `公开 updater/latest.json 门禁文档缺少未勾选证据项：${requiredUpdaterEvidence}`,
  );
}
assertCondition(
  trackedReleaseArtifactPaths.length === 0,
  `公开仓当前不得跟踪 latest.json 或 release-assets 发布产物：${trackedReleaseArtifactPaths.join(", ")}`,
);

const flatpakCompatibilityChecks = [
  [
    "Flatpak manifest app id",
    /^id:\s*com\.ccswitch\.desktop\s*$/m.test(flatpakManifest),
    "Flatpak manifest 必须继续保留 app id com.ccswitch.desktop，避免破坏已安装用户和打包工具链。",
  ],
  [
    "Flatpak manifest command",
    /^command:\s*bianma-app\s*$/m.test(flatpakManifest),
    "Flatpak manifest 必须使用 Cargo 生成的 bianma-app 二进制启动。",
  ],
  [
    "Flatpak manifest deb source",
    /^\s*path:\s*cc-switch\.deb\s*$/m.test(flatpakManifest),
    "Flatpak manifest 必须继续读取 cc-switch.deb 中间产物，避免破坏现有 Linux 打包链路。",
  ],
  [
    "Flatpak manifest desktop source",
    /^\s*path:\s*com\.ccswitch\.desktop\.desktop\s*$/m.test(flatpakManifest),
    "Flatpak manifest 必须继续引用 com.ccswitch.desktop.desktop 源文件。",
  ],
  [
    "Flatpak manifest metainfo source",
    /^\s*path:\s*com\.ccswitch\.desktop\.metainfo\.xml\s*$/m.test(
      flatpakManifest,
    ),
    "Flatpak manifest 必须继续引用 com.ccswitch.desktop.metainfo.xml 源文件。",
  ],
  [
    "Flatpak manifest desktop install",
    /\/app\/share\/applications\/com\.ccswitch\.desktop\.desktop/.test(
      flatpakManifest,
    ),
    "Flatpak manifest 必须继续安装 com.ccswitch.desktop.desktop 桌面文件。",
  ],
  [
    "Flatpak manifest metainfo install",
    /\/app\/share\/metainfo\/com\.ccswitch\.desktop\.metainfo\.xml/.test(
      flatpakManifest,
    ),
    "Flatpak manifest 必须继续安装 com.ccswitch.desktop.metainfo.xml。",
  ],
  [
    "Flatpak manifest icon install",
    /\/app\/share\/icons\/hicolor\/128x128\/apps\/com\.ccswitch\.desktop\.png/.test(
      flatpakManifest,
    ),
    "Flatpak manifest 必须继续安装 com.ccswitch.desktop 图标资源。",
  ],
  [
    "Flatpak desktop Exec",
    /^Exec=bianma-app\s*$/m.test(flatpakDesktopEntry),
    "Flatpak desktop entry 必须使用 Cargo 生成的 bianma-app 二进制启动。",
  ],
  [
    "Flatpak desktop Icon",
    /^Icon=com\.ccswitch\.desktop\s*$/m.test(flatpakDesktopEntry),
    "Flatpak desktop entry 必须继续保留 Icon=com.ccswitch.desktop，避免破坏既有 icon 资源映射。",
  ],
  [
    "Flatpak desktop bianma scheme handler",
    flatpakDesktopMimeTypes.includes("x-scheme-handler/bianma"),
    "Flatpak desktop entry 必须注册 x-scheme-handler/bianma。",
  ],
  [
    "Flatpak desktop legacy scheme handler",
    flatpakDesktopMimeTypes.includes("x-scheme-handler/ccswitch"),
    "Flatpak desktop entry 必须注册 x-scheme-handler/ccswitch。",
  ],
  [
    "Flatpak metainfo id",
    /<id>com\.ccswitch\.desktop<\/id>/.test(flatpakMetainfo),
    "Flatpak metainfo 必须继续保留 com.ccswitch.desktop app id。",
  ],
  [
    "Flatpak metainfo launchable",
    /<launchable type="desktop-id">com\.ccswitch\.desktop\.desktop<\/launchable>/.test(
      flatpakMetainfo,
    ),
    "Flatpak metainfo 必须继续声明 com.ccswitch.desktop.desktop launchable。",
  ],
  [
    "Flatpak metainfo binary",
    /<binary>bianma-app<\/binary>/.test(flatpakMetainfo),
    "Flatpak metainfo 必须声明 bianma-app binary。",
  ],
  [
    "Flatpak README compatibility app id",
    /App ID:\s*`com\.ccswitch\.desktop`/.test(flatpakReadme),
    "Flatpak README 必须记录 com.ccswitch.desktop 是兼容标识。",
  ],
  [
    "Flatpak README compatibility desktop file",
    /Desktop file:\s*`com\.ccswitch\.desktop\.desktop`/.test(flatpakReadme),
    "Flatpak README 必须记录 com.ccswitch.desktop.desktop 是兼容标识。",
  ],
  [
    "Flatpak README compatibility binary",
    /Desktop Exec \/ binary:\s*`bianma-app`/.test(flatpakReadme),
    "Flatpak README 必须记录 bianma-app 是实际启动二进制。",
  ],
  [
    "Flatpak README compatibility deb",
    /Intermediate deb name:\s*`cc-switch\.deb`/.test(flatpakReadme),
    "Flatpak README 必须记录 cc-switch.deb 是兼容中间产物名。",
  ],
  [
    "Flatpak README compatibility bundle",
    /Exported bundle name:\s*`CC-Switch-Linux\.flatpak`/.test(flatpakReadme),
    "Flatpak README 必须记录 CC-Switch-Linux.flatpak 是历史导出包名。",
  ],
  [
    "Flatpak README compatibility schemes",
    /Deep-link schemes:\s*`bianma`,\s*`ccswitch`/.test(flatpakReadme),
    "Flatpak README 必须记录 bianma 和 ccswitch deep-link 兼容 scheme。",
  ],
  [
    "Flatpak README deb copy command",
    /flatpak\/cc-switch\.deb/.test(flatpakReadme),
    "Flatpak README 构建步骤必须继续复制到 flatpak/cc-switch.deb。",
  ],
  [
    "Flatpak README manifest command",
    /flatpak\/com\.ccswitch\.desktop\.yml/.test(flatpakReadme),
    "Flatpak README 必须继续使用 com.ccswitch.desktop.yml manifest。",
  ],
  [
    "Flatpak README bundle export command",
    /CC-Switch-Linux\.flatpak\s+com\.ccswitch\.desktop/.test(flatpakReadme),
    "Flatpak README 导出命令必须保留历史包名和 app id。",
  ],
  [
    "Flatpak README run command",
    /flatpak run com\.ccswitch\.desktop/.test(flatpakReadme),
    "Flatpak README 运行命令必须保留 com.ccswitch.desktop。",
  ],
];

for (const [, condition, message] of flatpakCompatibilityChecks) {
  assertCondition(condition, message);
}

if (failures.length > 0) {
  console.error("公开发布预检审计失败：");
  for (const failure of failures) {
    console.error(`- ${failure}`);
  }
  process.exit(1);
}

console.log(
  "公开发布预检审计通过：当前 workflow 仍是非发布占位，Flatpak legacy 兼容标识仍受保护，未引入私有签名或 release 上传链路。",
);
