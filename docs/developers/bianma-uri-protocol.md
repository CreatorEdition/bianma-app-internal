# bianma URI Protocol (Public)

This document defines the public `bianma://` URI protocol for external developers, site owners, and integration partners.

## Overview

`bianma://` is the public deep link protocol for importing configuration resources described in open documentation.

Protocol baseline:

```text
bianma://v1/import?resource={type}&app={app}&name={name}&...
```

## Public resources

Supported public resource types:

- `provider`
- `mcp`
- `prompt`
- `skill`

## URL format

Required parameters:

- `resource`: `provider` / `mcp` / `prompt` / `skill`
- `name`: display name

Optional common parameter:

- `app`: `claude` / `codex` / `gemini` / `opencode` / `openclaw`

## Examples

Provider import:

```text
bianma://v1/import?resource=provider&app=claude&name=My%20Provider&endpoint=https%3A%2F%2Fapi.example.com&apiKey=sk-xxx
```

MCP import:

```text
bianma://v1/import?resource=mcp&name=mcp-fetch&apps=claude,codex&config=eyJtY3BTZXJ2ZXJzIjp7ImZldGNoIjp7ImNvbW1hbmQiOiJ1dngiLCJhcmdzIjpbIm1jcC1zZXJ2ZXItZmV0Y2giXX19fQ==
```

Prompt import:

```text
bianma://v1/import?resource=prompt&app=claude&name=Code%20Review&content=IyBSb2xlCllvdSBhcmUgYSBzdHJpY3QgcmV2aWV3ZXIu
```

Skill import:

```text
bianma://v1/import?resource=skill&name=my-skill&repo=owner/repo&directory=skills/my-skill&branch=main
```

## Integration guidance

- Always URL-encode query values.
- For `prompt` and `mcp`, Base64-encode the content payload before embedding it in the URL.
- Do not publish real production API keys in shared links.
- Prefer minimal-scope test credentials for demos and docs.

## Public tool support status

Supported in the current open-source implementation:

- Claude Code (`app=claude`)
- Codex CLI (`app=codex`)
- Gemini CLI (`app=gemini`)
- OpenCode (`app=opencode`)
- OpenClaw (`app=openclaw`)

Not publicly supported:

- Cursor
- Windsurf
- Cline

If a tool is not listed as supported, do not treat it as publicly supported.

## Migration Note

`ccswitch://` remains only as a migration compatibility alias.
New public integrations should use `bianma://`, and legacy behavior should be referenced only from migration documentation:

- [Migration compatibility guide (ZH)](../user-manual/zh/5-faq/5.5-migration-compatibility.md)
