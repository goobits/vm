# Pre-configured Python image with Goobits Package Server
FROM python:3.11-slim

# Accept server address as build argument
ARG PKG_SERVER_URL=http://localhost:3080

# Configure pip to use the package server
RUN mkdir -p /root/.pip && \
    echo "[global]" > /root/.pip/pip.conf && \
    echo "index-url = ${PKG_SERVER_URL}/pypi/simple/" >> /root/.pip/pip.conf

# Also set environment variable for runtime configuration
ENV PIP_INDEX_URL=${PKG_SERVER_URL}/pypi/simple/

# Add a label to identify this as a Goobits-configured image
LABEL goobits.configured="true"
LABEL goobits.server="${PKG_SERVER_URL}"

# Optional: Test the configuration
RUN pip config list || true

# Ready to use - all pip installs will now check your server first!
CMD ["python"]