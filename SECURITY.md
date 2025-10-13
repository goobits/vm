# Security Policy

## Reporting a Vulnerability

The project team and community take all security vulnerabilities seriously. Thank you for your efforts to responsibly disclose your findings.

To report a security vulnerability, please send an email to the project maintainers at `security@goobits.com`. Please include the following information in your report:

- A description of the vulnerability and its impact.
- Steps to reproduce the vulnerability.
- Any proof-of-concept code.
- Your name and contact information.

We will acknowledge receipt of your vulnerability report within 48 hours and will provide a more detailed response within 72 hours, including our initial assessment of the vulnerability and a timeline for a fix.

## Security Process

Our security process includes the following:

- **Automated Security Scanning:** We use `cargo-deny` to automatically scan our dependencies for known vulnerabilities and license compliance on every pull request.
- **Dependency Updates:** We use Dependabot to automatically create pull requests for dependency updates.
- **Security Audits:** We periodically conduct security audits of our codebase.
- **Vulnerability Disclosure:** We will publicly disclose vulnerabilities once a fix is available.
