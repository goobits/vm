# VM Remote Command Integration Tracker

This tracker owns the thin, generic command bridge exposed inside managed
guests. External services own provider credentials, repository mapping,
business rules, and service lifecycle. Package infrastructure is not a remote
API transport.

## Human and Agent API

```bash
vm issue list
vm issue open "title" "description"
vm issue 123
vm issue 123 comment "progress"
```

`issue` is one registered namespace, not a built-in issue subsystem. The same
bridge may expose other explicitly configured namespaces without adding another
VM adapter. Unknown namespaces continue to fail as unknown VM commands.

## Guest Contract

A root-owned, versioned guest registration file maps a command namespace to
one service endpoint and its capability. The guest sends one request:

```text
POST <endpoint>/v1/commands/<namespace>
{"schema":1,"arguments":[...],"idempotency_key":"..."}
```

The response is versioned JSON with an exit status plus stdout or stderr text.
The service owns operation validation, authorization, duplicate prevention,
audit records, rate limiting, and provider adapters.

The guest cannot choose a repository, URL, HTTP method, credential, or provider.
It does not read Git configuration or receive GitHub/GitLab credentials. The
bridge rejects redirects, oversized responses, invalid namespaces, and
unsupported protocol versions, and never logs capability values.

The controller registry is stored outside repositories at
`~/.vm/remote-commands.json` and scopes commands by exact environment name:

```json
{"schema":1,"environments":{"project-dev":{"schema":1,"commands":{"issue":{"endpoint":"http://issue-service:8080","capability":"scoped-token","repair_command":"vm start project-dev"}}}}}
```

## Delivery

- [x] Add strict unknown-command fallback and the generic HTTP dispatcher.
- [x] Validate registrations, endpoints, arguments, response JSON, and limits.
- [x] Install registered namespaces during explicit managed-guest reconciliation.
- [x] Keep host and guest context selection explicit in tests.
- [x] Cover success, mutation idempotency, service failures, and isolation.
- [x] Prove the Docker workflow without Git or provider credentials.

## Compatibility

- Existing built-in VM commands keep their current parsing and behavior.
- Package/tool commands keep their existing transports and credentials.
- Registration is additive and root-controlled; repositories cannot add commands.
- The first registered service may implement issue management, but no
  issue-specific request or response type belongs in VM.

## Non-goals

- A raw HTTP proxy or arbitrary remote API client.
- Managing external service containers or provider credentials.
- Adding provider routes to the package gateway or auth proxy.
- Repository discovery from `/workspace` or its Git remote.
- Provider-specific flags, schemas, or command implementations in VM.
