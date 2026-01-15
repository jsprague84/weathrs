# Project Instructions

## MCP Usage

Always use Context7 MCP when working with library/API documentation, code generation, setup, or configuration steps. Fetch up-to-date documentation for any Rust crates (axum, tokio, reqwest, serde, etc.) without requiring explicit "use context7" prompts.

## Project Overview

This is a Rust weather API application using OpenWeatherMap. The goal is to create a streamlined, well-written, focused application for learning Rust.

### Tech Stack
- **Framework**: Axum
- **Async Runtime**: Tokio
- **HTTP Client**: reqwest
- **Serialization**: serde + serde_json
- **Configuration**: config or figment
- **Scheduling**: tokio-cron-scheduler
- **Logging**: tracing + tracing-subscriber

### Deployment
- Docker container
- Traefik reverse proxy
- Future: Android application expansion
