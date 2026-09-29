# VM command inventory

Generated from the CLI parser. Run `vm help <command>` for options and examples.

| Command | Purpose |
| --- | --- |
| `vm init` | Initialize a project in the current or selected directory |
| `vm create` | Declare and provision a stopped environment |
| `vm start` | Start an existing environment |
| `vm list` | List environments for this project |
| `vm shell` | Provision/start an initialized environment and open a shell |
| `vm exec` | Run a single command inside an environment |
| `vm logs` | Stream output logs from an environment |
| `vm copy` | Move files between host and environment |
| `vm stop` | Gracefully halt an environment |
| `vm status` | Check environment status |
| `vm restart` | Stop and start an environment |
| `vm remove` | Remove environments while preserving persistent data and snapshots |
| `vm snapshots` | Manage environment snapshots and portable archives |
| `vm snapshots list` | List snapshots for the selected environment |
| `vm snapshots show` | Show snapshot metadata |
| `vm snapshots create` | Capture an environment |
| `vm snapshots restore` | Restore a snapshot into an environment |
| `vm snapshots remove` | Delete a snapshot |
| `vm snapshots export` | Export a snapshot as a portable archive |
| `vm snapshots import` | Import a portable snapshot archive |
| `vm packages` | Manage the shared package-infrastructure appliance |
| `vm packages up` | Prepare or reconcile the shared package-infrastructure appliance and configured sources |
| `vm packages down` | Stop the appliance while preserving all named volumes |
| `vm packages service` | Inspect and administer the package appliance |
| `vm packages service init` | Configure the controller source shelf and package appliance |
| `vm packages service status` | Show appliance engine and gateway health |
| `vm packages service doctor` | Validate the runtime, appliance definition, and gateway |
| `vm packages service backups` | Manage private appliance backups |
| `vm packages service backups list` | List appliance-local backups |
| `vm packages service backups create` | Create a consistent backup in a private named volume |
| `vm packages service backups remove` | Remove one exact appliance-local backup |
| `vm packages service backups restore` | Restore a private named-volume backup while services are stopped |
| `vm packages register` | Register repository URLs or remember local Git roots as read-only workspaces |
| `vm packages list` | List registered packages and their publication/consumability state |
| `vm packages show` | Show one registered package |
| `vm packages remove` | Remove a package registration while preserving published versions and source repositories |
| `vm packages consumers` | Manage consumer repositories tracked by the package infrastructure |
| `vm packages consumers retry` | Retry failed dependency updates for a registered consumer |
| `vm packages consumers register` | Register a consumer repository and its current internal dependencies |
| `vm packages consumers list` | List registered consumer repositories |
| `vm packages consumers show` | Show a registered consumer and its declared dependencies |
| `vm packages consumers remove` | Remove a consumer registration while preserving rollout records and source repositories |
| `vm packages consumers drift` | Show package-version drift across registered consumers |
| `vm packages open` | Open an attested package or tool in its owning Docker workspace without copying it |
| `vm packages checkout` | Create or resume an isolated package or tool checkout in this managed guest |
| `vm packages release` | Release the managed checkout or canonical workspace containing this directory |
| `vm packages cancel` | Cancel and clean up the managed checkout containing this directory |
| `vm packages auth` | Manage the controller's private Git token |
| `vm packages auth login` | Import an active GitHub credential or read a token from stdin or a file |
| `vm packages auth status` | Report whether a controller Git credential is configured |
| `vm packages auth logout` | Remove the controller Git credential |
| `vm tools` | Manage immutable tools activated inside project environments |
| `vm tools register` | Register one trusted tool source with package infrastructure |
| `vm tools list` | List VM-owned vendor tools and registered package tools |
| `vm tools show` | Show one vendor definition or package tool and its published releases |
| `vm tools remove` | Remove a managed tool registration while preserving published artifacts |
| `vm tools refresh` | Refresh the appliance-generated tool catalog cache |
| `vm tools status` | Show vendor, registered, published, installed, and consumable tool state |
| `vm tools enable` | Select package tools globally and activate them in running managed Docker environments |
| `vm tools disable` | Stop selecting package tools globally; existing managed files are retained |
| `vm tools update` | Update VM-owned vendor tools and configured package tools across managed environments |
| `vm config` | Manage defaults, providers, and profiles |
| `vm config validate` | Validate the current configuration |
| `vm config show` | Show the loaded configuration and its source |
| `vm config render` | Render the redacted provider configuration without applying it |
| `vm config set` | Change a configuration value |
| `vm config get` | View configuration values |
| `vm config unset` | Remove a configuration value |
| `vm config presets` | Manage configuration presets |
| `vm config presets list` | List available presets |
| `vm config presets show` | Show a preset and its configuration |
| `vm config presets apply` | Apply one or more presets to configuration |
| `vm config profiles` | Manage configuration profiles |
| `vm config profiles list` | List available profiles for this project |
| `vm config profiles show` | Show a named profile |
| `vm config profiles set-default` | Set the default profile for this project |
| `vm config ports` | Fix port conflicts |
| `vm tunnels` | Manage active port forwards |
| `vm tunnels open` | Open a named TCP tunnel through an environment |
| `vm tunnels list` | List project-owned tunnel relays, including orphaned environments |
| `vm tunnels close` | Close one named tunnel |
| `vm doctor` | Diagnose and repair engine issues |
| `vm plugins` | Extend with plugins |
| `vm plugins list` | See installed plugins |
| `vm plugins show` | Get plugin details |
| `vm plugins install` | Add a plugin |
| `vm plugins remove` | Remove a plugin |
| `vm plugins create` | Create a new plugin |
| `vm plugins validate` | Check plugin configuration |
| `vm system` | Self-management and lower-level system tools |
| `vm system info` | Show client, controller, provider, and schema versions |
| `vm system update` | Update this vm installation |
| `vm system uninstall` | Remove vm from this system |
| `vm system images` | Manage provider-native base images |
| `vm system images build` | Build a provider-native base artifact for a preset |
| `vm system storage` | Inspect and remove VM-owned provider storage |
| `vm system storage list` | List VM-owned provider storage with deletion eligibility |
| `vm system storage remove` | Remove one exact, unreferenced disposable resource |
| `vm db` | Database workflows |
| `vm db backups` | Manage PostgreSQL backups |
| `vm db backups list` | List retained backups |
| `vm db backups create` | Create a backup for one database or all databases |
| `vm db backups restore` | Restore a backup into one database |
| `vm db backups remove` | Remove one retained backup |
| `vm db list` | List databases |
| `vm db status` | Show the size and backup count of a database |
| `vm db export` | Export a database to a SQL file |
| `vm db import` | Import a database from a SQL file |
| `vm db reset` | Drop and recreate a database |
| `vm db credentials` | Show credentials metadata, or reveal the value explicitly |
| `vm secrets` | Secret workflows |
| `vm secrets status` | Check secret proxy status |
| `vm secrets set` | Store a secret |
| `vm secrets list` | See all secrets |
| `vm secrets show` | Reveal one secret value |
| `vm secrets remove` | Delete a secret |
