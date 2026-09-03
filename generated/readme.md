# Generated output — do not edit directly

Everything in this directory except this notice is derivative output. The committed Dart, Gleam, Rust, and TypeScript bindings are generated from repository-owned source inputs; edit those inputs and regenerate instead of patching language output by hand.

## Contract authority

TypeSpec and JSON Schema/OpenAPI are independent, human-authored peer authorities for semantic cross-language contracts. Neither may be generated from the other as the ultimate source of truth. A generated contract change is mergeable only when:

1. both peer authorities change together;
2. their normalized outputs agree; and
3. a machine-readable reconciliation receipt is committed under `../contracts/parity/`.

The generated-policy workflow enforces those requirements for future changes below this directory. The existing output predates the complete peer-authority migration and must not be represented as already compliant.

## Desktop boundary

This desktop application does not expose a public network API by default. Qt/QML owns presentation; Rust owns validation, domain logic, persistence, networking, and security. Generated contracts are appropriate for configuration, persisted state, local IPC, or an explicit remote wire contract—not for widget or QML implementation details.

## Regenerate and freeze

Before regeneration, temporarily make the tree writable:

```sh
chmod -R u+w generated
```

Run the repository's documented generator from its authoritative inputs, review the complete diff, then make the tree read-only again:

```sh
find generated -depth -exec chmod a-w {} +
```

Git persists only the regular-versus-executable distinction, not arbitrary owner-write bits. A fresh checkout can therefore be writable even when a working tree was frozen locally. The CI policy is the durable merge control; `chmod a-w` is an additional local deterrent.