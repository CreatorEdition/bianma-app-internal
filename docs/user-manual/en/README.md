# bianma-app User Manual

> English mirror of the public bianma-app documentation for Claude Code / Codex / Gemini CLI / OpenCode / OpenClaw.

This manual follows the Chinese documentation as the canonical public source.

## Recommended Reading Order

1. [Chinese user manual](../zh/README.md) for the primary public entry
2. [1.1 Introduction](./1-getting-started/1.1-introduction.md)
3. [1.2 Installation Guide](./1-getting-started/1.2-installation.md)
4. [1.4 Quick Start](./1-getting-started/1.4-quickstart.md)
5. [5.3 Deep Link Protocol (`bianma://`)](./5-faq/5.3-deeplink.md)
6. [Migration compatibility guide (ZH)](../zh/5-faq/5.5-migration-compatibility.md)

## Positioning

This public repository documents the open-source core, community layer, compatibility layer, and public integration surface of bianma-app.

- Public brand: `bianma-app`
- Primary public URI scheme: `bianma://`
- Migration compatibility details: see the [migration compatibility guide (ZH)](../zh/5-faq/5.5-migration-compatibility.md)

## Key Links

- [Chinese user manual](../zh/README.md)
- [Developer URI protocol](../../developers/bianma-uri-protocol.md)
- [Migration compatibility guide (ZH)](../zh/5-faq/5.5-migration-compatibility.md)
- [CHANGELOG](../../../CHANGELOG.md)

Historical release notes under `docs/release-notes/` are archival records and should not be treated as the primary current documentation entry.

## File List

### 1. Getting Started

| File | Description |
|------|-------------|
| [1.1-introduction.md](./1-getting-started/1.1-introduction.md) | Introduction, public positioning, supported tools |
| [1.2-installation.md](./1-getting-started/1.2-installation.md) | Installation and local development setup |
| [1.3-interface.md](./1-getting-started/1.3-interface.md) | Interface layout, navigation bar, provider cards |
| [1.4-quickstart.md](./1-getting-started/1.4-quickstart.md) | 5-minute quick start tutorial |
| [1.5-settings.md](./1-getting-started/1.5-settings.md) | Language, theme, directories, cloud sync settings |

### 2. Provider Management

| File | Description |
|------|-------------|
| [2.1-add.md](./2-providers/2.1-add.md) | Using presets, custom configuration, universal providers |
| [2.2-switch.md](./2-providers/2.2-switch.md) | Main UI switching, tray switching, activation methods |
| [2.3-edit.md](./2-providers/2.3-edit.md) | Edit configuration, modify API Key, backfill mechanism |
| [2.4-sort-duplicate.md](./2-providers/2.4-sort-duplicate.md) | Drag-to-reorder, duplicate provider, delete |
| [2.5-usage-query.md](./2-providers/2.5-usage-query.md) | Usage query, remaining balance, multi-plan display |

### 3. Extensions

| File | Description |
|------|-------------|
| [3.1-mcp.md](./3-extensions/3.1-mcp.md) | MCP protocol, add servers, app binding |
| [3.2-prompts.md](./3-extensions/3.2-prompts.md) | Create presets, activate/switch, smart backfill |
| [3.3-skills.md](./3-extensions/3.3-skills.md) | Discover skills, install/uninstall, repository management |
| [3.4-sessions.md](./3-extensions/3.4-sessions.md) | Session Manager: browse, search, resume, delete sessions |
| [3.5-workspace.md](./3-extensions/3.5-workspace.md) | Workspace files and daily memory (OpenClaw) |

### 4. Proxy & High Availability

| File | Description |
|------|-------------|
| [4.1-service.md](./4-proxy/4.1-service.md) | Start proxy, configuration, running status |
| [4.2-takeover.md](./4-proxy/4.2-takeover.md) | App takeover, configuration changes, status indicators |
| [4.3-failover.md](./4-proxy/4.3-failover.md) | Failover queue, circuit breaker, health status |
| [4.4-usage.md](./4-proxy/4.4-usage.md) | Usage statistics, trend charts, pricing configuration |
| [4.5-model-test.md](./4-proxy/4.5-model-test.md) | Model test, health check, latency testing |

### 5. FAQ / Protocol / Migration

| File | Description |
|------|-------------|
| [5.1-config-files.md](./5-faq/5.1-config-files.md) | bianma-app storage, CLI configuration file formats |
| [5.2-questions.md](./5-faq/5.2-questions.md) | Frequently asked questions |
| [5.3-deeplink.md](./5-faq/5.3-deeplink.md) | Public deep link protocol and usage |
| [5.4-env-conflict.md](./5-faq/5.4-env-conflict.md) | Environment variable conflict detection and resolution |
| [../zh/5-faq/5.5-migration-compatibility.md](../zh/5-faq/5.5-migration-compatibility.md) | Migration compatibility guide (Chinese-first) |

## Contributing

- [GitHub Issues](https://github.com/CreatorEdition/bianma-app/issues)
- [GitHub Repository](https://github.com/CreatorEdition/bianma-app)
