# VM Issue Integration Tracker

This tracker owns the thin `vm issue` integration. The issue service, provider
credentials, repository mapping, and service lifecycle remain outside this
repository. Package infrastructure is not an issue-management transport.

## Human and Agent API

```bash
vm issue list
vm issue open "title" "description"
vm issue 123
vm issue 123 comment "progress"
vm issue 123 close "summary"
```

`vm issue` is guest-only. It reads a service URL and repository-bound issue
capability from the managed environment, calls the service, and emits the
service's JSON response. It never reads Git configuration, invokes Git, accepts
a repository argument, or receives provider credentials.

## Service Contract

- `GET /v1/issues`
- `POST /v1/issues`
- `GET /v1/issues/{number}`
- `POST /v1/issues/{number}/comments`
- `POST /v1/issues/{number}/close`

The service capability determines the allowed repository and operations.
Mutation requests carry an idempotency key. The service owns authorization,
duplicate prevention, audit records, rate limiting, and GitHub/GitLab adapters.

## Delivery

- [ ] Add the guest-only CLI grammar and HTTP adapter.
- [ ] Validate the endpoint, issue number, response JSON, and argument counts.
- [ ] Keep bearer credentials out of output and cross-origin redirects.
- [ ] Cover list, read, open, comment, close, failures, and context isolation.
- [ ] Prove the Docker workflow without Git or provider credentials.
- [ ] Document the finished command surface in the CLI reference.

## Non-goals

- Managing the issue-service container.
- Storing GitHub or GitLab credentials.
- Adding issue routes to the package gateway or auth proxy.
- Repository discovery from `/workspace` or its Git remote.
- Labels, assignment, search, milestones, or provider-specific commands.
