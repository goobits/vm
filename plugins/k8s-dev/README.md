# Kubernetes Development Plugin

Tools for Kubernetes development and deployment workflows.

## What's Included

### System Packages
- `curl` - HTTP client for making requests.
- `gnupg2` - GPG encryption for repository management.
- `software-properties-common` - Manages software repositories.
- `kubectl` - Kubernetes command-line tool (installed via provisioning script).
- `helm` - Kubernetes package manager (installed via provisioning script).

### Python Packages
- `kubernetes` - Official Kubernetes Python client.
- `pykube-ng` - A lightweight, Pythonic Kubernetes client.

### Aliases
- `k` → `kubectl`
- `kgp` → `kubectl get pods`
- `kgs` → `kubectl get services`
- `kgd` → `kubectl get deployments`
- `kdesc` → `kubectl describe`
- `klog` → `kubectl logs`
- `kexec` → `kubectl exec -it`

### Environment Variables
- `KUBECONFIG` - Specifies the path to the Kubernetes configuration file.
- `KUBE_EDITOR` - Sets the default editor for commands like `kubectl edit`.

## Installation

This plugin is automatically installed with the VM tool. No additional installation required.

To verify availability:
```bash
vm config preset --list | grep kubernetes
```

## Usage

Apply this preset to your project:
```bash
vm config preset kubernetes
vm create
```

Or add to `vm.yaml`:
```yaml
preset: kubernetes
```

## Configuration

### Additional Packages
```yaml
preset: kubernetes
packages:
  pip:
    - new-python-package
  npm:
    - new-npm-package
```

## Common Use Cases

1. **Listing Pods in a Namespace**
   ```bash
   vm exec "kubectl get pods -n my-namespace"
   ```

2. **Applying a Manifest File**
   ```bash
   vm exec "kubectl apply -f deployment.yaml"
   ```

## Troubleshooting

### Issue: `The connection to the server localhost:8080 was refused`
**Solution**: Ensure that your Kubernetes cluster is running and that your `KUBECONFIG` environment variable is pointing to the correct configuration file.

### Issue: `command not found: kubectl`
**Solution**: The `kubectl` binary is installed during the provisioning step. Run `vm provision` to ensure all setup scripts have been executed.

## Related Documentation

- [Configuration Guide](../../docs/user-guide/configuration.md)
- [Presets Overview](../../docs/user-guide/presets.md)
- [CLI Reference](../../docs/user-guide/cli-reference.md)

## License

MIT