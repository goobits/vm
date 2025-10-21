# Example Configurations

This directory contains example `vm.yaml` configurations and custom base images for various project types. You can use these as a starting point for your own projects.

To use an example, copy the `vm.yaml` file to the root of your project directory and customize it as needed.

## Project Examples

-   **`nextjs-app/`**: A basic configuration for a Next.js web application.
-   **`configurations/`**: Sample configurations (minimal, full-stack)
-   **`services/`**: Service-specific examples (PostgreSQL, Redis, MongoDB)

## Custom Base Images

-   **`base-images/`**: Example Dockerfiles for creating reusable base images with pre-installed dependencies (Playwright, Chromium, dev tools, etc.)

See [base-images/README.md](./base-images/README.md) for details on creating and using custom base images to speed up VM creation.
