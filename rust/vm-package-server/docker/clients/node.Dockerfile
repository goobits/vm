# Pre-configured Node.js image with Goobits Package Server
FROM node:18-slim

# Accept server address as build argument
ARG PKG_SERVER_URL=http://localhost:3080

# Configure npm to use the package server
RUN npm config set registry ${PKG_SERVER_URL}/npm/

# Also set environment variable for runtime configuration
ENV NPM_CONFIG_REGISTRY=${PKG_SERVER_URL}/npm/

# Add a label to identify this as a Goobits-configured image
LABEL goobits.configured="true"
LABEL goobits.server="${PKG_SERVER_URL}"

# Optional: Show the configuration
RUN npm config get registry

# Ready to use - all npm installs will now check your server first!
CMD ["node"]