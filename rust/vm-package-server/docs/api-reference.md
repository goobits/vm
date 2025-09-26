# Package Server API Documentation

A comprehensive guide to the Goobits Package Server REST API supporting PyPI, NPM, and Cargo package registries.

## Table of Contents

- [Overview](#overview)
- [Base URL](#base-url)
- [Authentication](#authentication)
- [PyPI API](#pypi-api)
- [NPM API](#npm-api)
- [Cargo API](#cargo-api)
- [Management API](#management-api)
- [UI Endpoints](#ui-endpoints)
- [Error Handling](#error-handling)
- [Response Formats](#response-formats)

## Overview

The Goobits Package Server provides a unified package registry supporting three major package ecosystems:

- **PyPI** (Python Package Index) - Compatible with PEP 503 simple repository API
- **NPM** (Node Package Manager) - Compatible with NPM registry API
- **Cargo** (Rust Package Manager) - Compatible with Cargo registry API

Each ecosystem maintains its own namespace and follows the respective package manager's API conventions.

## Base URL

```
http://localhost:3080
```

The server runs on port 3080 by default and can be configured via command line arguments.

## Authentication

The server supports optional Bearer token authentication for package upload endpoints. When enabled via configuration, upload operations require a valid API key in the Authorization header.

**Authentication Header Format:**
```
Authorization: Bearer your-api-key-here
```

See the [Configuration Guide](configuration.md) for details on enabling authentication.

## PyPI API

### Package Discovery

#### Get Simple Index
Lists all available Python packages.

```http
GET /pypi/simple/
```

**Response**: HTML page with links to all packages
```html
<!DOCTYPE html>
<html>
  <head><title>Simple index</title></head>
  <body>
    <h1>Simple index</h1>
    <a href="package-name/">package-name</a><br/>
  </body>
</html>
```

#### Get Package Links
Returns download links for all versions of a specific package.

```http
GET /pypi/simple/{package}/
```

**Parameters**:
- `package` (string): Package name (normalized according to PEP 503)

**Response**: HTML page with download links including SHA256 hashes
```html
<!DOCTYPE html>
<html>
  <head><title>Links for package-name</title></head>
  <body>
    <h1>Links for package-name</h1>
    <a href="../packages/package_name-1.0.0-py3-none-any.whl#sha256=abcd...">package_name-1.0.0-py3-none-any.whl</a><br/>
  </body>
</html>
```

### Package Operations

#### Download Package File
Downloads a specific package file (.whl or .tar.gz).

```http
GET /pypi/packages/{filename}
```

**Parameters**:
- `filename` (string): Package filename

**Response**: Binary package data

#### Upload Package
Uploads a new Python package.

```http
POST /pypi/
```

**Content-Type**: `multipart/form-data`

**Body**: Package files (.whl or .tar.gz)

**Response**:
```json
{
  "message": "Package uploaded successfully",
  "status": "success"
}
```

#### Delete Package Version
Deletes a specific version of a package.

```http
DELETE /pypi/{package_name}/{version}
```

**Parameters**:
- `package_name` (string): Package name
- `version` (string): Version to delete

**Response**:
```json
{
  "message": "Package version deleted successfully",
  "status": "success"
}
```

#### Delete All Package Versions
Completely removes a package and all its versions.

```http
DELETE /pypi/package/{package_name}
```

**Parameters**:
- `package_name` (string): Package name

**Response**:
```json
{
  "message": "Package deleted successfully",
  "status": "success"
}
```

## NPM API

### Package Discovery

#### Get Package Metadata
Returns complete package metadata including all versions.

```http
GET /npm/{package}
```

**Parameters**:
- `package` (string): Package name (supports scoped packages like @scope/package)

**Response**:
```json
{
  "name": "package-name",
  "versions": {
    "1.0.0": {
      "name": "package-name",
      "version": "1.0.0",
      "dist": {
        "tarball": "http://localhost:3080/npm/package-name/-/package-name-1.0.0.tgz",
        "shasum": "abcd1234..."
      }
    }
  }
}
```

### Package Operations

#### Download Tarball
Downloads an NPM package tarball.

```http
GET /npm/{package}/-/{filename}
```

**Parameters**:
- `package` (string): Package name
- `filename` (string): Tarball filename (e.g., "package-1.0.0.tgz")

**Response**: Binary tarball data

#### Publish Package
Publishes a new NPM package version.

```http
PUT /npm/{package}
```

**Content-Type**: `application/json`

**Body**:
```json
{
  "_id": "package-name",
  "name": "package-name",
  "versions": {
    "1.0.0": {
      "name": "package-name",
      "version": "1.0.0",
      "dist": {
        "tarball": "http://server/npm/package/-/package-1.0.0.tgz"
      }
    }
  },
  "_attachments": {
    "package-1.0.0.tgz": {
      "data": "base64-encoded-tarball",
      "content_type": "application/octet-stream"
    }
  }
}
```

**Response**:
```json
{
  "message": "Package published successfully",
  "status": "success"
}
```

#### Delete Package Version
Unpublishes a specific version of an NPM package.

```http
DELETE /npm/{package_name}/{version}
```

**Parameters**:
- `package_name` (string): Package name
- `version` (string): Version to unpublish

**Response**:
```json
{
  "message": "Unpublished version 1.0.0 of NPM package 'my-package'"
}
```

#### Delete All Package Versions
Completely removes an NPM package.

```http
DELETE /npm/package/{package_name}
```

**Parameters**:
- `package_name` (string): Package name

**Response**:
```json
{
  "message": "Deleted 3 files for NPM package 'my-package'"
}
```

## Cargo API

### Registry Configuration

#### Get Registry Config
Returns Cargo registry configuration.

```http
GET /cargo/config.json
```

**Response**:
```json
{
  "dl": "http://localhost:3080/cargo/api/v1/crates/{crate}/{version}/download",
  "api": "http://localhost:3080/cargo/api/v1/"
}
```

### Package Operations

#### Get Index File
Serves Cargo index files containing crate metadata.

```http
GET /cargo/{path}
```

**Parameters**:
- `path` (string): Index file path (e.g., "cr/at/crate")

**Response**: Newline-delimited JSON with crate versions
```json
{"name":"crate-name","vers":"1.0.0","deps":[],"cksum":"abcd...","features":{}}
{"name":"crate-name","vers":"1.1.0","deps":[],"cksum":"efgh...","features":{}}
```

#### Download Crate
Downloads a Cargo crate file.

```http
GET /cargo/api/v1/crates/{crate}/{version}/download
```

**Parameters**:
- `crate` (string): Crate name
- `version` (string): Crate version

**Response**: Binary crate data (.crate file)

#### Publish Crate
Publishes a new Cargo crate.

```http
PUT /cargo/api/v1/crates/new
```

**Content-Type**: `application/octet-stream`

**Body**: Binary crate data

**Response**:
```json
{
  "message": "Crate published successfully",
  "status": "success"
}
```

#### Delete Crate Version
Deletes a specific crate version.

```http
DELETE /api/cargo/{crate_name}/{version}
```

**Parameters**:
- `crate_name` (string): Crate name
- `version` (string): Version to delete

**Response**:
```json
{
  "message": "Deleted crate version successfully"
}
```

#### Delete All Crate Versions
Completely removes a crate and all its versions.

```http
DELETE /api/cargo/crate/{crate_name}
```

**Parameters**:
- `crate_name` (string): Crate name

**Response**:
```json
{
  "message": "Deleted all versions of crate successfully"
}
```

## Management API

### Server Information

#### Get Package List
Returns all packages across all ecosystems.

```http
GET /api/packages
```

**Response**:
```json
{
  "pypi": ["package1", "package2"],
  "npm": ["package3", "package4"],
  "cargo": ["crate1", "crate2"]
}
```

#### Get Server Status
Returns server status and statistics.

```http
GET /api/status
```

**Response**:
```json
{
  "status": "running",
  "server_addr": "http://0.0.0.0:3080",
  "data_dir": "./data",
  "version": "0.1.0"
}
```

### Setup and Configuration

#### Get Setup Script
Returns a shell script for configuring client package managers.

```http
GET /setup.sh
```

**Response**: Shell script for client configuration

## UI Endpoints

### Web Interface

#### Home Page
Web interface showing package statistics and recent packages.

```http
GET /
```

#### Package Lists
Browse packages by type.

```http
GET /ui/{pkg_type}
```

**Parameters**:
- `pkg_type` (string): Package type ("pypi", "npm", or "cargo")

#### Package Details
View detailed information about a specific package.

```http
GET /ui/pypi/{pkg_name}
GET /ui/npm/{pkg_name}
GET /ui/cargo/{pkg_name}
```

#### Upload Page
Web form for uploading packages.

```http
GET /upload
```

## Error Handling

### HTTP Status Codes

- `200 OK` - Request successful
- `400 Bad Request` - Invalid request parameters
- `404 Not Found` - Package or resource not found
- `413 Payload Too Large` - Upload size exceeds limits
- `500 Internal Server Error` - Server error

### Error Response Format

```json
{
  "error": {
    "code": "NOT_FOUND",
    "message": "Package not found: example-package"
  }
}
```

### Common Error Codes

- `NOT_FOUND` - Resource not found
- `BAD_REQUEST` - Invalid request
- `UPLOAD_ERROR` - File upload failed
- `VALIDATION_ERROR` - Request validation failed
- `INTERNAL_ERROR` - Server internal error

## Response Formats

### Success Response
Standard success response for operations.

```json
{
  "message": "Operation completed successfully",
  "status": "success"
}
```

### Package Metadata
Each package ecosystem has its own metadata format following the respective registry API specifications.

### Binary Responses
Package files, tarballs, and crates are returned as binary data with appropriate Content-Type headers.

## Rate Limiting

Currently, no rate limiting is implemented. All endpoints accept unlimited requests.

## Client Configuration

The server automatically configures client package managers when started:

- **Pip**: Points to `/pypi/simple/`
- **NPM**: Points to `/npm/`
- **Cargo**: Points to `/cargo/`

Use the `/setup.sh` endpoint to get configuration scripts for remote machines.

## Upstream Fallback

The server supports transparent fallback to upstream registries:

- **PyPI**: Falls back to `https://pypi.org/`
- **NPM**: Falls back to `https://registry.npmjs.org/`
- **Cargo**: Falls back to `https://crates.io/`

When a package is not found locally, the server attempts to fetch it from the upstream registry and serve it transparently.

---

For more detailed information about specific endpoints, refer to the inline documentation in the source code modules (`pypi.rs`, `npm.rs`, `cargo.rs`).