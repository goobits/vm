# Kubernetes Development Plugin

Tools for Kubernetes development and deployment workflows.

## What's Included

### System Packages
- `curl` - HTTP client
- `gnupg2` - GPG encryption
- `software-properties-common` - Repository management

### Python Packages
- `kubernetes` - Kubernetes Python client
- `pykube-ng` - Pythonic Kubernetes client

### NPM Packages
- `kubernetes-yaml-completion` - YAML completion support

### Aliases
- `k` → `kubectl`
- `kgp` → `kubectl get pods`
- `kgs` → `kubectl get services`
- `kgd` → `kubectl get deployments`
- `kdesc` → `kubectl describe`
- `klog` → `kubectl logs`
- `kexec` → `kubectl exec -it`

### Environment
- `KUBECONFIG=~/.kube/config`
- `KUBE_EDITOR=nano`

### Provisioning
- Installs `kubectl` (latest stable version)
- Installs `helm` (Kubernetes package manager)

## Installation

```bash
vm plugin install plugins/k8s-dev
```

## Usage

```bash
vm config preset kubernetes
```

## License

MIT