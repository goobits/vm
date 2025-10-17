# Ruby on Rails Development Plugin

Full-stack Ruby web framework environment with PostgreSQL and Redis support.

## What's Included

### System Packages
- `ruby-dev` - Contains header files for compiling Ruby extensions.
- `build-essential` - Informational list of packages considered essential for building Debian packages.
- `libpq-dev` - Header files for PostgreSQL C application development.
- `nodejs` - JavaScript runtime.
- `yarn` - Package manager for your code.

### Ruby Gems
- `rails` - A web-application framework that includes everything needed to create database-backed web applications.
- `pg` - PostgreSQL adapter for Ruby.
- `redis` - Redis client for Ruby.
- `sidekiq` - Simple, efficient background processing for Ruby.
- `rspec-rails` - Testing framework for Rails.
- `rubocop` - A Ruby static code analyzer and formatter.

### NPM Packages
- `prettier` - An opinionated code formatter.
- `webpack` - A static module bundler for modern JavaScript applications.

### Environment Variables
- `RAILS_ENV` - Defines the Rails environment, set to `development`.
- `BUNDLE_PATH` - The location where gems are installed, set to `vendor/bundle`.

### Included Services
This preset includes the following services **enabled by default**:
- **PostgreSQL** - The default database for Rails applications.
- **Redis** - Used for caching and background jobs with Sidekiq.

To disable or customize services, see the [Configuration](#configuration) section below.

## Installation

This plugin is automatically installed with the VM tool. No additional installation required.

To verify availability:
```bash
vm config preset --list | grep rails
```

## Usage

Apply this preset to your project:
```bash
vm config preset rails
vm create
```

Or add to `vm.yaml`:
```yaml
preset: rails
```

## Configuration

### Customizing Services
```yaml
preset: rails
services:
  postgresql:
    database: my_rails_app_development
  redis:
    enabled: false
```

### Additional Packages
```yaml
preset: rails
packages:
  gem:
    - devise
  npm:
    - tailwindcss
```

## Common Use Cases

1. **Running Database Migrations**
   ```bash
   vm exec "rails db:migrate"
   ```

2. **Starting the Rails Server**
   ```bash
   vm exec "rails server -b 0.0.0.0"
   ```

## Troubleshooting

### Issue: Gem installation failure
**Solution**: Some gems have native extensions that require specific system libraries. Ensure all necessary `-dev` packages are listed under `packages.system` in your `vm.yaml`. Run `vm apply` after updating the configuration.

### Issue: `ActiveRecord::NoDatabaseError`
**Solution**: Make sure you have created the database. Run `vm exec "rails db:create"` and then `vm exec "rails db:migrate"`.

## Related Documentation

- [Configuration Guide](../../docs/user-guide/configuration.md)
- [Presets Overview](../../docs/user-guide/presets.md)
- [CLI Reference](../../docs/user-guide/cli-reference.md)

## License

MIT