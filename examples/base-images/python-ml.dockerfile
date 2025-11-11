# Base image with Python ML/Data Science stack pre-installed
# Reduces VM creation time from ~8 minutes to ~45 seconds
# Last updated: 2025-01-11

FROM ubuntu:24.04

# Install Python and system dependencies
RUN apt-get update && apt-get install -y \
    python3.12 \
    python3-pip \
    python3-venv \
    git \
    build-essential \
    libhdf5-dev \
    libatlas-base-dev \
    gfortran \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -m -s /bin/bash developer
USER developer
WORKDIR /home/developer

# Create virtual environment
RUN python3 -m venv /home/developer/venv
ENV PATH="/home/developer/venv/bin:$PATH"

# Install core ML/DS packages
# These are the time-consuming installations we're pre-caching
RUN pip install --no-cache-dir \
    numpy==1.26.2 \
    pandas==2.1.4 \
    scikit-learn==1.3.2 \
    matplotlib==3.8.2 \
    seaborn==0.13.0 \
    jupyter==1.0.0 \
    ipython==8.18.1 \
    scipy==1.11.4

# Install additional data tools
RUN pip install --no-cache-dir \
    requests==2.31.0 \
    beautifulsoup4==4.12.2 \
    sqlalchemy==2.0.23

WORKDIR /workspace

# Verify installation
RUN python3 -c "import numpy, pandas, sklearn; print('ML stack ready')"
