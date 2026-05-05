# Security Policy

OpenFiles touches object storage credentials and can expose mounted file paths to workloads. Treat it as security-sensitive infrastructure.

## Reporting vulnerabilities

Email security reports to `security@example.invalid` or open a private advisory in your fork/organization.

## Security model

- Components receive filesystem access only through explicit wasmCloud volume mounts or explicit OpenFiles WIT links.
- Credentials are read from environment variables, Kubernetes Secrets, or config files. Do not commit real credentials.
- The daemon should run with the minimum permissions needed for its target bucket/prefix.
- Conflict resolution defaults to object-store source-of-truth.
- Lost+found data remains local until a user intentionally copies it out.

## Hardening checklist

- Use prefix-scoped credentials or bucket policies.
- Enable object versioning where available.
- Use TLS endpoints.
- Use encrypted cache volumes for sensitive data.
- Set `cache.expiration_days` to match your data lifecycle.
- Disable FUSE mount for untrusted local users unless OS permissions are locked down.
