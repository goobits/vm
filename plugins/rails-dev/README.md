# Ruby on Rails Development Plugin

Full-stack Ruby web framework environment with PostgreSQL and Redis support.

## What's Included

### System Packages
- `ruby-dev` - Ruby development files
- `build-essential` - Build tools
- `libpq-dev` - PostgreSQL development files
- `nodejs` - JavaScript runtime
- `yarn` - Package manager

### Ruby Gems (via provision script)
- `rails` - Web framework
- `pg` - PostgreSQL adapter
- `redis` - Redis client
- `sidekiq` - Background job processor
- `rspec-rails` - Testing framework
- `factory_bot_rails` - Test fixtures
- `rubocop` - Linter
- `brakeman` - Security scanner
- `bundler-audit` - Dependency security checker

### NPM Packages
- `prettier` - Code formatter
- `webpack` - Asset bundler

### Optional Services
- PostgreSQL (enable manually if needed)
- Redis (enable manually if needed)

### Environment
- `RAILS_ENV=development`
- `BUNDLE_PATH=vendor/bundle`

## Installation

```bash
vm plugin install plugins/rails-dev
```

## Usage

```bash
vm config preset rails
```

## License

MIT