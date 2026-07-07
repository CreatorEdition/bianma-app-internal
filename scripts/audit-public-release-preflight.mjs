import { existsSync, readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "..");

const resolveRepoPath = (relativePath) => path.join(repoRoot, relativePath);

const fileExists = (relativePath) => existsSync(resolveRepoPath(relativePath));

const readText = (relativePath) =>
  readFileSync(resolveRepoPath(relativePath), "utf8");

const readTextIfExists = (relativePath) =>
  fileExists(relativePath) ? readText(relativePath) : "";

const failures = [];

const assertCondition = (condition, message) => {
  if (!condition) {
    failures.push(message);
  }
};

const workflow = readText(".github/workflows/release.yml");
const tauriConfig = JSON.parse(readText("src-tauri/tauri.conf.json"));
const packageJson = JSON.parse(readText("package.json"));
const cargoToml = readText("src-tauri/Cargo.toml");
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
  deepLinkSchemes.includes("bianma"),
  "Deep link 必须继续保留 bianma 当前公开 scheme。",
);
assertCondition(
  deepLinkSchemes.includes("ccswitch"),
  "Deep link 必须继续保留 ccswitch legacy scheme，避免破坏迁移兼容。",
);
assertCondition(
  packageJson?.repository?.url?.includes("CreatorEdition/bianma-app"),
  "package.json repository 必须指向 CreatorEdition/bianma-app。",
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

const flatpakCompatibilityChecks = [
  [
    "Flatpak manifest app id",
    /^id:\s*com\.ccswitch\.desktop\s*$/m.test(flatpakManifest),
    "Flatpak manifest 必须继续保留 app id com.ccswitch.desktop，避免破坏已安装用户和打包工具链。",
  ],
  [
    "Flatpak manifest command",
    /^command:\s*cc-switch\s*$/m.test(flatpakManifest),
    "Flatpak manifest 必须继续使用 command: cc-switch，避免破坏现有 Flatpak 启动入口。",
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
    /^Exec=cc-switch\s*$/m.test(flatpakDesktopEntry),
    "Flatpak desktop entry 必须继续保留 Exec=cc-switch，避免破坏现有二进制启动入口。",
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
    /<binary>cc-switch<\/binary>/.test(flatpakMetainfo),
    "Flatpak metainfo 必须继续声明 cc-switch binary。",
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
    /Desktop Exec \/ binary:\s*`cc-switch`/.test(flatpakReadme),
    "Flatpak README 必须记录 cc-switch 是兼容启动入口。",
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
