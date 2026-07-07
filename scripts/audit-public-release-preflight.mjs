import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "..");

const readText = (relativePath) =>
  readFileSync(path.join(repoRoot, relativePath), "utf8");

const failures = [];

const assertCondition = (condition, message) => {
  if (!condition) {
    failures.push(message);
  }
};

const workflow = readText(".github/workflows/release.yml");
const tauriConfig = JSON.parse(readText("src-tauri/tauri.conf.json"));
const packageJson = JSON.parse(readText("package.json"));

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
assertCondition(
  !/\bcontents:\s*write\b/.test(workflow),
  "release workflow 当前不得申请 contents: write 发布权限。",
);
assertCondition(
  !/\bid-token:\s*write\b/.test(workflow),
  "release workflow 当前不得申请 id-token: write 发布权限。",
);

const forbiddenWorkflowPatterns = [
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
  assertCondition(
    !pattern.test(workflow),
    `release workflow 当前不得包含 ${label} 相关逻辑。`,
  );
}

const endpoints = tauriConfig?.plugins?.updater?.endpoints ?? [];
assertCondition(
  endpoints.includes(
    "https://github.com/CreatorEdition/bianma-app/releases/latest/download/latest.json",
  ),
  "Tauri updater endpoint 必须指向 CreatorEdition/bianma-app 的 latest.json。",
);
for (const endpoint of endpoints) {
  assertCondition(
    !/bianma-app-product|data\.bianma\.ai|internal|private/i.test(endpoint),
    `Tauri updater endpoint 不得指向 product、内部域名或私有通道：${endpoint}`,
  );
}
assertCondition(
  tauriConfig?.bundle?.createUpdaterArtifacts === true,
  "Tauri 配置应保留 createUpdaterArtifacts=true，正式发布时由人工门禁决定是否上传。",
);
assertCondition(
  tauriConfig?.plugins?.["deep-link"]?.desktop?.schemes?.includes("ccswitch"),
  "Deep link 必须继续保留 ccswitch legacy scheme，避免破坏迁移兼容。",
);
assertCondition(
  packageJson?.repository?.url?.includes("CreatorEdition/bianma-app"),
  "package.json repository 必须指向 CreatorEdition/bianma-app。",
);

if (failures.length > 0) {
  console.error("公开发布预检审计失败：");
  for (const failure of failures) {
    console.error(`- ${failure}`);
  }
  process.exit(1);
}

console.log(
  "公开发布预检审计通过：当前 workflow 仍是非发布占位，未引入私有签名或 release 上传链路。",
);
