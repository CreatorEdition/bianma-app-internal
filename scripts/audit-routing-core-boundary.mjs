import { readdirSync, readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const scriptDirectory = path.dirname(fileURLToPath(import.meta.url));
const repositoryRoot = path.resolve(scriptDirectory, "..");
const rustRoot = path.join(repositoryRoot, "src-tauri");
const failures = [];

const normalizePath = (value) => value.replaceAll("\\", "/");

const walkRustFiles = (directory) => {
  const files = [];
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    if (entry.name === "target") {
      continue;
    }
    const absolutePath = path.join(directory, entry.name);
    if (entry.isDirectory()) {
      files.push(...walkRustFiles(absolutePath));
    } else if (entry.isFile() && entry.name.endsWith(".rs")) {
      files.push(absolutePath);
    }
  }
  return files;
};

const readSection = (text, sectionName) => {
  const marker = `[${sectionName}]`;
  const start = text.indexOf(marker);
  if (start < 0) {
    return "";
  }
  const remainder = text.slice(start + marker.length);
  const nextSection = remainder.search(/^\s*\[[^\]]+\]\s*$/m);
  return nextSection < 0 ? remainder : remainder.slice(0, nextSection);
};

const assertCondition = (condition, message) => {
  if (!condition) {
    failures.push(message);
  }
};

for (const absolutePath of walkRustFiles(rustRoot)) {
  const relativePath = normalizePath(path.relative(repositoryRoot, absolutePath));
  const source = readFileSync(absolutePath, "utf8");
  const inIngressContract = relativePath.startsWith(
    "src-tauri/crates/ingress-contract/",
  );
  const isClassifier = relativePath.endsWith(
    "src-tauri/crates/routing-core/src/classifier.rs",
  );
  const isRoutingCoreTests = relativePath.endsWith(
    "src-tauri/crates/routing-core/src/tests.rs",
  );
  const isRoutingCoreRoot = relativePath.endsWith(
    "src-tauri/crates/routing-core/src/lib.rs",
  );
  const isAcceptedTypestateConsumer = [
    "src-tauri/crates/routing-core/src/classifier.rs",
    "src-tauri/crates/routing-core/src/disposition.rs",
    "src-tauri/crates/routing-core/src/normalizer.rs",
  ].includes(relativePath);

  if (/\bVerifiedIngressRequest\b/.test(source)) {
    assertCondition(
      inIngressContract || isClassifier || isRoutingCoreTests,
      `裸 VerifiedIngressRequest 越过 classifier 边界：${relativePath}`,
    );
  }
  if (/\bVerifiedIngressReceiver\b/.test(source)) {
    assertCondition(
      inIngressContract || isClassifier || isRoutingCoreRoot,
      `VerifiedIngressReceiver 未被 classifier 独占：${relativePath}`,
    );
  }
  if (/\bReceiverAcceptedIngressRequest\b/.test(source)) {
    assertCondition(
      inIngressContract || isAcceptedTypestateConsumer,
      `receiver 后置 typestate 越过 classifier/normalizer/disposition 封闭链：${relativePath}`,
    );
  }
  if (/\bRawIngressRequest\b/.test(source)) {
    assertCondition(
      inIngressContract || isClassifier || isRoutingCoreTests,
      `裸 RawIngressRequest 进入新的下游模块：${relativePath}`,
    );
  }
}

const rootManifest = readFileSync(
  path.join(repositoryRoot, "src-tauri", "Cargo.toml"),
  "utf8",
);
assertCondition(
  !/^\s*routing-core\s*=/m.test(readSection(rootManifest, "dependencies")),
  "根 App 当前切片不得依赖 routing-core；生产接线必须由后续切流 PR 单独完成",
);

const routingManifest = readFileSync(
  path.join(
    repositoryRoot,
    "src-tauri",
    "crates",
    "routing-core",
    "Cargo.toml",
  ),
  "utf8",
);
const ingressVerifiedSource = readFileSync(
  path.join(
    repositoryRoot,
    "src-tauri",
    "crates",
    "ingress-contract",
    "src",
    "verified",
    "mod.rs",
  ),
  "utf8",
);
const ingressRootSource = readFileSync(
  path.join(
    repositoryRoot,
    "src-tauri",
    "crates",
    "ingress-contract",
    "src",
    "lib.rs",
  ),
  "utf8",
);
const ingressContractSources = walkRustFiles(
  path.join(repositoryRoot, "src-tauri", "crates", "ingress-contract", "src"),
).filter((absolutePath) => !absolutePath.endsWith(`${path.sep}tests.rs`));
const routingRootSource = readFileSync(
  path.join(
    repositoryRoot,
    "src-tauri",
    "crates",
    "routing-core",
    "src",
    "lib.rs",
  ),
  "utf8",
);
assertCondition(
  /pub\(crate\)\s+fn\s+accept\s*\(/m.test(ingressVerifiedSource),
  "VerifiedIngressReceiver::accept 必须保持 crate-private，禁止外部宿主绕过 classifier",
);
assertCondition(
  /pub\(crate\)\s+struct\s+ReceiverAcceptedIngressRequest\b/m.test(
    ingressVerifiedSource,
  ),
  "receiver accepted typestate 必须保持 crate-private",
);
assertCondition(
  /pub\(crate\)\s+fn\s+body\s*\(&self\)\s*->\s*&\[u8\]/m.test(
    readFileSync(
      path.join(
        repositoryRoot,
        "src-tauri",
        "crates",
        "routing-core",
        "src",
        "normalizer.rs",
      ),
      "utf8",
    ),
  ),
  "NormalizerInput::body 必须保持 crate-private，公开 Normalizer 不得在 gate 前读取正文",
);
for (const absolutePath of ingressContractSources) {
  const relativePath = normalizePath(path.relative(repositoryRoot, absolutePath));
  const source = readFileSync(absolutePath, "utf8");
  const publicSignatures = source.match(/pub(?:\([^)]*\))?\s+fn\b[^\{;]{0,1200}/gs) ?? [];
  for (const signature of publicSignatures) {
    assertCondition(
      !(
        /\bVerifiedIngressReceiver\b/.test(signature) &&
        /\bVerifiedIngressRequest\b/.test(signature)
      ),
      `ingress-contract 不得新增公开 receiver/request 包装旁路：${relativePath}`,
    );
  }
}
assertCondition(
  !/pub\s+use[\s\S]{0,300}\bReceiverAcceptedIngressRequest\b/m.test(
    ingressRootSource,
  ),
  "ingress-contract 根模块不得重新导出 receiver accepted typestate",
);
assertCondition(
  /pub\s+use\s+ingress_contract\s*::/m.test(routingRootSource) &&
    !/^\s*mod\s+(classifier|disposition|normalizer|snapshot)\s*;/m.test(
      routingRootSource,
    ),
  "routing-core package 必须保持 facade；分类实现只能与 receiver 在 ingress-contract 同 crate 编译",
);
const ingressManifest = readFileSync(
  path.join(
    repositoryRoot,
    "src-tauri",
    "crates",
    "ingress-contract",
    "Cargo.toml",
  ),
  "utf8",
);
assertCondition(
  /^\s*publish\s*=\s*false\s*$/m.test(readSection(routingManifest, "package")),
  "routing-core 必须保持 publish=false，跨 crate accepted API 只允许在本仓边界门禁内使用",
);
assertCondition(
  /^\s*publish\s*=\s*false\s*$/m.test(readSection(ingressManifest, "package")),
  "ingress-contract 必须保持 publish=false，禁止把 receiver accepted API 发布为第三方扩展面",
);
const routingDependencies = readSection(routingManifest, "dependencies");
for (const forbiddenDependency of [
  "tauri",
  "tokio",
  "reqwest",
  "hyper",
  "axum",
  "rusqlite",
  "sqlx",
  "diesel",
]) {
  assertCondition(
    !new RegExp(`^\\s*${forbiddenDependency}\\s*=`, "m").test(
      routingDependencies,
    ),
    `routing-core 禁止引入运行时/网络/数据库依赖：${forbiddenDependency}`,
  );
}

if (failures.length > 0) {
  console.error("routing-core 边界审计失败：");
  for (const failure of failures) {
    console.error(`- ${failure}`);
  }
  process.exit(1);
}

console.log(
  "routing-core 边界审计通过：receiver/classifier typestate 未旁路，根 App 未接生产链，核心 crate 无运行时、网络或数据库依赖。",
);
