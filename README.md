# CC System Prompt Normalizer

A lightweight proxy that sits between Claude Code and LM Studio, normalizing the `cch=` values in Claude Code's system prompts to a fixed value (`cch=0;`). This dramatically increases KV cache hit rates and speeds up time-to-first-token (TTFT) for successive calls.

## How it works

Claude Code includes a `cch=<hex>` token in its system prompt that changes between requests. This defeats LM Studio's KV cache since the prompt is technically different each time. This proxy intercepts requests, replaces all `cch=<hex>;` patterns with `cch=0;`, and forwards the normalized request to LM Studio.

Supports both API formats:
- **Anthropic** (`/v1/messages`): normalizes the top-level `system` field (string or content block array)
- **OpenAI** (`/v1/chat/completions`): normalizes `content` in messages with `role: "system"`

## Usage

```bash
cargo run
```

The proxy listens on `http://127.0.0.1:7609` and forwards to LM Studio at `http://127.0.0.1:1234`.

Point Claude Code's base URL to `http://127.0.0.1:7609` instead of LM Studio directly.

### Verbose logging

To see forwarded request details (URL, body size, headers):

```bash
cargo run -- --verbose-log
```
