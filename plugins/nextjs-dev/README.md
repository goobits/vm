# Next.js Development Plugin

Full-stack React framework environment with SSR, API routes, and modern tooling.

## What's Included

### NPM Packages
- `next` - Next.js framework
- `@next/eslint-plugin-next` - Next.js ESLint plugin
- `@next/font` - Font optimization
- `sharp` - Image optimization
- `typescript` - TypeScript compiler
- `@types/node` - Node.js type definitions
- `@types/react` - React type definitions
- `@types/react-dom` - React DOM type definitions
- `eslint-config-next` - ESLint configuration
- `react-testing-library` - Testing utilities
- `jest` - Testing framework
- `jest-environment-jsdom` - Jest DOM environment

### Python Tools
- `git-filter-repo` - Git history tools

### Optional Services
- Redis (enable manually if needed)
- PostgreSQL (enable manually if needed)

### Environment
- `NODE_ENV=development`
- `NEXT_PUBLIC_ENV=development`
- `NEXT_TELEMETRY_DISABLED=1`

## Installation

```bash
vm plugin install plugins/nextjs-dev
```

## Usage

```bash
vm config preset next
```

## License

MIT