# bianma-app 单仓开源主线说明

## 当前定位

- `bianma-app` 当前是 `Bianma` 唯一正式 App 主仓。
- 默认范围包括：产品源码、README、用户手册、公开中文文案、开源协作说明、release、updater 与二进制分发。
- `bianma-app-product` 降级为历史迁移源与待归档目录，不再作为新任务或新发布入口。

## 当前状态

- 🔄 进行中（2026-08-10）：启动 C1-AC「账户、凭据与额度组统一静态布局」。将现有仅额度组的激活入口收敛为唯一 `SelectionRuntimeDefinitions`，一次性精确覆盖可达 Account、Credential 与 `QuotaGroupId` 的正整数并发上限。布局将只为同一快照的 Target/Selector/Unit/Member 生成同时绑定三类资源的只读 binding；缺失、多余、重复、错 Unit/Member 或跨快照一律 fail closed。C1-AC 不产生选中身份、Lease、Registry、inflight 写入、策略、Health/429、Attempt/Replay、Secret 或网络。
- ✅ 已完成（2026-08-10）：完成 C1-L「全局额度组静态布局」。为未来跨 Selector 共享额度的原子 Lease 建立同代快照绑定、唯一 `QuotaGroupId` 与正整数并发上限的编译期合同。`MAX_TRACKED_QUOTA_GROUPS` 固定为 256，按所有可达 Selector 的去重 Group 计数；超限、缺失/多余/重复定义均拒绝配置激活，禁止 per-selector 溢出、静默降级或将共享额度拆入多个 Registry。布局查询还必须同时绑定同一快照、目标 Selector、显式 Unit 与其 Group；同版本但不同快照、跨 Selector/Unit Group 均 fail closed。该值是面向本地低 CPU/固定内存的明确产品边界，不改变当前 16 个 Target/Account/Credential 上限。73 个 `routing-core` 单测、全工作区 fmt、Clippy、严格 rustdoc、零正常依赖、diff 检查与 release 微基准（100,000 次约 9.02ms，约 90ns/plan，本机负载波动）通过，独立 Terra 实际 diff 审计无剩余 P0/P1。C1-L 只验证和索引静态布局，不实现选择、Lease、inflight 运行时写入、Health/429、Attempt/Replay、Secret、网络或旧 proxy。
- ✅ 已完成（2026-08-10）：新增 C0「账户选择请求与动态 eligibility 合同」，为未来同站多账户/多 Credential 的安全实际选择固定输入边界。Selector 现携带不可变 revision 与 affinity salt；Session 只接受宿主已经 HMAC 化的 16 字节别名，核心不接收原始 Session Key 或实现 HMAC。`SessionAffinityAlias` 不提供 Debug、Display、Hash 或字节 getter，避免外部 Hasher、日志或普通读取路径观察别名材料。`CompiledRoutingSnapshot` 仅可为同一 `RoutingSnapshot` 实例解析的目标建立选择请求；动态 Eligibility 以 16 位 Unit/Member 位图保守表达，越界位、未知 Unit、Member 未获其 Unit 允许、空 Unit 或跨快照均 fail closed。同一 QuotaSelectionUnit 混用 priority tier 在编译期拒绝，保证未来 Unit-first Priority 语义唯一。73 个 `routing-core` 单测（覆盖 16 Unit/16 Member 的 `u16::MAX` 满位资格边界）、全工作区 fmt、Clippy、严格 rustdoc、零正常依赖、diff 检查通过，独立 Terra 实际 diff 审计无剩余 P0/P1。本切片不执行选择、不创建 Lease、不更新 Health/429、不修改 Attempt/Replay、不读取 Secret/网络/时钟或旧 proxy。
- ✅ 已完成（2026-08-10）：新增仅 crate 内可见、零分配的 `AccountSelectionCandidates` 候选视图，作为同站多账户/多 Credential/共享额度拓扑从编译快照走向未来执行选择的安全桥接层。视图仅由 `CompiledRoutingSnapshot` 接收同代 `ResolvedRouteTarget` 后创建，并按 `RoutingSnapshot` 实例身份拒绝同版本、同 TargetId 但 Site/Deployment 不同的跨快照目标；只枚举已编译 `QuotaSelectionUnit` 和指定 Unit 成员，未知 Unit 为空且不回退，不实现任何 Priority、RoundRobinCompat、WeightedLeastInflight 执行、Session 粘性、Lease、冷却、重试或 Secret 使用。回归覆盖目标/策略/拓扑、同账户双 Credential、同站双 Account、共享 Unit 权重、保守单 Unit、未知 Unit 与跨快照拒绝；现有普通及写后 429 不重放回归保持。`routing-core` 共 65 个单测、fmt、Clippy、严格 rustdoc、零正常依赖、diff 检查与 release 微基准（100,000 次约 10.10ms，约 101ns/plan，本机负载波动）均通过。该切片不改变 RoutePlan、Coordinator、Health/429 或实际账户/凭据选择。
- ✅ 已完成（2026-08-10）：将 `TrustedPreExecutionRejection` 收紧为单次 `AdapterReplayReporter` 消费式签发的收据；收据绑定 CoordinatorId、AttemptId、完整 Target 与同一 RoutingSnapshot 实例，失配按 Unknown fail closed。`VerifiedPreExecutionContract` 包含 Site、稳定错误码、adapter 版本、合同修订版与证据种类；真实 adapter registry/verifier 尚未实现，正常构建没有该合同的构造路径，只有 `cfg(test)` 固定 helper 可生成测试合同。普通 HTTP 状态、`Retry-After`、Header、JSON 自由字段、错误文本、未知错误码与通用 adapter 均不得产生回放凭据；限流观测链与 receipt 仍完全分离。已写字节、SSE 或下游提交优先禁止重放，摘要仅 crate-private 保留且不进入 Debug。完整运行时代码统计为 369/380 行，Tracker ≤32B、Outcome ≤32B、Completion ≤64B；62 个 `routing-core` 单测、fmt、Clippy、严格 rustdoc、零正常依赖与 diff 检查通过。本切片不接入 HTTP 解析、真实 Transport、adapter registry、账户/额度选择、异步、持久化或旧 `src-tauri/src/proxy/**`。
- ✅ 已完成（2026-08-10）：收紧 HealthRegistry 冷却写入口为一次性 `AttemptTracker -> AdapterRateLimitReporter -> TrustedRateLimitObservation -> HealthRegistry` 合同。Reporter 工厂只在 Attempt 模块私有实现，单个 Tracker 只签发一次；Permit 与 ResolvedRouteTarget 都借用同一 RoutingSnapshot，并按实例地址拒绝同版本、同 TargetId 但 Site/Deployment 不同的跨快照归因。该链只写 Site/ModelDeployment/Unknown（Unknown 按 Site）冷却，不解析 HTTP/Retry-After，不改变 DeliveryState/ReplayEvidence，也不得生成重放许可；不实现真实 adapter/Transport、Account/Credential/Quota 冷却、异步或持久化。59 个 `routing-core` 单测、fmt、Clippy、严格 rustdoc、零正常依赖、diff 检查与 release 微基准通过；独立 Terra 复审无剩余 BLOCKING。
- ✅ 已完成（2026-08-10）：新增单所有者、固定容量的 `HealthRegistry` 与借用同一 `RoutingSnapshot` 实例的 `RouteEligibility` / `RoutePlan`，仅记录 Site/ModelDeployment 冷却；Planner 与 Coordinator 均强制消费 eligibility，冷却目标被跳过但不改写 RoutePlan。即使 `SnapshotVersion` 相同、TargetId 相同而 Site/Deployment 不同，跨快照 eligibility 与计划也会按实例身份 fail closed。普通或写后 429 仍不得产生重放许可；不解析 HTTP/Retry-After，不实现 Account/Credential/Quota 冷却、Transport、网络、异步或持久化。58 个 `routing-core` 单测、fmt、Clippy、严格 rustdoc、零正常依赖、diff 检查与 release 微基准均通过，独立 Terra 复审无剩余 BLOCKING。
- ✅ 已完成（2026-08-10）：新增借用式、最多 16 项的静态 `Account` / `Credential` catalog，并由零分配的 `AccountCredentialDefinitions` 成对传入 `CompiledRoutingSnapshot::compile`。编译顺序保持为 RoutingSnapshot 形状、Target/Deployment 三元组、selector 引用、Account/Credential 目录、selector member 静态关系；未知 Account、未知 Credential、Credential owner 不一致、Account 与 deployment 跨 Site 均 fail closed。`CompiledRoutingSnapshot` 原子持有静态目录，但 `ResolvedRouteTarget` 不伪造实际账户或 Key 选择，公开 Debug 与 crate 外接口不暴露 Definition。Planner、RoutePlan、Coordinator 与请求热循环不查询新目录；新增目录构造、边界、同站多 Account、同 Account 多 Credential、共享 QuotaUnit 不加权回归。`routing-core` 48 个单测、fmt、Clippy、rustdoc、零正常依赖、diff 检查和 release 微基准（100,000 次约 7.21ms，约 72ns/plan，本机负载波动）均通过；独立 Terra 复审无 BLOCKING。本切片不代表 Grant/Origin/AuthScheme/Secret 授权，不实现实际选择、Health/429、重试、Transport、网络、数据库、异步、ContextPipeline 或旧 `src-tauri/src/proxy/**`。
- ✅ 已完成（2026-08-10）：新增借用式、最多 16 项的静态 `ModelDeployment` catalog，且 `CompiledRoutingSnapshot` 已原子持有它并在 RoutingSnapshot 形状验证后按固定顺序校验 RouteTarget 的 Site/Endpoint/Deployment 三元组：未知 deployment、Site 不一致、Endpoint 不一致均 fail closed；selector 引用校验保持在其后。单次计划解析也只从同代 deployment/selector catalog 取受控静态定义，理论不变量破坏返回明确错误。公开 `ResolvedRouteTarget` 只显示版本、Stage、Target 与 selector ID，selector/deployment Definition getter 与 Debug 输出均不对 crate 外暴露。Planner、RoutePlan 与 Coordinator 的字段、签名和热循环不变。新增回归后 `routing-core` 43 个单测、fmt、Clippy、rustdoc、零正常依赖与 diff 检查均通过；release 微基准为 100,000 次约 11.37ms（约 114ns/plan，本机负载波动）。独立 Terra 复审无剩余 BLOCKING。本切片不替代 Grant/Origin/认证校验，也不实现模型能力、账户选择、Health/429、Vault、Transport、网络、数据库、异步、ContextPipeline 或旧 `src-tauri/src/proxy/**`。
- ✅ 已完成（2026-08-10）：`CompiledRoutingSnapshot` 已新增 `resolve_plan_target`，是计划单次尝试绑定同代 `RouteTarget + AccountSelector` 的唯一公开入口。它先复用 RoutePlan 的版本/Target 校验，再在同一编译代中解析 Stage 与 selector，错代计划 fail closed，越界尝试返回 `None`；公开解析结果只暴露静态版本、Stage、Target 与 selector ID，原始 selector Definition 仅供 crate 内受控执行层消费，不选择 Account/Credential、不携带 Secret、Health、额度、租约、传输或请求内容。catalog 的裸 ID 查询已收紧为 crate 内，Planner、RoutePlan 与 Coordinator 的字段、签名和热循环不变。新增回归后 `routing-core` 41 个单测、fmt、Clippy、rustdoc、零正常依赖与 diff 检查均通过；release 微基准为 100,000 次约 8.93ms（约 89ns/plan，本机负载波动）。独立 Terra 复审确认公开面不再暴露裸 selector Definition。本切片仍不实现账户选择、Health/429、Vault、Transport、网络、数据库、异步、ContextPipeline 或旧 `src-tauri/src/proxy/**`。
- ✅ 已完成（2026-08-10）：编译路由快照与 `AccountSelector` catalog 已同代绑定。`CompiledRoutingSnapshot` 原子持有候选快照与借用式、最多 16 项的 catalog，Target 引用不存在的 selector 会在编译期 fail closed；`RoutingSnapshot::new` 已收紧为 crate 内入口，外部只能使用编译快照。Planner、RoutePlan 与 Coordinator 不保存或查询 catalog，热路径签名不变。新增悬空引用、共享 selector 的 Stage-first、快照间 catalog 不混用回归后，`routing-core` 40 个单测、fmt、Clippy、rustdoc、零正常依赖与 diff 检查均通过；release 微基准为 100,000 次约 10.98ms（约 110ns/plan，本机负载波动）。独立 Terra 复审无剩余 BLOCKING/NON-BLOCKING。本切片仍不实现账户选择、Health/429、Vault、Transport、网络、数据库、异步、ContextPipeline 或旧 `src-tauri/src/proxy/**`。
- ✅ 已完成（2026-08-10）：新增纯数据 `AccountSelectorDefinition` 与 `QuotaSelectionUnit` 合同，固定成员/单元/额度组上限、验证成员到额度单元的引用，并保守表达未知额度拓扑。权重只属于额度单元，禁止由多 Key 成员数叠加；未知拓扑只能单一 Unit，用户确认或受信 Adapter 标记才可拆分，且同一 QuotaGroup 在单个 Selector 中不得出现在多个 Unit，避免共享额度伪装为独立额度并触发 429 轮换风暴。Selector 只借用静态配置、无堆分配，新增验证后 `routing-core` 共 36 个单测，并通过 fmt、Clippy、rustdoc、零正常依赖、diff 检查；release 微基准为 100,000 次约 6.79ms（约 68ns/plan）。本切片不把 catalog 临时接入 Planner、不实现账户选择、Session、Health、429 冷却、Lease、Credential/Vault、Transport、数据库、异步或旧 `src-tauri/src/proxy/**`。
- ✅ 已完成（2026-08-10）：将 RouteTarget 与具体 Account/Credential 解耦，改为只绑定 `AccountSelectorId`；并在 RoutingSnapshot 构建时拒绝同一已编译计划重复引用同一 ModelDeployment。不同 RoutePolicy 可合法复用同一 Deployment，Target 只携带完整 Target 身份与账户选择合同引用，不假装已选定账户/凭据。`routing-core` 维持 31 个单测、RoutePlan ≤160B，通过 fmt、Clippy、rustdoc、零正常依赖、diff 检查；release 微基准为 100,000 次约 7.55ms（约 76ns/plan）。此切片不实现 selector、QuotaGroup、健康/429 冷却、Credential/Vault、Transport、数据库、异步或旧 `src-tauri/src/proxy/**`。
- ✅ 已完成（2026-08-10）：将 Stage-first `RoutePlanner` 的输入收紧为已验证的 Routed disposition 与同版本 RoutingSnapshot，关闭任意调用方绕过闭集 ingress 分类、直接拿快照规划的旁路。Planner 只接受字段私有、仅由 IngressClassifier 产生的 `VerifiedRouteDispatch`，并在热路径首步 fail closed 拒绝版本错配；此类型仍只证明闭集 RouteSpec/形状通过，不宣称完成入站鉴权、Attestation 或 Secret 授权。新增错配回归后 `routing-core` 共 31 个单测，并通过 fmt、Clippy、rustdoc、零正常依赖、diff 检查；release 微基准为 100,000 次约 7.49ms（约 75ns/plan）。不接入 Context gate、BoundDeployment、账户选择、Health/Quota、Transport、数据库、异步或旧 `src-tauri/src/proxy/**`。
- ✅ 已完成（2026-08-10）：实现 Stage-first `RoutePlan` 合同，修复全局 RoundRobin/LeastPenalty 可能让 B 抢在 A 前、或将 `A:[A.1 ∥ A.2]` 错当串行全局池的问题。快照候选已绑定稳定 `RouteStageId` 并拒绝非连续 Stage；均衡只在 Stage 内发生，且每个可用 Stage 只签发一个目标。快照拒绝小于逻辑 Stage 数的 `max_attempts`，Coordinator 也拒绝小于 RoutePlan 长度的 RetryPolicy，避免 B 被静默截断而永久不可达；RoutePlan 可解析每次尝试的 Stage ID，且计划体仍固定 16 项、无堆分配。新增回归后 `routing-core` 共 30 个单测，并通过 fmt、Clippy、rustdoc、零正常依赖、diff 检查；release 微基准为 100,000 次约 7.12ms（约 71ns/plan）。不接入 Health/Quota/AccountSelector、Credential/Vault、Transport、数据库、异步或旧 `src-tauri/src/proxy/**`。
- ✅ 已完成（2026-08-10）：基于 `d76e1c8` 实现纯 Rust `AttemptCoordinator` 的线性消费合同。Coordinator 消费不可变 `RoutePlan`，只签发不可复制且无公开构造器的单次 Attempt 许可；许可必须按所有权链 `Permit → Tracker → 私有 Completion → Coordinator` 被消费，完成一次 Attempt 才能原子地产生唯一的下一许可或永久停止。Completion 同时绑定单调 Attempt ID 与进程内不复用的 Coordinator ID，错误 Coordinator 接收时 fail closed；写后、首个 SSE 语义事件、下游提交和普通 429 均不会签发下一许可。新增 Coordinator 级回归覆盖线性推进、跨 Coordinator 拒绝、写后 429、SSE 与下游提交停止；`routing-core` 共 27 个单测，并通过 fmt、Clippy、rustdoc、零正常依赖与 diff 检查。本切片不接入 Transport、Adapter、Health/Cooldown、Credential/Quota、DAG、数据库、异步或旧 `src-tauri/src/proxy/**`。
- ✅ 已完成（2026-08-10）：在 `8a3c983` 的最小闭集核心上实现独立的单次发送、SSE 与重放安全合同。固定大小且不可复制的 crate-private `AttemptTracker` 只允许状态单调前进；公开 `AttemptOutcome` 没有公开构造路径，只有受控观察或 crate 内受信证明可生成。任意上游字节写入、首个 SSE 语义事件、任意阶段下游提交与普通 HTTP 429/Retry-After 均禁止自动重放；受信执行前拒绝也仅在零字节状态下允许。新增 6 个回归测试，`routing-core` 共 23 个单测，并通过 fmt、Clippy、rustdoc、零正常依赖和 diff 检查。本切片不接入旧 `src-tauri/src/proxy/**`，不实现网络 I/O，也不引入第三方依赖。
- ✅ 已完成（2026-08-09）：实现 `routing-core` 的最小闭集入站规格与默认拒绝分类器；仅允许内置 OperationId 产生 Local、BoundDeployment 或 Routed 结果，不引入外部 Normalizer。`LocalContextCompact` 只分发给本地 handler，ContextPipeline 的压缩、记忆和图实现不进入模型 RoutePlan 或 routing-core。
- ✅ 已完成（2026-08-09）：完成最小 `routing-core` 领域核心切片。保留 Tauri/React 外壳，独立使用零第三方依赖的纯 Rust crate，不修改旧 `src-tauri/src/proxy/**`；实现固定上限 16 个候选、Priority/RoundRobin/LeastPenalty 计划、快照版本绑定与保守 ReplayGate。仅允许传输层 `NotSent` 或受信适配器证明的 `PreExecutionRejected` 推进；普通 HTTP 429 不自动重放。17 个单测、Clippy、rustdoc 通过；RoutePlan ≤160 bytes，规划器运行时代码 ≤520 行，入站分类器运行时代码 ≤280 行；16 候选 release 微基准 100,000 次多次复跑约 105–171 ns/plan（本机负载波动基线）。本切片尚未接入生产转发链，不承载 ContextPipeline。
- ✅ 已完成（2026-08-07）：将 RTK、Context Mode、Graphify、memsearch、lean-ctx 与 tokbench 的固定源码审计纳入 `routing-core v2` 规格；完成独立 `ContextPipeline`、整体密封的授权 bundle/anti-signing-oracle、Local/BoundDeployment/Routed 闭集、GatewayOnly/N0/Shadow/Managed Execute 门禁、精确账户 Capability 探测、最终出站冻结复核、单一有损变换所有权、本地归档/图/记忆 Sidecar、子代理旁路防护和版本绑定 A/B 门禁。冻结架构文档 SHA-256 为 `44CDFE8812DBA6CC1EE15122885CAE1844D424D0306ADC0F8828F09C8B692926`；Prettier、TypeScript、本地链接、围栏、1–26 不变量、token-like 扫描、`git diff --check` 与 4/4 Mermaid 11.12.0 真实解析/渲染均通过。Vitest 全量运行 66/67 suites 通过，唯一 App integration hook 首轮 20 秒超时后以 60 秒 hook timeout 定向复跑 5/5 通过。本轮仍只修改文档，不声明生产功能已实现。
- ✅ 已完成（2026-08-06）：根据格式化后二轮独立审计收紧 `routing-core v2` 架构规格；已关闭 Credential 跨站/跨 Origin 释放、最终 URL 先验证、Session 账户粘性与受控打破、Vault 激活闩锁、一次性 SecretSubmissionPermit、认证输入校验、WebDAV 冲突标识、账户/额度双语义和 JWT NumericDate 合同。后续两份独立复核未发现新增 P0；其指出的核心 P0 残余合同与 P1/P2 已逐项修订并通过定向合同检查；本任务仍只修改架构和任务文档，不修改 Rust/TypeScript 生产逻辑。
- ✅ 已完成（2026-07-08）：补齐公开发布人工审批 checklist 门禁；新增 `docs/open-source-migration/public-release-approval-checklist-2026-07-08.md`，预检脚本强制 checklist 保持 `Status: BLOCKED`、审批项未勾选，并覆盖版本策略、签名/notarization、`latest.json`、构建矩阵、artifact manifest 和人工审批记录。
- ✅ 已完成（2026-07-07）：补齐 product 迁移 denylist 自动化门禁；新增 `scripts/audit-product-migration-guard.mjs` 与 `pnpm audit:product-migration`，将 `.teamwork/**`、`docs/internal-spec/**`、私有发布链路、providerRuleCenter、Session Cloud、Risk/Local Policy/Keyword Guard、多 key 池、合作方材料和 token-like 示例纳入 staged diff 硬阻断。
- ✅ 已完成（2026-07-07）：补齐公开发布版本占位门禁；预检脚本已强制 `package.json`、`src-tauri/Cargo.toml` 与 `src-tauri/tauri.conf.json` 版本一致，并在正式版本策略获批前继续保持 `0.0.1` 占位版本，避免误发正式 release。
- ✅ 已完成（2026-07-07）：补齐 Flatpak/Linux legacy 兼容门禁；公开发布预检脚本已保护 `com.ccswitch.desktop`、`com.ccswitch.desktop.desktop`、`Exec=cc-switch`、`cc-switch.deb`、`CC-Switch-Linux.flatpak`、`bianma`/`ccswitch` deep-link scheme 与 Flatpak desktop `MimeType` 注册，确认这些旧标识属于兼容边界而非待清理品牌残留。
- ✅ 已完成（2026-07-07）：完成公开 URI 协议文档对齐切片；仅吸收 product 公开文档中的确认流安全边界，明确公开仓当前只承诺 `v1/import`，`v2/providers/import` 与 `v2/subscriptions/import` 仍未公开支持，继续排除 `docs/internal-spec/**` 私有协议、subscription v2、token-like 示例和 `data.bianma.ai` 场景。
- ✅ 已完成（2026-07-07）：核验 Gemini OAuth client secret 公开例外；通过 `gh search code` 与上游 `google-gemini/gemini-cli` 源码回读确认该值属于 installed application OAuth client，补充 `subscription.rs` 注释与敏感材料审计文档，保留现有刷新逻辑。
- ✅ 已完成（2026-07-07）：完成合入前敏感/私有材料复查切片；新增 `docs/open-source-migration/private-material-audit-2026-07-07.md`，确认 product 的 `.teamwork`、`docs/internal-spec`、私有发布链路、远端规则中心、Session Cloud、Risk/Local Policy/Keyword Guard、多 key 池、合作方材料和 token-like 示例不得直接迁入，并清理公开仓 Rust 单测中的 `sk-*` / `AIza*` 形态占位。
- ✅ 已完成（2026-07-07）：补齐公开发布门禁预检切片；新增 `docs/open-source-migration/public-release-gate-runbook-2026-07-07.md` 与 `scripts/audit-public-release-preflight.mjs`，确认当前 release workflow 仍是非发布占位，静态阻断 product 私有签名/notarization/latest.json 上传链路，并保留 `ccswitch` legacy scheme。
- ✅ 已完成（2026-07-07）：完成 `bianma-app-product` 差异与发布风险集中审计批次；新增 `docs/open-source-migration/product-diff-and-release-audit-2026-07-07.md`，记录 product-only 202 个路径、公开仓 app-only 39 个路径、不可直接迁移区域、发布 workflow 风险和后续白名单切片，继续禁止整仓迁移 product。
- ✅ 已完成（2026-07-07）：收口合作方促销运行时展示最小切片；仅停止 API Key 区域渲染 `partnerPromotion` 促销文案并移除预设选择器 `isPartner` 星标徽标，补充定向组件测试；保留普通 API Key 获取链接、预设源数据、ProviderForm 提交/元数据路径、OAuth 判断、历史 release notes 与外链参数。
- ✅ 已完成（2026-07-07）：收口公开 demo、用户文档与配置占位密钥形态；将 deplink.html、URI 协议文档、三语用户手册、Gemini 通用配置与三语 Codex auth 示例中的 sk-/AIzaSy 形态占位替换为非密钥形态 placeholder，不迁入合作方促销、签名发布或私有 URL。
- ✅ 已完成（2026-07-07）：补齐 DirectorySettings 公开基础测试；仅覆盖应用目录与 Claude/Codex/Gemini/OpenCode/OpenClaw 目录输入、变更、浏览和重置回调，不迁入 product 风险审批、SessionCloud 或目录写入 fan-out 私有链路。
- ✅ 已完成（2026-07-07）：补齐 RequestDetailPanel 基础详情测试；仅覆盖公开仓已有 provider/model、tokens、cost、latency、错误和未找到状态，不迁入 Local Policy、Keyword Guard 或 rule facts。
- ✅ 已完成（2026-07-07）：收口 import_export_sync SQL 备份测试命名；仅替换测试临时文件名与测试名，不修改 `.cc-switch` 兼容目录或 SQL 导入兼容逻辑。
- ✅ 已完成（2026-07-07）：补齐 ProviderWorkspacePanel 选择回退与范围过滤测试；仅覆盖公开仓已有交互，不迁入规则中心、多 key、策略后端或合作方能力。
- ✅ 已完成（2026-07-07）：收口 WebDAV 远端快照预览测试契约；仅补齐公开仓已有 RemoteSnapshotInfo 字段与确认弹窗路径断言，不迁入 SessionCloud 或私有同步能力。
- ✅ 已完成（2026-07-07）：收口 useImportExport 导出文件名断言与预期错误日志测试噪声；仅迁移公开仓已有导入导出 hook 的测试增强，不改业务逻辑。
- ✅ 已完成（2026-07-07）：收口 McpFormModal 测试 JsonEditor double 最小切片；测试 mock 不再把 `darkMode` / `showValidation` 等非 DOM props 透传到 textarea，仅降低测试噪声，不修改 MCP 表单业务逻辑。
- ✅ 已完成（2026-07-07）：迁移 UniversalProviderPanel 同步状态与批量同步公开切片；仅覆盖最近同步状态、错误摘要、选择清理、批量同步与定向测试，不迁入 partner/affiliate、apiKeyPool、providerRuleCenter、SessionCloud、Risk Guard、Local Policy、订阅 quota、签名发布相关内容。
- ✅ 已完成（2026-07-07）：收口公开迁移安全/发布口径最小切片；替换 `deplink.html` 中 token-like Context7 示例值，修正三语 FAQ 中提前声明 macOS 已签名/公证的发布口径，并移除 partner promotion 旧品牌文案固化测试；未改动 provider preset 合作方结构、updater 配置或发布工作流。
- ✅ 已完成（2026-07-07）：补齐通用配置片段 legacy localStorage 迁移测试最小切片；仅覆盖公开仓 Claude/Codex/Gemini common config snippet 迁移行为，不迁入 product 私有能力。
- ✅ 已完成（2026-07-07）：补齐 useSetAutoFailoverEnabled 翻译 toast 定向测试最小切片；仅覆盖公开仓故障转移开关 mutation 成功/失败提示，不迁入 product 私有能力。
- ✅ 已完成（2026-07-07）：抽取 Provider Workspace 动作 Hook 最小切片；仅迁移公开仓 App 内已有外链打开、确认删除/移除、复制、终端打开与导入后刷新动作，不迁入 product 私有能力。
- ✅ 已完成（2026-07-06）：收口三语可见 UI 旧品牌文案最小切片；仅替换非合作方/非兼容路径/非历史说明的 `CC Switch` 可见文案，不迁移合作方推广、兼容路径、历史 release notes 或 product 私有能力。
- ✅ 已完成（2026-07-06）：补齐 Settings 前端契约最小切片；仅覆盖公开仓已存在的 OpenCode/OpenClaw 配置目录字段与本地当前供应商字段类型/schema，并补充定向 schema 测试，不迁入 product 私有能力。
- ✅ 已完成（2026-07-05）：实现 WebDAV 自动同步 scope v1 最小切片；仅允许 Providers/MCP/Prompts 作为自动同步触发范围，自动上传与手动上传/下载均继续保持完整快照，不迁入 SessionCloud、providerRuleRegistry/providerRuleCenter、data.bianma.ai、apiKeyPool、Risk Guard、Local Policy、strategy/load-balancing/failover backend、partner/affiliate/referral/sponsor、subscription/quota 或签名发布材料。
- ✅ 已完成（2026-07-05）：实现 OpenClaw 配置目录 UI wiring 最小切片；仅补齐 Settings 高级目录页、目录 hook、表单 sanitize、保存 payload 与定向测试，默认目录为用户 home 下 `.openclaw`，不迁入 providerRuleRegistry/providerRuleCenter、data.bianma.ai、apiKeyPool、SessionCloud、Risk Guard、Local Policy、strategy/load-balancing/failover backend、partner/affiliate/referral/sponsor、subscription/quota、Pricing/Auth 下线/Settings IA 重组或任何 Risk Guard UI。
- ✅ 已完成（2026-07-05）：实现 Settings 加载失败可读提示最小切片；仅处理 get_settings / Tauri invoke 不可用的可读错误、useSettings 查询错误透出与 SettingsPage 首次失败重试 UI，不迁入 providerRuleRegistry/providerRuleCenter、data.bianma.ai、apiKeyPool、SessionCloud、Risk Guard、Local Policy、strategy/load-balancing/failover backend、partner/affiliate/referral/sponsor、subscription/quota、Pricing/Auth 下线/Settings IA 重组或 OpenClaw 目录字段。
- ✅ 已完成（2026-07-05）：收口默认 App 配置目录解析最小切片；优先从 Tauri `get_app_config_path` 返回的 `config.json` 路径派生目录，失败时保留 `.cc-switch` 兼容 fallback，不迁移 product 的 OpenClaw 目录 UI。
- ✅ 已完成（2026-07-05）：收口 Rust 运行时品牌常量最小切片；新增公开仓 brand 常量并接入开机自启名称、GitHub User-Agent、release latest URL 与 deep link scheme 判断，未迁移 product release URL、配置目录切换、规则中心、会话云、风险守卫、策略链、合作方推广或签名发布材料。
- ✅ 已完成（2026-07-05）：抽取本地会话详情卡最小切片；仅将公开仓现有右侧会话详情拆为 SessionDetailCard 并补充组件测试，不迁移 Session Cloud、列表改版、排序模式或远端同步能力。
- ✅ 已完成（2026-07-05）：抽取公开仓 App 视图保护 Hook；仅迁移可见应用兜底与会话视图保护逻辑，不迁移 product 新导航模型、规则中心、多 key 池、会话云、风险守卫、策略链、合作方推广或签名发布材料。
- ✅ 已完成（2026-07-05）：抽取公开仓 App 纯 UI 状态 Hook；仅迁移 settings 默认 tab、Add/Edit/Usage 弹窗、delete/remove confirm 与关闭回调，不迁移 product 新导航模型或规则中心、多 key 池、会话云、风险守卫、策略链、合作方推广和签名发布材料。
- ✅ 已完成（2026-07-05）：迁移剪贴板工具增强与测试最小切片；仅迁移剪贴板纯工具增强，不迁移规则中心、多 key 池、会话云、风险守卫、策略链、合作方推广或签名发布材料。
- ✅ 已完成（2026-07-05）：移除 useSettings 手动保存目录覆盖变化后的额外 live 同步副作用；仅收口公开仓现有 settings 保存、App 配置目录覆盖与托盘刷新行为，不迁移 product 的额外目录、规则中心或供应商策略能力。
- ✅ 已完成（2026-07-05）：迁移 useSessionListState 最小切片；仅抽取会话页当前搜索、Provider 过滤、选中 key 与 selectedSession 派生逻辑，不迁移 product 的排序模式、SessionListCard 或 Session Cloud。
- ✅ 已完成（2026-07-05）：迁移 useSessionDeleteActions 最小切片；仅抽取会话页单个/批量删除动作、删除弹窗文案与删除状态管理，不迁移 product 的 Session Cloud 或会话页大重构。
- ✅ 已完成（2026-07-05）：迁移 useSessionActions 最小切片；仅抽取会话页复制文本与恢复会话动作，保留 macOS 终端启动和非 macOS 复制兜底行为，不迁移 product 的 Session Cloud 或会话页大重构。
- ✅ 已完成（2026-07-05）：迁移 useSessionSelectionState 最小切片；仅抽取会话页批量选择纯 UI 状态、过滤收窄清理与选择 key 移除逻辑，不迁移 product 的 Session Cloud 或会话页大重构。
- ✅ 已完成（2026-07-05）：迁移 useAppKeyboardShortcuts 最小切片；仅抽取公开仓 App 内已有 Ctrl/Cmd+`,` 与 Escape 回退逻辑，保留当前视图回退规则和可编辑目标/弹窗锁定跳过行为，未迁移 product 新导航模型。
- ✅ 已完成（2026-07-05）：迁移 useProviderOmoActions 最小切片；仅抽取 App 内已有 OMO / OMO Slim 停用 mutation 与 toast 逻辑，保持 ProviderWorkspacePanel 传参条件不变，新增公开仓 hook 定向测试。
- ✅ 已完成（2026-07-05）：迁移 App startup checks 抽取最小切片；仅抽取启动环境变量冲突检查、配置迁移 toast、skills 迁移 toast/invalidate 与 activeApp 切换冲突合并逻辑，新增公开仓 useAppStartupChecks 与定向 hook 测试。
- ✅ 已完成（2026-07-05）：迁移 useEnvBannerActions 最小切片；仅抽取 EnvWarningBanner dismiss/deleted 动作与对应定向测试，App.tsx 仅替换 EnvWarningBanner 的 dismiss/deleted 回调，未迁移 product 的 App 大结构。
- ✅ 已完成（2026-07-05）：收口 useEnvBannerActions 测试隔离与 App 集成测试冷启动稳定化；定向组合测试不再因模块级 mock 污染或 App 动态导入耗时超时失败。
- ✅ 已完成（2026-07-05）：迁移 App 事件订阅抽取最小切片；仅抽取 provider-switched、universal-provider-synced 与 webdav-sync-status-updated 三段订阅逻辑，新增公开仓 useAppEventSubscriptions 与定向 hook 测试。
- ✅ 已完成（2026-07-05）：补齐 Tauri 事件测试隔离 helper；全局测试清理会重置 mock event listeners，降低事件订阅类测试的跨用例污染风险。
- ✅ 已完成（2026-07-05）：设置元数据错误路径测试噪声收口小切片；同步 product 参考的 console.error spy，仅屏蔽 `[useSettingsMetadata]` 预期错误日志并在 afterEach 恢复，避免污染其他测试。
- ✅ 已完成（2026-07-05）：SQL 导出头品牌收口小切片；新导出的 SQL 备份 header 已统一为 `-- bianma.ai SQLite 导出`，导入继续兼容历史 `-- CC Switch SQLite 导出` 文件并补充定向 Rust 单测。
- ✅ 已完成（2026-07-05）：迁移主题 localStorage 主键兼容小切片；ThemeProvider 默认主键切换到 `bianma-theme`，兼容迁移并清理旧 `cc-switch-theme`，补充新旧键优先级定向单测。
- ✅ 已完成（2026-07-05）：同步用户手册导入提示默认导出文件名品牌；中英文数据库备份导入提示已统一为 `bianma-export-{时间戳/timestamp}.sql`。
- ✅ 已完成（2026-07-05）：按白名单最小切片迁移 useImportExport 默认导出文件名品牌收口；默认 SQL 导出文件名已从 `cc-switch-export-*` 改为 `bianma-export-*`，并补充 saveFileDialog 默认文件名断言。
- ✅ 已完成（2026-07-05）：按白名单最小切片迁移 Failover 前端 tooltip 资源化与组件单测；仅移除 FailoverToggle 与 FailoverPriorityBadge 内 tooltip 中文 fallback，并补充资源化 tooltip 与切换 action 参数定向单测。
- ✅ 已完成（2026-07-05）：按白名单最小切片迁移 ProxyToggle tooltip 资源化与组件单测；移除组件内 tooltip 中文 fallback，确认中英日已有资源 key，并补充 inactive/active/broken 与切换动作定向单测。
- ✅ 已完成（2026-07-05）：迁移会话删除纯工具最小切片；新增 deleteUtils 复用删除目标过滤、删除参数映射与批量结果汇总逻辑，SessionManagerPage 已替换内联实现并补充定向单测。
- ✅ 已完成（2026-04-12）：清理公开深链接测试页 `deplink.html` 的品牌口径，统一页面标题、说明文案与使用提示到 `bianma-app` / `bianma://` 主语境。
- ✅ 已完成（2026-04-12）：停用公开仓 `release.yml`，避免把公开仓误当成当前正式发布仓。
- ✅ 已完成（2026-04-12）：移除 `CODE_OF_CONDUCT.md` 中的旧个人邮箱，统一改为当前公开维护入口。
- ✅ 已完成（2026-04-12）：压缩公开用户手册主路径中的历史 `cc-switch` 命名解释，并统一回指迁移兼容说明。
- ✅ 已完成（2026-04-12）：继续压缩 FAQ / Skills / 导入说明 / 英日文索引中的历史命名提醒，减少旧标识在公开主路径的前置暴露。
- ✅ 已完成（2026-04-12）：收束开发者协议文档与 Flatpak 兼容文档中的 legacy 标识说明，统一改为兼容标识清单与迁移导向。
- ✅ 已完成（2026-07-04）：单仓完全开源收口，`bianma-app` 改为后续唯一正式开发、发布与 updater 目标仓；基础 manifest 已同步到 `bianma-app / 0.0.1 / CreatorEdition/bianma-app`。
- ✅ 已完成（2026-07-04）：切片 1 清理开源单仓发布身份与 release/updater 残留；前端 release 链接与 Rust fallback 更新入口已指向 `CreatorEdition/bianma-app`，公开 `release.yml` 已改为安全预检占位，不再声明由 `bianma-app-product` 私有仓承接。
- ✅ 已完成（2026-07-04）：切片 2 清理公开可见品牌入口；主 UI、About 面板、Windows 覆盖窗口标题与 Flatpak 用户可见元数据已统一到 `bianma.ai` / `CreatorEdition/bianma-app`。
- ✅ 已完成（2026-07-04）：切片 3 清理 i18n 应用标题；中英日 `app.title` 与 `app.description` 已统一到 `bianma.ai` 和本地 AI 编码控制面口径。
- ✅ 已完成（2026-07-05）：迁移 Provider 批量延迟测速最小切片；已补齐缓存表、DAO、Tauri 命令与前端 API 基础能力，未迁移 ProviderWorkspacePanel 大 UI。
- ✅ 已完成（2026-07-05）：补齐 Provider Workspace 未来依赖的通用模型发现 API 基础能力；新增 `fetch_provider_models` 命令、结构化错误、前端类型与 API 包装，未迁移 ProviderWorkspacePanel 大 UI。
- ✅ 已完成（2026-07-05）：迁移 Provider Workspace 前置依赖的 storageCompat 最小切片；仅补齐本地存储兼容工具与必要单测，未迁移 ProviderWorkspacePanel 大 UI。
- ✅ 已完成（2026-07-05）：应用级 localStorage 主键切换兼容小切片；`last-app`、`last-view` 与更新提醒关闭版本已切换到 `bianma-*` 主键，并保留旧键自动迁移清理。
- ✅ 已完成（2026-07-05）：ProviderWorkspacePanel 前置依赖切片 1，补齐公开仓所需的 ProviderMeta 收藏/模型发现协议字段与 providerConfigUtils 最小连接信息导出。
- ✅ 已完成（2026-07-05）：ProviderWorkspacePanel 前置依赖切片 2，给 ProviderList 增加最小 displayMode 支持；single 模式禁用搜索浮层快捷键与拖拽上下文，仅渲染传入供应商卡片。
- ✅ 已完成（2026-07-05）：迁移 ProviderWorkspacePanel 主 UI 切片；默认 providers 分支已接入工作台面板，保留搜索、收藏、键盘导航、模型发现、测速与单卡详情能力，未迁移合作方权重排序和内部 shell 变量。
- ✅ 已完成（2026-07-05）：修复 ProviderWorkspacePanel 审核问题；收口模型发现协议切换后的自动重入，确保旧请求不覆盖新协议结果且同 provider/protocol 不重复自动发现。
- ✅ 已完成（2026-07-05）：迁移剪贴板兼容兜底最小切片；`copy_text_to_clipboard` 保留 `arboard` 主路径，并在失败时使用系统命令写入剪贴板。
- ✅ 已完成（2026-07-05）：迁移 Provider 表单 key 输入字段最小切片；新增 providerKeyUtils 与 ProviderKeyField，OpenCode/OpenClaw 供应商标识输入已复用共享字段并补充最小单测。
- ✅ 已完成（2026-07-05）：迁移 Provider 预设列表工具最小切片；新增 providerPresetUtils，ProviderForm 预设条目构造、分组、分类 key 与标签已改为复用共享工具并补充最小单测。
- ✅ 已完成（2026-07-05）：迁移 Provider 预设选择应用工具最小切片；新增 providerPresetApplyUtils，ProviderForm 预设选择分支已复用 custom 重置计划与选择结果解析工具并补充最小单测。
- ✅ 已完成（2026-07-05）：迁移 Provider 提交校验/配置解析工具最小切片；新增 providerSubmitUtils，提交前供应商标识、非官方凭据与 Codex/Gemini/OMO settingsConfig 解析已改为共享工具并补充最小单测。
- ✅ 已完成（2026-07-05）：迁移 BasicFormFields 图标选择器可访问性与交互最小切片；已按流程先记录进行中，完成后对齐 Dialog 标题/描述、测试标识、选中即关闭与移除独立完成按钮，并补充定向单测。
- ✅ 已完成（2026-07-05）：按白名单最小切片迁移 ProviderActions 按钮文案资源化与组件单测；仅移除 6 个指定按钮文案中文 fallback，并补充资源化来源断言。
- ✅ 已完成（2026-07-05）：按白名单最小切片迁移 ProviderKeyField 编辑态锁定/加载提示行为；仅调整共享字段锁定判定、表单传参与组件单测，未迁移任何明确排除的非目标能力。
- ✅ 已完成（2026-07-05）：Rust 后端后置同步 AppState 复用最小切片；导入、WebDAV 下载与手动 live sync 均复用当前 AppState，不再通过 Arc<Database> 重新构造 AppState，未迁入 providerRuleRegistry/providerRuleCenter、Risk Guard、SessionCloud、订阅配额或其他 product 私有能力。

## 维护边界

- 后续产品代码与公开文档默认都进入本仓。
- release / updater / 二进制分发默认以本仓为准。
- 内部任务拆解、灰度状态和迁移审计材料仍应优先写入 `.teamwork/` 或 `docs/`，避免污染用户文档入口。
- 从 `bianma-app-product` 合入内容前，必须先完成差异审计和敏感信息审计。

## 仍待后续复查

- ⚠️ 需要分批审计 `bianma-app-product` 与本仓差异，确认哪些代码、文档与发布配置应合入。
- ⚠️ 合入前必须复查密钥、私有 URL、签名配置、内部任务记录与未公开合作方材料。
- ⚠️ 正式公开打包发布仍需后续门禁：签名与 notarization、版本号策略、`latest.json` 生成、跨平台构建矩阵、release artifact 上传和人工发布审批。

## 说明

- 本文件只记录 `bianma-app` 仓内主线边界；跨仓治理以根级 `docs/` 为准。
