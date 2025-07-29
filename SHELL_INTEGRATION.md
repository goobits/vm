# VM Tool Directory Tracking

The VM tool tracks your current directory inside the VM. When you navigate to subdirectories inside the VM and exit, the tool remembers where you were.

## How It Works

1. **Inside VM**: When you navigate to subdirectories and exit the VM, your current directory is saved
2. **Directory Tracking**: The VM tool tracks this information internally
3. **Manual Sync**: After exiting, you can retrieve the directory and change to it

## Usage

After exiting the VM, check what directory you were in:
```bash
vm get-sync-directory
```

To change to that directory:
```bash
cd $(vm get-sync-directory)
```

Or as a one-liner after SSH:
```bash
vm ssh && cd $(vm get-sync-directory)
```

## Example Session

```bash
# Start in your project root
$ pwd
/home/user/myproject

# SSH into VM
$ vm ssh
🚀 vm-dev /workspace > cd src/components
🚀 vm-dev components > exit

# Back on host, check where you were
$ vm get-sync-directory
/home/user/myproject/src/components

# Change to that directory
$ cd $(vm get-sync-directory)
$ pwd
/home/user/myproject/src/components
```

## Troubleshooting

- **No output from get-sync-directory**: This happens when:
  - You were outside the project workspace when you exited
  - The corresponding host directory doesn't exist
  - No directory state was saved
- **Multiple VMs**: Each VM tracks its own directory state independently