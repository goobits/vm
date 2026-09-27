# Dependency and runtime maintenance

Review dependency PRs together when they overlap, but retain immutable action
commits and container image digests. Validate combined changes with workspace
checks, package acceptance, and the repository security workflows before merging.

## Supported image lines

- Package jobs and package acceptance use Node 22 LTS. Update patch releases and
  image digests on this line. A Node major migration requires checking the job
  toolchain: Node 25 and later no longer bundle Corepack, while the jobs image
  currently runs `corepack disable`. Node 26 is Current, not LTS, as of September
  2026. See [Node release policy](https://nodejs.org/en/about/previous-releases)
  and [Corepack availability](https://nodejs.org/download/release/v25.8.0/docs/api/corepack.html).
- Vibe uses Ubuntu 22.04, whose standard security maintenance ends in May 2027.
  Plan its migration before then. Changing only `FROM` to Ubuntu 26.04 is unsafe:
  package installation still selects the Jammy Python PPA and distro-specific
  Chromium packages. See [Ubuntu lifecycle](https://ubuntu.com/about/release-cycle).
- Rust builders and the CLI/package-server runtime use Debian Bookworm. Keep
  runtime libc compatible with the builder when changing either base.
- Dependabot continues digest/minor/patch and security updates. Ubuntu distro-series changes (including minor versions such as 22.10) and Node
  major changes require an explicit migration with live acceptance; their routine
  migration PRs are excluded in `.github/dependabot.yml`.

## Workflow and security checks

GitHub-hosted runners execute dependency policy checks and CodeQL analysis for
Rust and GitHub Actions. All workflows are opt-in; run security scans explicitly
from Actions or `gh workflow run security.yml --ref <branch>`. Repository Dependabot alerts/security updates, secret scanning, and
secret push protection are enabled. Review alert results; a successful scan is
not a guarantee that all vulnerabilities have been found.

Actions using Node 24 require runner 2.327.1 or newer. Keep the protected
self-hosted Tart publisher current (at least 2.329.0 for container-based
checkout authentication). Release workflows retain protected environments and
immutable publication checks. Validate artifact transport by manually running `ci.yml`;
do not publish releases or overwrite existing image tags merely to test actions.
